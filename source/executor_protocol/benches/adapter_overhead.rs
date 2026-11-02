// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What one adapter that passes each call down costs, composed with an
//! executor made with [`executor_fn`], against that executor alone, in
//! nanoseconds per instruction per shot.
//!
//! The executor holds a state per shot ([`OnePerShot`]), eight bits, and
//! runs a typed instruction ([`Gate`]): a flip, or a read under a tag. It
//! does little work of its own, so the adapter's cost is a large share of
//! its time. Each column calls its executor directly with a batch on 1 or
//! 10 shots, of 1 or 100 instructions, then waits for the reply, as a
//! driver does; each shot runs [`INSTRUCTIONS`] / shots instructions.
//!
//! The columns:
//!
//! - **bare**: the executor alone, the baseline.
//! - **by hand**: an adapter written by hand that passes each call down as
//!   it is ([`Passing`], [`Apply`]).
//! - **one state**: an [`adapter_fn`] whose logic sends each call down
//!   ([`pass`]), with one state.
//! - **per shot**: the same with a state per shot
//!   (`.shots(OnePerShot)`), whose shots' equal sends are merged back into
//!   one call below.
//!
//! Before any timing, the bench asserts that every column gives the bare
//! executor's outcomes at every size. It asserts no timing: the numbers
//! depend on the machine and its load, so compare the columns of one run,
//! not runs on different machines.
//!
//! Run it pinned to one core where the platform allows (on Linux,
//! `taskset -c <core>` before the command):
//! `cargo bench -p executor_protocol --bench adapter_overhead`.

use std::hint::black_box;
use std::time::Instant;

use executor_protocol::{
    Adapter, AdapterError, Apply, Below, CallInput, Capabilities, Close, Describe, Executor,
    InstructionBatch, InstructionBatchRef, OnePerShot, Outcome, Outcomes, ShotContext, ShotOutcome,
    ShotSet, TypedInstruction, Value, adapter_fn, executor_fn, wait,
};

/// The instructions each column runs, over all its shots.
const INSTRUCTIONS: usize = 100_000;
/// The runs of each measurement in a round.
const RUNS: usize = 15;
/// The rounds, each measuring every column in turn, so that a change of
/// the machine's load reaches all columns; the fastest run of all rounds
/// is reported.
const ROUNDS: usize = 3;

/// A typed instruction of the executor: flip a bit, or read it under a
/// tag. Equal sends of the shots of a per-shot adapter are merged, so it
/// compares.
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

/// The executor: a state of eight bits per shot.
fn bits() -> impl Executor<InstructionBatch<Gate>> + Describe + Close {
    let state = |_: ShotContext| Ok(0_u8);
    let logic = |bits: &mut u8, input: &CallInput<'_, Gate>, outcomes: &mut Outcomes<'_>| {
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

/// An adapter written by hand that passes each call down as it is.
struct Passing;

impl Adapter for Passing {
    type Accepts = Gate;
    type Sends = Gate;
}

impl<E: Executor<InstructionBatch<Gate>> + Close> Apply<E> for Passing {
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// The logic of an [`adapter_fn`] that sends each call down as it is.
fn pass<S>(
    _: &mut S,
    input: &CallInput<'_, Gate>,
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

fn main() {
    println!(
        "ns per instruction per shot, the fastest of {} runs ({ROUNDS} rounds of {RUNS}) of \
         {INSTRUCTIONS} instructions",
        ROUNDS * RUNS
    );
    println!(
        "{:>24} {:>8} {:>8} {:>10} {:>9} {:>14} {:>16} {:>15}",
        "",
        "bare",
        "by hand",
        "one state",
        "per shot",
        "by hand / bare",
        "one state / bare",
        "per shot / bare"
    );
    for count in [1_u32, 10] {
        let shots = ShotSet::range(0..=count - 1).expect("not empty");
        let per_shot = INSTRUCTIONS / count as usize;
        let program: Vec<Gate> = (0..per_shot).map(gate_at).collect();
        for size in [1, 100] {
            let mut bare = bits();
            let mut by_hand =
                executor_protocol::compose(Passing, bits()).expect("it passes what it gets");
            let mut one_state = executor_protocol::compose(adapter_fn(|_| Ok(()), pass), bits())
                .expect("it passes what it gets");
            let mut per_shot_adapter =
                executor_protocol::compose(adapter_fn(|_| Ok(()), pass).shots(OnePerShot), bits())
                    .expect("it passes what it gets");
            let mut columns: [Column<'_>; 4] = [
                &mut |b| outcomes(&mut bare, &shots, b),
                &mut |b| outcomes(&mut by_hand, &shots, b),
                &mut |b| outcomes(&mut one_state, &shots, b),
                &mut |b| outcomes(&mut per_shot_adapter, &shots, b),
            ];
            let check: Vec<Gate> = (0..size.max(2)).map(gate_at).collect();
            let [expected, rest @ ..] = &mut columns;
            let expected = expected(&check);
            for column in rest {
                assert_eq!(column(&check), expected, "every column reads the same");
            }
            let mut fastest = [f64::INFINITY; 4];
            for _ in 0..ROUNDS {
                for (column, fastest) in columns.iter_mut().zip(&mut fastest) {
                    let time = per_instruction(&mut **column, &program, size, count as usize);
                    *fastest = fastest.min(time);
                }
            }
            let [bare, by_hand, one_state, per_shot] = fastest;
            println!(
                "{:>24} {bare:>8.1} {by_hand:>8.1} {one_state:>10.1} {per_shot:>9.1} {:>14.2} \
                 {:>16.2} {:>15.2}",
                format!("{count} shots, batches of {size}"),
                by_hand / bare,
                one_state / bare,
                per_shot / bare
            );
        }
    }
}
