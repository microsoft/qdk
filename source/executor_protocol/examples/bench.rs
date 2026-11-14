// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the protocol costs, per instruction and per `execute`, against a
//! driver that calls its executor itself:
//!
//! - D: one driver on four chains, batches of 1 to 10 000 instructions;
//! - C: the checks of each `execute`, and building an instruction by name;
//! - F: a lowering adapter, each logical instruction sent as three physical;
//! - W: an executor that answers from a worker thread: the time of a call.
//!
//! The executors only count what they get, so the time is the protocol's
//! and the parts' own. Each variant is checked on every run: it must count
//! what the program holds (or, in F, answer what the direct call answers,
//! and its composing must compose). It runs once untimed before it is
//! timed, and keeps the best of 7. W has no untimed run, and keeps the best
//! of up to 5 runs' median call. Run it with
//! `cargo run --release --example bench [D|C|F|W]`.

use std::fmt::Debug;
use std::future::poll_fn;
use std::hint::black_box;
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Poll, Waker};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use executor_protocol::{
    Adapt, Adapter, Argument, Arguments, Batch, BatchReadouts, BlockType, Capabilities,
    CompositionFaults, Drive, DriveError, Driver, Execute, ExecuteError, Executor, Instruction,
    InstructionSet, Needs, Opcode, OpcodeMap, ReadoutValue, Readouts, Runner, Seed, ShotConditions,
    ShotSet, instruction_set,
};

instruction_set! {
    /// The gates of the mix: QIR's, without measurements, whose draws cost
    /// the same on every chain.
    pub MixGates {
        CCX = ccx(qubit, qubit, qubit),
        CX = cx(qubit, qubit),
        CZ = cz(qubit, qubit),
        RZ = rz(qubit; theta: Number),
        H = h(qubit),
        S = s(qubit),
        T = t(qubit),
        T_ADJ = t_adj(qubit),
        X = x(qubit),
        SWAP = swap(qubit, qubit),
        RESET = reset(qubit),
    }
}

/// The mix: each gate of `MixGates`, its qubits and its weight, as in QIR
/// programs (two-qubit gates and single-qubit rotations most). A gate's
/// place here is its place in a [`Profile`]'s counts.
const MIX: [(Opcode, usize, u64); 11] = [
    (MixGates::CX, 2, 30),
    (MixGates::H, 1, 15),
    (MixGates::RZ, 1, 15),
    (MixGates::T, 1, 10),
    (MixGates::T_ADJ, 1, 5),
    (MixGates::S, 1, 5),
    (MixGates::X, 1, 5),
    (MixGates::CZ, 2, 5),
    (MixGates::CCX, 3, 3),
    (MixGates::RESET, 1, 2),
    (MixGates::SWAP, 2, 5),
];

/// The qubits the mix draws from.
const QUBITS: u32 = 100;

/// The seed of the mix's draws, and of every run, whose executors draw
/// nothing.
const SEED: u64 = 1;

/// How much each section runs.
struct Size {
    /// The mix's instructions, for D, C and W.
    instructions: usize,
    /// The timed runs of each variant, of which the best is kept.
    repeats: usize,
    /// F's logical rounds, of 6 instructions each.
    rounds: usize,
    /// W's most calls per batch size.
    most_calls: usize,
}

/// The size of the tables, which `main` runs.
const FULL: Size = Size {
    instructions: 4_000_000,
    repeats: 7,
    rounds: 2_000,
    most_calls: 20_000,
};

/// A size at which every variant runs and agrees in a debug test.
#[cfg(test)]
const TINY: Size = Size {
    instructions: 3_000,
    repeats: 1,
    rounds: 4,
    most_calls: 10,
};

/// The sections, by the names `main` takes.
const SECTIONS: [&str; 4] = ["D", "C", "F", "W"];

fn main() {
    let only = std::env::args().nth(1);
    if let Some(only) = &only
        && !SECTIONS.contains(&only.as_str())
    {
        eprintln!("no section {only:?}: the sections are D, C, F and W");
        std::process::exit(2);
    }
    run(&FULL, only.as_deref());
}

/// Runs the sections, or only the one named `only`.
fn run(size: &Size, only: Option<&str>) {
    let wants = |section: &str| only.is_none_or(|only| only == section);
    println!(
        "{} instructions of a seeded gate mix, best of {}; an instruction is {} bytes",
        size.instructions,
        size.repeats,
        size_of::<Instruction>()
    );
    if wants("D") || wants("C") || wants("W") {
        let draws = draws(size.instructions);
        let program = build(&draws);
        if wants("D") {
            chains_bench(size, &program);
        }
        if wants("C") {
            checks_bench(size, &draws, &program);
        }
        if wants("W") {
            worker_bench(size, &program);
        }
    }
    if wants("F") {
        lowering_bench(size);
    }
}

