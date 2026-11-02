// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What shot conditions cost once raised, in nanoseconds per instruction
//! per shot: an executor made with [`executor_fn`] that raises one on its
//! shot, alone and under an [`adapter_fn`] that sends each call down, against
//! the same with none raised.
//!
//! The executor is `benches/adapter_overhead.rs`'s: a state per shot
//! ([`OnePerShot`]), eight bits, and a flip or a read under a tag per
//! instruction. Each column calls its executor directly with a batch on 1
//! or 10 shots, of 1 or 100 instructions, then waits for the reply; each
//! shot runs [`INSTRUCTIONS`] / shots instructions.
//!
//! The columns:
//!
//! - **bare**: the executor alone, raising none; the baseline.
//! - **bare, every shot**: the executor alone, raising
//!   [`ShotConditions::REJECTED`] on its shot in every call
//!   ([`Outcomes::raise_conditions`]).
//! - **adapter**: the executor under an [`adapter_fn`] whose logic sends
//!   each call down, with one state, none raised.
//! - **adapter, shot 0**: the same, the executor raising on shot 0 in
//!   every call, so that the adapter holds a condition for one shot of
//!   each call, and looks up the others' in every call.
//! - **adapter, every shot**: the same, the executor raising on every shot
//!   in every call.
//!
//! Then it reports the size of an outcome, with its conditions and as its
//! other fields would make it without them, and the memory an adapter
//! holds for the shots flagged so far, which it keeps until it is dropped:
//! the bytes a counting allocator sees still allocated after one call of
//! [`adapter_fn`] over an executor raising on every shot of the call, less
//! the same with none raised, per shot.
//!
//! Before any timing, the bench asserts that each column's outcomes carry
//! the conditions it raises, on the shots it raises them on, and no
//! other. It asserts no timing: compare the columns of one run.
//!
//! Run it pinned to one core where the platform allows (on Linux,
//! `taskset -c <core>` before the command):
//! `cargo bench -p executor_protocol --bench shot_conditions`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Instant;

use executor_protocol::{
    AdapterError, Below, Capabilities, Close, Describe, Executor, InstructionBatch,
    InstructionBatchRef, OnePerShot, Outcome, Outcomes, ProgramInput, ShotConditions, ShotContext,
    ShotId, ShotOutcome, ShotSet, TypedInstruction, Value, adapter_fn, executor_fn, wait,
};

/// The instructions each column runs, over all its shots.
const INSTRUCTIONS: usize = 100_000;
/// The runs of each measurement in a round.
const RUNS: usize = 15;
/// The rounds, each measuring every column in turn, so that a change of
/// the machine's load reaches all columns; the fastest run of all rounds
/// is reported.
const ROUNDS: usize = 3;

/// The system allocator, counting the bytes still allocated while
/// [`COUNTING`] is set: not while the columns are timed.
struct Counting;

/// Whether [`Counting`] counts.
static COUNTING: AtomicBool = AtomicBool::new(false);
/// The bytes [`Counting`] has allocated and not yet freed while counting.
static LIVE: AtomicIsize = AtomicIsize::new(0);

/// Adds `bytes` to [`LIVE`] while [`COUNTING`] is set.
fn count(bytes: isize) {
    if COUNTING.load(Ordering::Relaxed) {
        LIVE.fetch_add(bytes, Ordering::Relaxed);
    }
}

/// `bytes` as a count of [`LIVE`].
fn signed(bytes: usize) -> isize {
    isize::try_from(bytes).expect("an allocation is below isize::MAX bytes")
}

// SAFETY: every call is passed on to `System` as it is; the count is a
// side record.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(signed(layout.size()));
        // SAFETY: the caller's contract, passed on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(-signed(layout.size()));
        // SAFETY: the caller's contract, passed on.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(signed(new_size) - signed(layout.size()));
        // SAFETY: the caller's contract, passed on.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// A typed instruction of the executor: flip a bit, or read it under a
/// tag.
#[derive(Clone, Copy, PartialEq)]
enum Gate {
    Flip(u32),
    Read(u32, u32),
}

impl TypedInstruction for Gate {}

/// The `i`-th instruction of a batch: a flip, then a read of the same bit
/// under the tag `i`, so that a batch's tags are distinct.
fn gate_at(i: usize) -> Gate {
    let bit = u32::try_from(i / 2 % 8).expect("fewer than 8 bits");
    let tag = u32::try_from(i).expect("fewer than 2^32 instructions");
    if i.is_multiple_of(2) {
        Gate::Flip(bit)
    } else {
        Gate::Read(bit, tag)
    }
}