// --------------------------------------------------------------- D: chains

/// One driver on each chain under measure, over the same counting executor.
#[derive(Clone, Copy)]
enum Chain {
    /// The driver calls the executor itself, past the protocol: the baseline.
    Direct,
    /// `Driver >> Executor`: one boundary, so one check of each `execute`.
    Bare,
    /// `Driver >> Adapter >> Executor`, an adapter that forwards.
    OneAdapter,
    /// `Driver >> Adapter >> Adapter >> Executor`, both forwarding.
    TwoAdapters,
    /// `Bare`, with the executor offering `max_instructions` this many.
    Limited(u32),
}

/// D: each chain, per batch size, in ns per instruction and, beside each
/// chain, its cost per `execute` over the direct call.
fn chains_bench(size: &Size, program: &[Instruction]) {
    println!("\nD. one driver on four chains, ns/instr (overhead per call vs direct)");
    println!(
        "  ≫P = Driver ≫ Executor(profiler); ≫F≫P with an adapter that forwards; ≫F≫F≫P with two"
    );
    println!(
        "  {:>7} {:>8} {:>20} {:>20} {:>20}",
        "batch", "direct", "≫P", "≫F≫P", "≫F≫F≫P"
    );
    let expected = Profile::of(program);
    for batch in [1, 10, 100, 1_000, 10_000] {
        let time = |chain| best(size.repeats, &expected, || run_chain(program, batch, chain));
        let direct = time(Chain::Direct);
        let calls = program.len().div_ceil(batch);
        let cell = |t: f64| {
            format!(
                "{:>8.2} ({:>7.1} ns)",
                per(t, program.len()),
                per(t - direct, calls)
            )
        };
        println!(
            "  {batch:>7} {:>8.2} {:>20} {:>20} {:>20}",
            per(direct, program.len()),
            cell(time(Chain::Bare)),
            cell(time(Chain::OneAdapter)),
            cell(time(Chain::TwoAdapters))
        );
    }
}

/// Sends `program` down `chain`, `batch` instructions per `execute`, and
/// returns what the executor counted.
fn run_chain(program: &[Instruction], batch: usize, chain: Chain) -> Profile {
    let mut profiler = Profiler::new(Capabilities::unlimited());
    let in_batches = ProgramInBatches::new(MixGates::set(), program, QUBITS, batch);
    let received = match chain {
        Chain::Direct => {
            // The chain's executor is sent no batch: `Direct` sends past it.
            let unused = Profiler::new(Capabilities::unlimited());
            run_once(Driver::new(Direct(in_batches, &mut profiler)) >> Executor::new(unused))
        }
        Chain::Bare => run_once(Driver::new(in_batches) >> Executor::new(&mut profiler)),
        Chain::OneAdapter => run_once(
            Driver::new(in_batches) >> Adapter::new(Forward::new()) >> Executor::new(&mut profiler),
        ),
        Chain::TwoAdapters => run_once(
            Driver::new(in_batches)
                >> Adapter::new(Forward::new())
                >> Adapter::new(Forward::new())
                >> Executor::new(&mut profiler),
        ),
        Chain::Limited(most) => {
            profiler.offer = Capabilities::unlimited().max_instructions(most);
            run_once(Driver::new(in_batches) >> Executor::new(&mut profiler))
        }
    };
    black_box(received);
    profiler.profile
}

// ------------------------------------------------------------- C: checks

/// C: what the checks of each `execute` cost, by difference, since they run
/// inside the protocol; and building an instruction by name, which checks
/// it too. The difference also holds the rest of a boundary's cost per
/// call (the answer's shape check, the catch of a panic, one `Checked`
/// call), negligible at batch 10 000.
fn checks_bench(size: &Size, draws: &[Draw], program: &[Instruction]) {
    println!("\nC. the checks, ns/instr (per call, where given)");
    let expected = Profile::of(program);
    let time = |batch, chain| best(size.repeats, &expected, || run_chain(program, batch, chain));
    let n = program.len();
    let large = 10_000;
    let checked = time(large, Chain::Bare) - time(large, Chain::Direct);
    println!(
        "  each execute's checks, batch {large} (≫P minus direct)          {:>6.2}",
        per(checked, n)
    );
    let most = u32::try_from(large).expect("a small limit");
    for batch in [1, large] {
        let limited = time(batch, Chain::Limited(most)) - time(batch, Chain::Bare);
        println!(
            "  max_instructions {most} offered, batch {batch:>5} (minus unlimited) {:>6.2} ({:.1} ns)",
            per(limited, n),
            per(limited, n.div_ceil(batch))
        );
    }
    // Into one buffer, reused, so that the row times building, not
    // allocating and freeing a program of instructions (their size printed
    // above).
    let set = MixGates::set();
    let mut buffer = Vec::with_capacity(BUILD_BUFFER);
    let built = best(size.repeats, &program.len(), || {
        let mut count = 0;
        for some in draws.chunks(BUILD_BUFFER) {
            buffer.clear();
            buffer.extend(some.iter().map(|draw| build_one(&set, draw)));
            count += black_box(&buffer).len();
        }
        count
    });
    let last = &program[program.len() - buffer.len()..];
    assert!(
        buffer == last,
        "the buffer holds the program's last instructions"
    );
    println!(
        "  building each instruction by name (InstructionSet::instruction) {:>6.2}",
        per(built, n)
    );
}

/// The instructions C builds into its buffer at a time.
const BUILD_BUFFER: usize = 10_000;

// ----------------------------------------------------------- F: lowering

instruction_set! {
    /// A code's logical instructions, each logical qubit a block of three
    /// physical ones.
    pub LogicalGates {
        PREPARE_Z = prepare_z(qubit),
        X = x(qubit),
        CX = cx(qubit, qubit),
        MEASURE_Z = measure_z(qubit) -> 1 outcome,
    }
}

instruction_set! {
    /// The physical instructions the lowering sends.
    pub PhysicalGates {
        RESET = reset(qubit),
        X = x(qubit),
        CX = cx(qubit, qubit),
        MZ = mz(qubit) -> 1 outcome,
    }
}

/// F: a lowering adapter's cost per physical instruction, against the
/// physical program sent directly to the same executor.
fn lowering_bench(size: &Size) {
    let logical_gate = |mnemonic: &str, qubits: &[u32]| {
        LogicalGates::set()
            .instruction(mnemonic, qubits.iter().copied(), &[])
            .expect("an instruction of LogicalGates")
    };
    // A logical round: two qubits prepared, one flipped, copied, both read.
    let round = [
        logical_gate("prepare_z", &[0]),
        logical_gate("prepare_z", &[1]),
        logical_gate("x", &[0]),
        logical_gate("cx", &[0, 1]),
        logical_gate("measure_z", &[0]),
        logical_gate("measure_z", &[1]),
    ];
    let logical: Vec<Instruction> = round
        .iter()
        .cycle()
        .take(6 * size.rounds)
        .cloned()
        .collect();
    // The physical program, as the lowering sends it, for the baseline.
    let mut recorder = Sink::recording();
    run_once(
        Driver::new(ProgramInBatches::new(
            LogicalGates::set(),
            &logical,
            2,
            logical.len(),
        )) >> Adapter::new(Repetition3::new())
            >> Executor::new(&mut recorder),
    );
    let physical = recorder.record.expect("a recording sink");
    let compose = compose_time(size.repeats, || {
        (
            Driver::new(ProgramInBatches::new(LogicalGates::set(), &logical, 2, 6)),
            Adapter::new(Repetition3::new()),
            Executor::new(Sink::new()),
        )
    });
    println!(
        "\nF. lowering, repetition code of 3 over a sink, {} logical -> {} physical instructions; composing the chain {:.1} µs",
        logical.len(),
        physical.len(),
        compose * 1e6
    );
    println!(
        "  {:>12} {:>16} {:>16} {:>16} {:>22}",
        "logical/call", "physical direct", "physical ≫sink", "lowered", "overhead per physical"
    );
    let direct_answer = sink_direct(&physical, 3 * 6);
    let lowered_answer = (direct_answer.0, majority_of_threes(&direct_answer.1));
    let n = physical.len();
    for batch in [6, 60, 600, 6_000] {
        let direct = best(size.repeats, &direct_answer, || {
            sink_direct(&physical, 3 * batch)
        });
        let bare = best(size.repeats, &direct_answer, || {
            let mut sink = Sink::new();
            let received = run_once(
                Driver::new(ProgramInBatches::new(
                    PhysicalGates::set(),
                    &physical,
                    6,
                    3 * batch,
                )) >> Executor::new(&mut sink),
            );
            (sink.seen, received.values)
        });
        let lowered = best(size.repeats, &lowered_answer, || {
            let mut sink = Sink::new();
            let received = run_once(
                Driver::new(ProgramInBatches::new(
                    LogicalGates::set(),
                    &logical,
                    2,
                    batch,
                )) >> Adapter::new(Repetition3::new())
                    >> Executor::new(&mut sink),
            );
            (sink.seen, received.values)
        });
        println!(
            "  {batch:>12} {:>13.2} ns {:>13.2} ns {:>13.2} ns {:>19.2} ns",
            per(direct, n),
            per(bare, n),
            per(lowered, n),
            per(lowered - direct, n)
        );
    }
}