/// Which shots the executor raises a condition on.
#[derive(Clone, Copy)]
enum Raising {
    None,
    Shot0,
    Every,
}

impl Raising {
    /// Whether it raises one on `shot`.
    fn on(self, shot: ShotId) -> bool {
        match self {
            Raising::None => false,
            Raising::Shot0 => shot == 0,
            Raising::Every => true,
        }
    }
}

/// The executor: a state of eight bits per shot, raising
/// [`ShotConditions::REJECTED`] in every call on the shots of `raising`.
fn bits(raising: Raising) -> impl Executor<InstructionBatch<Gate>> + Describe + Close {
    let state = |context: ShotContext| Ok((context.shot(), 0_u8));
    let logic = move |(shot, bits): &mut (ShotId, u8),
                      input: &ProgramInput<'_, Gate>,
                      outcomes: &mut Outcomes<'_>| {
        if raising.on(*shot) {
            outcomes.raise_conditions(ShotConditions::REJECTED);
        }
        for &gate in input.instructions() {
            match gate {
                Gate::Flip(bit) => *bits ^= 1 << bit,
                Gate::Read(bit, tag) => {
                    outcomes.push(Outcome::new(tag, Value::from(*bits >> bit & 1 == 1)));
                }
            }
        }
        Ok(())
    };
    executor_fn(Capabilities::default(), state, logic).shots(OnePerShot)
}

/// The logic of an [`adapter_fn`] that sends each call down as it is.
fn pass(
    (): &mut (),
    input: &ProgramInput<'_, Gate>,
    below: &mut Below<'_, Gate>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// The outcomes of `batch` on `shots` of `executor`.
fn outcomes<E: Executor<InstructionBatch<Gate>>>(
    executor: &mut E,
    shots: &ShotSet,
    batch: &[Gate],
) -> Vec<ShotOutcome> {
    let reply = executor
        .call(InstructionBatchRef {
            shots,
            instructions: batch,
        })
        .expect("the batch is valid");
    let mut out = Vec::new();
    wait(reply, &mut out).expect("the bits never fail");
    out
}

/// A column: its executor's batch on the shots.
type Column<'a> = &'a mut dyn FnMut(&[Gate]) -> Vec<ShotOutcome>;

/// The fastest of [`RUNS`] runs of `program` in calls of `size`
/// instructions, in nanoseconds per instruction per shot.
#[allow(
    clippy::cast_precision_loss,
    reason = "a time and a count of instructions, far below 2^52"
)]
fn per_instruction(column: Column<'_>, program: &[Gate], size: usize, shots: usize) -> f64 {
    let fastest = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            for batch in program.chunks(size) {
                black_box(column(black_box(batch)));
            }
            start.elapsed()
        })
        .min()
        .expect("RUNS is not 0");
    fastest.as_secs_f64() * 1e9 / (program.len() * shots) as f64
}

/// Asserts that `out` reads the same as `expected`, each outcome carrying
/// [`ShotConditions::REJECTED`] exactly when `raising` raises on its shot.
fn assert_marked(out: &[ShotOutcome], expected: &[ShotOutcome], raising: Raising) {
    assert_eq!(out.len(), expected.len(), "every column reads the same");
    for (outcome, unmarked) in out.iter().zip(expected) {
        let conditions = if raising.on(outcome.shot) {
            ShotConditions::REJECTED
        } else {
            ShotConditions::NONE
        };
        assert_eq!(
            *outcome,
            unmarked.with_conditions(conditions),
            "an outcome carries its own shot's conditions alone"
        );
    }
}

/// The bytes an adapter over `bits(raising)` still holds after one call
/// on `count` shots, against what it held before.
fn held_after_call(raising: Raising, count: u32) -> isize {
    let shots = ShotSet::range(0..=count - 1).expect("not empty");
    let batch: Vec<Gate> = (0..2).map(gate_at).collect();
    let mut adapter = executor_protocol::compose(adapter_fn(|_| Ok(()), pass), bits(raising))
        .expect("it passes what it gets");
    LIVE.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    let out = outcomes(&mut adapter, &shots, &batch);
    assert_eq!(out.len(), count as usize, "one readout per shot");
    drop(out);
    COUNTING.store(false, Ordering::Relaxed);
    drop(adapter);
    LIVE.load(Ordering::Relaxed)
}