/// The best time, in seconds, of `repeats` compositions of the three parts
/// `parts` builds, after an untimed one: only the `>>`s are timed, not
/// building the parts.
fn compose_time<'p>(
    repeats: usize,
    mut parts: impl FnMut() -> (
        Driver<ProgramInBatches<'p>>,
        Adapter<Repetition3>,
        Executor<Sink>,
    ),
) -> f64 {
    (0..=repeats)
        .map(|_| {
            let (driver, adapter, executor) = parts();
            let start = Instant::now();
            let chain = black_box(driver >> adapter >> executor);
            let elapsed = start.elapsed().as_secs_f64();
            chain.map(drop).expect("the chain composes");
            elapsed
        })
        .skip(1)
        .fold(f64::INFINITY, f64::min)
}

/// The physical program sent straight to a sink, `batch` per `execute`:
/// the instructions the sink saw, and its readouts.
fn sink_direct(physical: &[Instruction], batch: usize) -> (u64, Vec<ReadoutValue>) {
    let mut sink = Sink::new();
    let in_batches = ProgramInBatches::new(PhysicalGates::set(), physical, 6, batch);
    let received =
        run_once(Driver::new(Direct(in_batches, &mut sink)) >> Executor::new(Sink::new()));
    (sink.seen, received.values)
}

/// Each three values' majority, as the lowering reads a logical qubit.
fn majority_of_threes(values: &[ReadoutValue]) -> Vec<ReadoutValue> {
    values.chunks(3).map(majority).collect()
}

fn majority(three: &[ReadoutValue]) -> ReadoutValue {
    let ones = three.iter().filter(|&&v| v == ReadoutValue::One).count();
    if 2 * ones > three.len() {
        ReadoutValue::One
    } else {
        ReadoutValue::Zero
    }
}

/// A toy lowering: the repetition code of three, each logical qubit `q` the
/// physical qubits `3q`, `3q + 1` and `3q + 2`, each logical instruction the
/// same physical one on each, and a logical readout the majority of three.
struct Repetition3 {
    logical: InstructionSet,
    physical: InstructionSet,
    /// The physical opcodes, in the receiver's, from `bind`.
    opcodes: [Opcode; 4],
    /// Each batch's physical instructions, kept to reuse their memory.
    lowered: Vec<Vec<Instruction>>,
    /// The physical readouts, kept to reuse their memory.
    physical_readouts: Readouts,
}

impl Repetition3 {
    fn new() -> Self {
        Self {
            logical: LogicalGates::set(),
            physical: PhysicalGates::set(),
            opcodes: [
                PhysicalGates::RESET,
                PhysicalGates::X,
                PhysicalGates::CX,
                PhysicalGates::MZ,
            ],
            lowered: Vec::new(),
            physical_readouts: Readouts::new(),
        }
    }
}

impl Adapt for Repetition3 {
    fn accepts(&self) -> &InstructionSet {
        &self.logical
    }

    fn sends(&self) -> &InstructionSet {
        &self.physical
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.opcodes = self.opcodes.map(|opcode| below.map(opcode));
    }

    fn needs_below(&self, above: Needs) -> Needs {
        let qubits = 3 * above.blocks_of(&BlockType::qubit());
        let mut below = above;
        below.blocks.insert(BlockType::qubit(), qubits);
        below
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        let mut above = below;
        if let Some(Some(most)) = above.max_blocks.get_mut(&BlockType::qubit()) {
            *most /= 3;
        }
        above.max_instructions = above.max_instructions.map(|most| most / 3);
        above
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        self.lowered.resize_with(batches.len(), Vec::new);
        for (batch, lowered) in batches.iter().zip(&mut self.lowered) {
            lowered.clear();
            for instruction in batch.instructions {
                lower(instruction, self.opcodes, lowered);
            }
        }
        let sent: Vec<Batch<'_>> = batches
            .iter()
            .zip(&self.lowered)
            .map(|(batch, lowered)| Batch {
                shots: batch.shots,
                instructions: lowered,
            })
            .collect();
        self.physical_readouts.clear();
        below
            .execute(&sent, seed, &mut self.physical_readouts)
            .await?;
        for batch in &self.physical_readouts {
            read_logical(batch, readouts);
        }
        Ok(())
    }
}

/// Appends `instruction`'s three physical instructions to `lowered`.
fn lower(instruction: &Instruction, opcodes: [Opcode; 4], lowered: &mut Vec<Instruction>) {
    let [reset, x, cx, mz] = opcodes;
    let opcode = match instruction.opcode {
        LogicalGates::PREPARE_Z => reset,
        LogicalGates::X => x,
        LogicalGates::CX => cx,
        LogicalGates::MEASURE_Z => mz,
        _ => unreachable!("the protocol checked: an instruction of LogicalGates"),
    };
    for i in 0..3 {
        lowered.push(Instruction {
            opcode,
            blocks: instruction.blocks.iter().map(|q| 3 * q + i).collect(),
            arguments: Arguments::new(),
            select: None,
        });
    }
}

/// The logical readouts of a batch's physical ones: each three's majority.
fn read_logical(physical: &BatchReadouts, readouts: &mut Readouts) {
    let logical = readouts.push_with_capacity(physical.shots(), physical.readouts() / 3);
    for shot in 0..physical.shots() {
        let row = majority_of_threes(physical.row(shot));
        logical
            .push_row(&row, physical.conditions(shot))
            .expect("a third of the physical readouts");
    }
}

/// An executor of `PhysicalGates` that only counts what it gets, answering
/// each `mz` by its place in the stream, so that two chains sending the same
/// stream get the same readouts; it may record the stream.
struct Sink {
    set: InstructionSet,
    /// The instructions seen so far.
    seen: u64,
    record: Option<Vec<Instruction>>,
}

impl Sink {
    fn new() -> Self {
        Self {
            set: PhysicalGates::set(),
            seen: 0,
            record: None,
        }
    }

    fn recording() -> Self {
        Self {
            record: Some(Vec::new()),
            ..Self::new()
        }
    }
}