/// Prints the bytes an adapter holds per flagged shot.
#[allow(
    clippy::cast_precision_loss,
    reason = "counts of bytes and shots, far below 2^52"
)]
fn report_memory() {
    let without = (size_of::<ShotId>() + size_of::<u32>() + size_of::<Value>())
        .next_multiple_of(align_of::<ShotOutcome>());
    println!();
    println!(
        "bytes of an outcome: {} with its conditions, {without} without them",
        size_of::<ShotOutcome>()
    );
    println!();
    println!("bytes an adapter holds per flagged shot, after one call on every shot");
    println!(
        "{:>12} {:>14} {:>14} {:>10}",
        "shots", "none raised", "every shot", "per shot"
    );
    for count in [1_000_u32, 100_000, 1_000_000] {
        let none = held_after_call(Raising::None, count);
        let every = held_after_call(Raising::Every, count);
        let per_shot = (every - none) as f64 / f64::from(count);
        println!("{count:>12} {none:>14} {every:>14} {per_shot:>10.1}");
    }
}

fn main() {
    println!(
        "ns per instruction per shot, the fastest of {} runs ({ROUNDS} rounds of {RUNS}) of \
         {INSTRUCTIONS} instructions; ratios to the same with none raised",
        ROUNDS * RUNS
    );
    println!(
        "{:>24} {:>6} {:>12} {:>8} {:>15} {:>18} {:>12} {:>15} {:>18}",
        "",
        "bare",
        "every shot",
        "adapter",
        "adapter, shot 0",
        "adapter, every shot",
        "every / bare",
        "shot 0 / adapter",
        "every / adapter"
    );
    for count in [1_u32, 10] {
        let shots = ShotSet::range(0..=count - 1).expect("not empty");
        let per_shot = INSTRUCTIONS / count as usize;
        let program: Vec<Gate> = (0..per_shot).map(gate_at).collect();
        for size in [1, 100] {
            let mut bare = bits(Raising::None);
            let mut bare_every = bits(Raising::Every);
            let mut adapter =
                executor_protocol::compose(adapter_fn(|_| Ok(()), pass), bits(Raising::None))
                    .expect("it passes what it gets");
            let mut adapter_shot0 =
                executor_protocol::compose(adapter_fn(|_| Ok(()), pass), bits(Raising::Shot0))
                    .expect("it passes what it gets");
            let mut adapter_every =
                executor_protocol::compose(adapter_fn(|_| Ok(()), pass), bits(Raising::Every))
                    .expect("it passes what it gets");
            let raisings = [
                Raising::None,
                Raising::Every,
                Raising::None,
                Raising::Shot0,
                Raising::Every,
            ];
            let mut columns: [Column<'_>; 5] = [
                &mut |b| outcomes(&mut bare, &shots, b),
                &mut |b| outcomes(&mut bare_every, &shots, b),
                &mut |b| outcomes(&mut adapter, &shots, b),
                &mut |b| outcomes(&mut adapter_shot0, &shots, b),
                &mut |b| outcomes(&mut adapter_every, &shots, b),
            ];
            let check: Vec<Gate> = (0..size.max(2)).map(gate_at).collect();
            let [expected, rest @ ..] = &mut columns;
            let expected = expected(&check);
            for (column, &raising) in rest.iter_mut().zip(&raisings[1..]) {
                assert_marked(&column(&check), &expected, raising);
            }
            let mut fastest = [f64::INFINITY; 5];
            for _ in 0..ROUNDS {
                for (column, fastest) in columns.iter_mut().zip(&mut fastest) {
                    let time = per_instruction(&mut **column, &program, size, count as usize);
                    *fastest = fastest.min(time);
                }
            }
            let [bare, bare_every, adapter, adapter_first, every] = fastest;
            println!(
                "{:>24} {bare:>6.1} {bare_every:>12.1} {adapter:>8.1} {adapter_first:>15.1} \
                 {every:>18.1} {:>12.2} {:>15.2} {:>18.2}",
                format!("{count} shots, batches of {size}"),
                bare_every / bare,
                adapter_first / adapter,
                every / adapter
            );
        }
    }
    report_memory();
}