impl Execute for Sink {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Capabilities {
        Capabilities::unlimited()
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        _seed: Seed,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        let mut row = Vec::new();
        for batch in batches {
            row.clear();
            for instruction in batch.instructions {
                self.seen += 1;
                if instruction.opcode == PhysicalGates::MZ {
                    row.push(ReadoutValue::from(self.seen % 5 < 2));
                }
            }
            if let Some(record) = &mut self.record {
                record.extend_from_slice(batch.instructions);
            }
            let width = u32::try_from(row.len()).expect("a batch's readouts fit a u32");
            let rows = readouts.push_with_capacity(batch.shots.len(), width);
            for _ in 0..batch.shots.len() {
                rows.push_row(&row, ShotConditions::NONE)
                    .expect("one value per `mz`");
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------- W: worker

/// W: the time of a call to an executor that answers from a worker thread,
/// beside the same call answered on this thread. Each call's time holds a
/// clock read before and after it, whose cost the section prints, so the
/// columns compare above that floor (batch 1 is near it).
fn worker_bench(size: &Size, program: &[Instruction]) {
    let repeats = size.repeats.min(5);
    println!("\nW. an executor answering from a worker thread, median call, µs, best of {repeats}");
    println!(
        "  each call's time holds two clock reads, {:.2} µs here (an empty pair's median)",
        clock_floor()
    );
    println!(
        "  {:>7} {:>7} {:>14} {:>14}",
        "batch", "calls", "worker", "this thread"
    );
    for batch in [1, 100, 10_000, 100_000] {
        let program = &program[..program.len().min(batch * size.most_calls)];
        let expected = Profile::of(program);
        let worker = median_call(repeats, || {
            let mut worker = FromWorker::spawn();
            let mut in_batches = ProgramInBatches::new(MixGates::set(), program, QUBITS, batch);
            in_batches.timed = true;
            let received = run_once(Driver::new(in_batches) >> Executor::new(&mut worker));
            assert_eq!(worker.finish(), expected, "the worker counted the program");
            received.call_times
        });
        let here = median_call(repeats, || {
            let mut profiler = Profiler::new(Capabilities::unlimited());
            let mut in_batches = ProgramInBatches::new(MixGates::set(), program, QUBITS, batch);
            in_batches.timed = true;
            let received = run_once(Driver::new(in_batches) >> Executor::new(&mut profiler));
            assert_eq!(
                profiler.profile, expected,
                "the profiler counted the program"
            );
            received.call_times
        });
        let calls = program.len().div_ceil(batch);
        println!("  {batch:>7} {calls:>7} {worker:>14.2} {here:>14.2}");
    }
}

/// The median time, in µs, of a clock read before and after nothing: what
/// each call's time in W holds beside the call.
fn clock_floor() -> f64 {
    let mut times: Vec<Duration> = (0..10_001)
        .map(|_| black_box(Instant::now()).elapsed())
        .collect();
    times.sort_unstable();
    times[times.len() / 2].as_secs_f64() * 1e6
}

/// The best, over `repeats` runs, of a run's median call, in µs.
fn median_call(repeats: usize, mut run: impl FnMut() -> Vec<Duration>) -> f64 {
    (0..repeats)
        .map(|_| {
            let mut times = run();
            times.sort_unstable();
            times[times.len() / 2].as_secs_f64() * 1e6
        })
        .fold(f64::INFINITY, f64::min)
}

/// An executor of `MixGates` whose worker thread counts each batch, from a
/// copy, and hands the answer back, waking the run's waker.
struct FromWorker {
    set: InstructionSet,
    /// Each `execute`'s batches, copied, to the worker; closed by `finish`.
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<Profile>>,
    handover: Arc<Mutex<Handover>>,
}

/// An `execute`'s batches, copied: each one's shots and instructions.
type Job = Vec<(ShotSet, Vec<Instruction>)>;

/// The worker's answer, once ready, and the waker to wake then.
#[derive(Default)]
struct Handover {
    answer: Option<Vec<BatchReadouts>>,
    waker: Option<Waker>,
}

impl FromWorker {
    fn spawn() -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let handover = Arc::new(Mutex::new(Handover::default()));
        let worker = {
            let handover = Arc::clone(&handover);
            thread::spawn(move || {
                let mut profile = Profile::default();
                for job in inbox {
                    let answer = job
                        .iter()
                        .map(|(shots, instructions)| {
                            profile.record(instructions);
                            no_readouts(shots.len())
                        })
                        .collect();
                    let mut handover = handover.lock().expect("no panic while held");
                    handover.answer = Some(answer);
                    if let Some(waker) = handover.waker.take() {
                        waker.wake();
                    }
                }
                profile
            })
        };
        Self {
            set: MixGates::set(),
            jobs: Some(jobs),
            worker: Some(worker),
            handover,
        }
    }

    /// Stops the worker and returns what it counted.
    fn finish(mut self) -> Profile {
        drop(self.jobs.take());
        let worker = self.worker.take().expect("finished once");
        worker.join().expect("the worker ends")
    }
}

impl Execute for FromWorker {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Capabilities {
        Capabilities::unlimited()
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        _seed: Seed,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        let job = batches
            .iter()
            .map(|batch| (batch.shots.clone(), batch.instructions.to_vec()))
            .collect();
        let jobs = self.jobs.as_ref().expect("the worker runs until `finish`");
        jobs.send(job)
            .expect("the worker takes jobs until `finish`");
        let answer = poll_fn(|context| {
            let mut handover = self.handover.lock().expect("no panic while held");
            if let Some(answer) = handover.answer.take() {
                Poll::Ready(answer)
            } else {
                handover.waker = Some(context.waker().clone());
                Poll::Pending
            }
        })
        .await;
        for batch in answer {
            readouts.push(batch);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- parts

/// A driver that sends its program below, `batch` instructions per
/// `execute`, one shot, and returns every readout and, when `timed`, the
/// time of each call.
struct ProgramInBatches<'p> {
    set: InstructionSet,
    /// The map `bind` must get, the set's own: the program goes below as
    /// it is, so every chain gets the same instructions.
    identity: OpcodeMap,
    program: &'p [Instruction],
    qubits: u32,
    batch: usize,
    timed: bool,
}

/// What [`ProgramInBatches`] got back.
struct Received {
    values: Vec<ReadoutValue>,
    call_times: Vec<Duration>,
}

impl<'p> ProgramInBatches<'p> {
    fn new(set: InstructionSet, program: &'p [Instruction], qubits: u32, batch: usize) -> Self {
        let identity = set.fits(&set).expect("a set fits itself");
        Self {
            set,
            identity,
            program,
            qubits,
            batch,
            timed: false,
        }
    }

    /// Sends the program to `target`.
    async fn send(
        &self,
        target: &mut impl Execute,
        shots: &ShotSet,
        seed: Seed,
    ) -> Result<Received, ExecuteError> {
        let mut received = Received {
            values: Vec::new(),
            call_times: Vec::new(),
        };
        let mut readouts = Readouts::new();
        for chunk in self.program.chunks(self.batch) {
            let start = self.timed.then(Instant::now);
            let batches = [Batch {
                shots,
                instructions: black_box(chunk),
            }];
            readouts.clear();
            target.execute(&batches, seed, &mut readouts).await?;
            if let Some(start) = start {
                received.call_times.push(start.elapsed());
            }
            received.values.extend_from_slice(readouts[0].row(0));
        }
        Ok(received)
    }
}

impl Drive for ProgramInBatches<'_> {
    type Output = Received;

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        assert!(below == self.identity, "the receiver's set is the driver's");
    }

    fn needs(&self) -> Needs {
        Needs::qubits(self.qubits)
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<Received>, DriveError> {
        let received = self.send(below, shots, seed).await?;
        Ok(vec![received])
    }
}

/// [`ProgramInBatches`] sending to an executor of its own, past the chain
/// it is composed with, awaiting each `execute` itself: the baseline. A
/// driver, because only a run gives a seed to call `execute` with.
struct Direct<'p, E>(ProgramInBatches<'p>, E);

impl<E: Execute> Drive for Direct<'_, E> {
    type Output = Received;

    fn sends(&self) -> &InstructionSet {
        self.0.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    fn needs(&self) -> Needs {
        self.0.needs()
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        _chain: &mut impl Execute,
    ) -> Result<Vec<Received>, DriveError> {
        let Self(in_batches, executor) = self;
        let received = in_batches.send(executor, shots, seed).await?;
        // Its executor is past the chain, so the runner does not end the
        // run's shots on it.
        executor.end_shots(shots);
        Ok(vec![received])
    }
}

/// An adapter that forwards every batch as it is: the cost of a boundary.
struct Forward {
    set: InstructionSet,
    identity: OpcodeMap,
}

impl Forward {
    fn new() -> Self {
        let set = MixGates::set();
        let identity = set.fits(&set).expect("a set fits itself");
        Self { set, identity }
    }
}

impl Adapt for Forward {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        assert!(
            below == self.identity,
            "the receiver's set is the adapter's"
        );
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        below.execute(batches, seed, readouts).await
    }
}

/// An executor of `MixGates` that counts each opcode and the highest qubit:
/// as cheap an executor as one that reads every instruction can be.
struct Profiler {
    set: InstructionSet,
    offer: Capabilities,
    profile: Profile,
}

/// What a [`Profiler`] counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Profile {
    /// Each gate's count, in `MIX`'s order.
    counts: [u64; MIX.len()],
    top_qubit: Option<u32>,
}

impl Profiler {
    fn new(offer: Capabilities) -> Self {
        Self {
            set: MixGates::set(),
            offer,
            profile: Profile::default(),
        }
    }
}

impl Profile {
    /// What a profiler counts of `program`.
    fn of(program: &[Instruction]) -> Self {
        let mut profile = Self::default();
        profile.record(program);
        profile
    }

    fn record(&mut self, instructions: &[Instruction]) {
        for instruction in instructions {
            self.counts[gate_index(instruction.opcode)] += 1;
            if let Some(&top) = instruction.blocks.iter().max() {
                self.top_qubit = Some(self.top_qubit.map_or(top, |was| was.max(top)));
            }
        }
    }
}

/// A gate's place in `MIX`. A match, since an opcode's index is the
/// protocol's own, and a search of `MIX` would compare each opcode in turn
/// on the profiler's hot path.
const fn gate_index(opcode: Opcode) -> usize {
    match opcode {
        MixGates::CX => 0,
        MixGates::H => 1,
        MixGates::RZ => 2,
        MixGates::T => 3,
        MixGates::T_ADJ => 4,
        MixGates::S => 5,
        MixGates::X => 6,
        MixGates::CZ => 7,
        MixGates::CCX => 8,
        MixGates::RESET => 9,
        MixGates::SWAP => 10,
        _ => panic!("the protocol checked: an instruction of MixGates"),
    }
}

// `gate_index` gives each gate of `MIX` its place there, checked when the
// bench is built.
const _: () = {
    let mut place = 0;
    while place < MIX.len() {
        assert!(gate_index(MIX[place].0) == place);
        place += 1;
    }
};

impl Execute for Profiler {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Capabilities {
        self.offer.clone()
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        _seed: Seed,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        for batch in batches {
            self.profile.record(batch.instructions);
            readouts.push(no_readouts(batch.shots.len()));
        }
        Ok(())
    }
}

/// The answer to a batch without readouts: an empty row per shot.
fn no_readouts(shots: usize) -> BatchReadouts {
    let mut rows = BatchReadouts::with_capacity(shots, 0);
    for _ in 0..shots {
        rows.push_row(&[], ShotConditions::NONE)
            .expect("a row of no readouts");
    }
    rows
}

// --------------------------------------------------------------- harness

/// Runs the composed chain on one shot, and returns its driver's output.
fn run_once<D: Drive<Output = Received>, E: Execute>(
    chain: Result<Runner<D, E>, CompositionFaults>,
) -> Received {
    let mut runner = chain.expect("the chain composes");
    let mut outputs = runner
        .run(&ShotSet::ids([0]), SEED)
        .expect("the chain runs one shot");
    outputs.pop().expect("one shot's output")
}

/// The best time, in seconds, of `repeats` runs of `sample`, each checked
/// to give `expected`, after an untimed run also checked.
fn best<T: PartialEq + Debug>(repeats: usize, expected: &T, mut sample: impl FnMut() -> T) -> f64 {
    assert_eq!(&sample(), expected, "the variant agrees before it is timed");
    (0..repeats)
        .map(|_| {
            let start = Instant::now();
            let got = black_box(sample());
            let elapsed = start.elapsed().as_secs_f64();
            assert_eq!(&got, expected, "the variant agrees when timed");
            elapsed
        })
        .fold(f64::INFINITY, f64::min)
}

/// `seconds` per item, in ns.
#[expect(clippy::cast_precision_loss, reason = "a count below 2^52, so exact")]
fn per(seconds: f64, items: usize) -> f64 {
    seconds * 1e9 / items as f64
}

/// One instruction of the mix, drawn: its gate in `MIX`, its qubits (the
/// first ones its gate takes, all different) and its angle.
struct Draw {
    gate: usize,
    qubits: [u32; 3],
    theta: f64,
}

/// `n` draws of the mix, from a fixed seed.
fn draws(n: usize) -> Vec<Draw> {
    let total: u64 = MIX.iter().map(|&(_, _, weight)| weight).sum();
    let mut stream = Stream(SEED);
    (0..n)
        .map(|_| {
            let mut pick = stream.next() % total;
            let gate = MIX
                .iter()
                .position(|&(_, _, weight)| {
                    let here = pick < weight;
                    pick = pick.wrapping_sub(weight);
                    here
                })
                .expect("a pick below the total weight");
            let mut qubits = [0; 3];
            for k in 0..MIX[gate].1 {
                // A block once per instruction: the protocol refuses a repeat.
                qubits[k] = loop {
                    let qubit = stream.below(QUBITS);
                    if !qubits[..k].contains(&qubit) {
                        break qubit;
                    }
                };
            }
            let theta = f64::from(stream.below(1 << 30)) / f64::from(1u32 << 30);
            Draw {
                gate,
                qubits,
                theta,
            }
        })
        .collect()
}

/// The program of `draws`, each instruction built and checked by name.
fn build(draws: &[Draw]) -> Vec<Instruction> {
    let set = MixGates::set();
    draws.iter().map(|draw| build_one(&set, draw)).collect()
}

/// The instruction of `draw`, built and checked by name in `set`,
/// `MixGates`.
fn build_one(set: &InstructionSet, draw: &Draw) -> Instruction {
    let (opcode, arity, _) = MIX[draw.gate];
    let mnemonic = set.mnemonic(opcode).expect("a gate of MixGates");
    let theta = [("theta", Argument::from(draw.theta))];
    let arguments: &[(&str, Argument)] = if opcode == MixGates::RZ { &theta } else { &[] };
    set.instruction(mnemonic, draw.qubits[..arity].iter().copied(), arguments)
        .expect("an instruction of MixGates")
}

/// `SplitMix64`: the mix's draws, without a crate beyond the protocol.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A draw below `n`.
    fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next() % u64::from(n)).expect("a draw below a u32")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every section, at a tiny size: each variant must count, or answer,
    /// what the direct call does, so the tables compare the same work.
    #[test]
    fn every_variant_does_the_same_work_at_a_tiny_size() {
        run(&TINY, None);
    }
}
