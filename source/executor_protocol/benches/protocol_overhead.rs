// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the protocol adds to an executor's own work, per instruction: an
//! executor of instruction batches that holds it as each shot's state,
//! driven by a run, and the check of each batch against an instruction set
//! described at run time.
//!
//! The executor is a counting profiler: it counts each instruction by its
//! kind, tracks the highest qubit and draws a fair value for each
//! measurement, so it does little work of its own and the protocol's cost
//! is a large share of its time. It is called with batches of 1, 10, 100
//! and 10 000 instructions of one 4M-instruction gate mix, each batch
//! called and then waited for, as a driver does. It reports nanoseconds
//! per instruction, the fastest of 7 runs on one thread, and each column's
//! ratio to the first. The crate's `docs/performance.md` holds a table of
//! its results and what they mean.
//!
//! The columns:
//!
//! - **typed**: the profiler as an executor of [`Gate`], a Rust instruction
//!   type, called directly: the baseline.
//! - **typed per-shot**: that profiler as the state of each shot of an
//!   executor made with [`executor_fn`] under [`OnePerShot`], run on one
//!   shot by a run ([`compose`] of the program's driver, [`InChunks`],
//!   with the executor, given the program as a job, then
//!   [`Run::wait`](executor_protocol::Run::wait); [`per_shot_run`]): the
//!   executor makes the shot's state at its first call, and is closed at
//!   the end.
//! - **run-time**: the profiler as an executor of [`InstructionCall`]s of
//!   [`gate_set`], described at run time: it checks each batch against the
//!   set ([`InstructionSet::check_batch`]) and finds a call's readout
//!   through an [`OpcodeTable`], called directly.
//! - **run-time per-shot**: that profiler as the state of each shot of an
//!   executor made so, driven by a run whose program states the set in its
//!   needs, so the chain checks the set when given the job and passes the
//!   calls as they are, since the program numbers the set as the profiler
//!   does.
//! - **QEP-like**: the profiler as QEP's ([`QepCounting`]), called
//!   directly on one shot, with no protocol: it takes QEP's instruction
//!   ([`QepInstruction`]), finds a gate's counter by matching its name
//!   against QEP's instruction names, and gives its readouts in order,
//!   untagged.
//!
//! Every column but QEP-like checks that each batch's readout tags are
//! unique ([`check_unique_tags`]), as every executor that reports by tag
//! must; QEP reports by position.
//!
//! Before any timing, every column runs the mix at every batch size, and
//! the bench asserts that each gives the baseline's counts, highest qubit
//! and readout tags (QEP-like: its readouts' values); that the columns
//! called directly read the same values (they draw from one seed), as do
//! the two per-shot columns (they draw from the shot's seed); and that the
//! executor of each per-shot column took one call per batch. It asserts
//! no timing: the numbers depend on the machine and its load, so compare
//! the columns of one run, not runs on different machines.
//!
//! Run it pinned to one core where the platform allows (on Linux,
//! `taskset -c <core>` before the command):
//! `cargo bench -p executor_protocol --bench protocol_overhead`.

use std::collections::BTreeSet;
use std::f64::consts::FRAC_PI_4;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use executor_protocol::{
    Action, Angle, CallInput, Capabilities, Close, Driver, DriverExt, Executor, ExecutorReport,
    Fold, ImmediateReply, Instruction, InstructionBatch, InstructionBatchRef, InstructionCall,
    InstructionSet, Needs, OnePerShot, OpcodeTable, Operand, OperandKind, Outcome, Outcomes, Pauli,
    Rejected, ShotContext, ShotId, ShotOutcome, ShotSet, TypedInstruction, Value,
    check_unique_tags, compose, executor_fn, wait,
};

/// The instructions of the mix.
const INSTRUCTIONS: usize = 4_000_000;
/// The runs of each measurement; the fastest is reported.
const REPEATS: usize = 7;
/// The instructions per call.
const BATCHES: [usize; 4] = [1, 10, 100, 10_000];
/// The seed of the profilers called directly, and of the per-shot runs.
const SEED: u64 = 0;
/// The one shot each run runs.
const SHOT: u32 = 1;
/// The qubits the mix acts on.
const QUBITS: u32 = 100;

fn main() {
    let program = Program::new();
    let set = Arc::new(RuntimeSet::new(&Arc::new(gate_set())));
    println!(
        "Instruction sizes: Gate {} B, InstructionCall {} B, QepInstruction {} B",
        size_of::<Gate>(),
        size_of::<InstructionCall>(),
        size_of::<QepInstruction>()
    );
    for batch in BATCHES {
        assert_columns_agree(&program, &set, batch);
    }
    println!(
        "Every column gives the typed executor's counts, highest qubit and readouts;\n\
         each per-shot column took one call per batch.\n"
    );

    println!("ns per instruction, the fastest of {REPEATS} runs of {INSTRUCTIONS} instructions");
    println!(
        "| {:>6} | {:>6} | {:>14} | {:>13} | {:>17} | {:>13} |",
        "batch", "typed", "typed per-shot", "run-time", "run-time per-shot", "QEP-like"
    );
    println!(
        "| -----: | -----: | -------------: | ------------: | ----------------: | ------------: |"
    );
    for batch in BATCHES {
        let typed = best(|| {
            let mut profiler = TypedCounting::new(SHOT, SEED);
            drive(&mut profiler, &program.typed, batch);
            black_box(&profiler.counter.counts);
        });
        let typed_per_shot = best(|| {
            black_box(typed_run(&program.typed, batch));
        });
        let run_time = best(|| {
            let mut profiler = RuntimeCounting::new(&set, SHOT, SEED);
            drive(&mut profiler, &program.calls, batch);
            black_box(&profiler.counter.counts);
        });
        let run_time_per_shot = best(|| {
            black_box(runtime_run(&set, &program.calls, batch));
        });
        let qep = best(|| {
            let mut profiler = QepCounting::new(SEED);
            black_box(qep_drive(&mut profiler, &program.qep, batch));
            black_box(&profiler.counts);
        });
        let cell = |t: f64| format!("{:.2} ({:.2}×)", ns_per_instruction(t), t / typed);
        println!(
            "| {batch:>6} | {:>6.2} | {:>14} | {:>13} | {:>17} | {:>13} |",
            ns_per_instruction(typed),
            cell(typed_per_shot),
            cell(run_time),
            cell(run_time_per_shot),
            cell(qep)
        );
    }
    println!("(×: the time against the typed executor called directly)");
}

/// The seconds of the fastest of [`REPEATS`] runs of `f`.
fn best(mut f: impl FnMut()) -> f64 {
    (0..REPEATS)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

/// `seconds` for the whole mix, in nanoseconds per instruction.
#[allow(
    clippy::cast_precision_loss,
    reason = "INSTRUCTIONS is far below 2^52, so it is exact as an f64"
)]
fn ns_per_instruction(seconds: f64) -> f64 {
    seconds * 1e9 / INSTRUCTIONS as f64
}

/// Runs `program` on shot [`SHOT`] of `executor` in calls of `batch`
/// instructions, each called and then waited for, and returns the readouts.
fn drive<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    program: &[I],
    batch: usize,
) -> Vec<ShotOutcome> {
    const SHOTS: &ShotSet = &ShotSet::one(SHOT);
    let mut outcomes = Vec::new();
    for chunk in program.chunks(batch) {
        let reply = executor
            .call(InstructionBatchRef {
                shots: SHOTS,
                instructions: black_box(chunk),
            })
            .expect("the counting profiler refuses no batch of the mix");
        wait(reply, &mut outcomes).expect("the counting profiler does not fail");
    }
    outcomes
}

/// What a profiler counted over a run, to compare the columns: its counts by
/// instruction, its highest qubit and the tags of its readouts in order.
#[derive(Debug, PartialEq)]
struct Totals {
    counts: Vec<u64>,
    max_qubit: Option<u32>,
    readouts: Vec<u32>,
}

impl Totals {
    fn of(tally: Tally, outcomes: &[ShotOutcome]) -> Self {
        Self {
            counts: tally.counts,
            max_qubit: tally.max_qubit,
            readouts: outcomes.iter().map(|o| o.tag).collect(),
        }
    }
}

/// The values of `outcomes`, in order.
fn values_of(outcomes: &[ShotOutcome]) -> Vec<Value> {
    outcomes.iter().map(|o| o.value).collect()
}

/// `outcomes`, checked to be all of the one shot of a run.
fn of_the_shot(outcomes: &[ShotOutcome]) -> &[ShotOutcome] {
    assert!(outcomes.iter().all(|o| o.shot == SHOT), "{outcomes:?}");
    outcomes
}

/// Asserts that every column gives the baseline's [`Totals`] on `program`
/// in calls of `batch` instructions; that the columns called directly read
/// the same values, as do the two per-shot columns; and that the executor
/// of each per-shot column took one call per batch.
fn assert_columns_agree(program: &Program, set: &Arc<RuntimeSet>, batch: usize) {
    let calls = u64::try_from(program.typed.len().div_ceil(batch)).expect("the calls fit in a u64");

    let mut typed = TypedCounting::new(SHOT, SEED);
    let outcomes = drive(&mut typed, &program.typed, batch);
    let expected = Totals::of(typed.close(), &outcomes);
    let direct_values = values_of(&outcomes);

    let mut run_time = RuntimeCounting::new(set, SHOT, SEED);
    let outcomes = drive(&mut run_time, &program.calls, batch);
    assert_eq!(
        Totals::of(run_time.close(), &outcomes),
        expected,
        "run-time, batch {batch}"
    );
    assert_eq!(
        values_of(&outcomes),
        direct_values,
        "run-time's values, batch {batch}"
    );

    let mut qep = QepCounting::new(SEED);
    let readouts = qep_drive(&mut qep, &program.qep, batch);
    assert!(qep.unexpected.is_empty(), "QEP-like names every gate");
    let tally = qep.take();
    assert_eq!(
        (tally.counts, tally.max_qubit),
        (expected.counts.clone(), expected.max_qubit),
        "QEP-like, batch {batch}"
    );
    assert_eq!(
        readouts.into_iter().map(Value::from).collect::<Vec<_>>(),
        direct_values,
        "QEP-like's values, batch {batch}"
    );

    let (outcomes, report) = typed_run(&program.typed, batch);
    let outcomes = of_the_shot(&outcomes);
    let shot_values = values_of(outcomes);
    assert_eq!(
        Totals::of(report.states, outcomes),
        expected,
        "typed per-shot, batch {batch}"
    );
    assert_eq!(report.calls, calls, "typed per-shot's calls");

    let (outcomes, report) = runtime_run(set, &program.calls, batch);
    let outcomes = of_the_shot(&outcomes);
    assert_eq!(
        values_of(outcomes),
        shot_values,
        "run-time per-shot's values, batch {batch}"
    );
    assert_eq!(
        Totals::of(report.states, outcomes),
        expected,
        "run-time per-shot, batch {batch}"
    );
    assert_eq!(report.calls, calls, "run-time per-shot's calls");
}

/// A `SplitMix64` generator: the mix's draws and the profiler's, so that the
/// bench depends on no random number crate.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`.
    fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next() % u64::from(n)).expect("it is below n, a u32")
    }

    /// A fair bit.
    fn bit(&mut self) -> bool {
        self.next() >> 63 == 1
    }
}

/// The instructions of the mix, as a Rust type, in [`GATES`]' order. A
/// measurement names its readout by a tag.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gate {
    Cx(u32, u32),
    Cz(u32, u32),
    Swap(u32, u32),
    H(u32),
    S(u32),
    T(u32),
    X(u32),
    Rz(u32, f64),
    Reset(u32),
    Mz { qubit: u32, tag: u32 },
}

impl TypedInstruction for Gate {}

/// The instructions' mnemonics and operands, in [`Gate`]'s order, so that an
/// opcode of [`gate_set`] is the index of the gate's counter.
const GATES: [(&str, &[OperandKind]); 10] = {
    use OperandKind::{Block, Real, Tag};
    [
        ("cx", &[Block, Block]),
        ("cz", &[Block, Block]),
        ("swap", &[Block, Block]),
        ("h", &[Block]),
        ("s", &[Block]),
        ("t", &[Block]),
        ("x", &[Block]),
        ("rz", &[Block, Real]),
        ("reset", &[Block]),
        ("mz", &[Block, Tag]),
    ]
};

/// The mix: (mnemonic, weight in 100).
const MIX: [(&str, u32); 10] = [
    ("cx", 30),
    ("h", 15),
    ("rz", 15),
    ("t", 10),
    ("s", 5),
    ("x", 5),
    ("cz", 5),
    ("swap", 5),
    ("reset", 5),
    ("mz", 5),
];

impl Gate {
    /// Its index in [`GATES`].
    #[inline]
    fn id(self) -> usize {
        match self {
            Self::Cx(..) => 0,
            Self::Cz(..) => 1,
            Self::Swap(..) => 2,
            Self::H(_) => 3,
            Self::S(_) => 4,
            Self::T(_) => 5,
            Self::X(_) => 6,
            Self::Rz(..) => 7,
            Self::Reset(_) => 8,
            Self::Mz { .. } => 9,
        }
    }

    /// The highest qubit it acts on.
    #[inline]
    fn max_qubit(self) -> u32 {
        match self {
            Self::Cx(a, b) | Self::Cz(a, b) | Self::Swap(a, b) => a.max(b),
            Self::H(q)
            | Self::S(q)
            | Self::T(q)
            | Self::X(q)
            | Self::Rz(q, _)
            | Self::Reset(q)
            | Self::Mz { qubit: q, .. } => q,
        }
    }

    /// The tag of its readout, for a measurement.
    #[inline]
    fn readout(self) -> Option<u32> {
        match self {
            Self::Mz { tag, .. } => Some(tag),
            _ => None,
        }
    }

    /// As QEP passes it: by its name, with its qubits, then its angle.
    fn qep(self) -> QepInstruction {
        let (qubits, arity, angle) = match self {
            Self::Cx(a, b) | Self::Cz(a, b) | Self::Swap(a, b) => ([a, b, 0], 2, None),
            Self::H(q) | Self::S(q) | Self::T(q) | Self::X(q) | Self::Reset(q) => {
                ([q, 0, 0], 1, None)
            }
            Self::Mz { qubit, .. } => ([qubit, 0, 0], 1, None),
            Self::Rz(q, theta) => ([q, 0, 0], 1, Some(theta)),
        };
        QepInstruction {
            instruction_name: GATES[self.id()].0,
            targets: qubits.map(|q| usize::try_from(q).expect("a qubit fits in a usize")),
            target_count: arity,
            angle,
        }
    }

    /// Its operands, as [`gate_set`] declares them.
    fn operands(self) -> Vec<Operand> {
        use Operand::{Block, Real, Tag};
        match self {
            Self::Cx(a, b) | Self::Cz(a, b) | Self::Swap(a, b) => vec![Block(a), Block(b)],
            Self::H(q) | Self::S(q) | Self::T(q) | Self::X(q) | Self::Reset(q) => vec![Block(q)],
            Self::Rz(q, theta) => vec![Block(q), Real(theta)],
            Self::Mz { qubit, tag } => vec![Block(qubit), Tag(tag)],
        }
    }
}

/// The mix, as each executor is called with it.
struct Program {
    typed: Vec<Gate>,
    calls: Vec<InstructionCall>,
    qep: Vec<QepInstruction>,
}

impl Program {
    /// [`INSTRUCTIONS`] instructions drawn from [`MIX`] on [`QUBITS`]
    /// qubits, from a fixed seed, so every run times the same mix. The
    /// `k`-th measurement is tagged `k`.
    fn new() -> Self {
        let set = gate_set();
        let total: u32 = MIX.iter().map(|m| m.1).sum();
        let mut rng = SplitMix(1);
        let mut readouts = 0;
        let mut typed = Vec::with_capacity(INSTRUCTIONS);
        for _ in 0..INSTRUCTIONS {
            let mut pick = rng.below(total);
            let name = MIX
                .iter()
                .find(|m| {
                    if pick < m.1 {
                        true
                    } else {
                        pick -= m.1;
                        false
                    }
                })
                .expect("`pick` is below the weights' total")
                .0;
            let (a, b) = (rng.below(QUBITS), rng.below(QUBITS));
            typed.push(match name {
                "cx" => Gate::Cx(a, b),
                "cz" => Gate::Cz(a, b),
                "swap" => Gate::Swap(a, b),
                "h" => Gate::H(a),
                "s" => Gate::S(a),
                "t" => Gate::T(a),
                "x" => Gate::X(a),
                "rz" => Gate::Rz(a, f64::from(b) / f64::from(QUBITS)),
                "reset" => Gate::Reset(a),
                "mz" => {
                    readouts += 1;
                    Gate::Mz {
                        qubit: a,
                        tag: readouts - 1,
                    }
                }
                other => unreachable!("`{other}` is not of the mix"),
            });
        }
        let calls = typed
            .iter()
            .map(|&gate| {
                set.call(GATES[gate.id()].0, gate.operands())
                    .expect("the set declares each gate, as the mix calls it")
            })
            .collect();
        let qep = typed.iter().map(|&gate| gate.qep()).collect();
        Self { typed, calls, qep }
    }
}

/// [`Gate`] described at run time: the same instructions, in the same
/// order, each doing what [`action`] declares. The driver and the profiler
/// share this set, so they match by mnemonic, the actions checked to agree.
fn gate_set() -> InstructionSet {
    GATES
        .iter()
        .fold(InstructionSet::new("gates"), |set, &(name, operands)| {
            set.with_action(name, operands.iter().copied(), action(name))
        })
}

/// What gate `name` does, as Qodec declares it. A rotation about `P` by `θ`
/// is `exp(-iθP/2)`, so `t` rotates about `Z` by `π/4`, up to a global
/// phase. A measurement keeps its qubit.
fn action(name: &str) -> Action {
    let (x, y, z) = (Pauli::x, Pauli::y, Pauli::z);
    let one = || Action::new([1], [1]);
    let two = || Action::new([1, 1], [1, 1]);
    match name {
        "cx" => two().clifford([(x(0), x(0) * x(1)), (z(1), z(0) * z(1))]),
        "cz" => two().clifford([(x(0), x(0) * z(1)), (x(1), z(0) * x(1))]),
        "swap" => two().clifford([(x(0), x(1)), (z(0), z(1)), (x(1), x(0)), (z(1), z(0))]),
        "h" => one().clifford([(x(0), z(0)), (z(0), x(0))]),
        "s" => one().clifford([(x(0), y(0))]),
        "t" => one().rotate(z(0), Angle::Literal(FRAC_PI_4)),
        "x" => one().pauli(x(0)),
        "rz" => one().rotate(z(0), Angle::Parameter(0)),
        "reset" => Action::new([], [1]).stabilize([z(0)]),
        "mz" => one().observe([z(0)]),
        other => unreachable!("each gate has its action, and `{other}` is not one"),
    }
}

/// The profiler's state of one shot, whatever its instructions' type: its
/// shot, its counts by gate, the highest qubit, and its draws.
struct Counter {
    shot: ShotId,
    counts: Vec<u64>,
    max_qubit: Option<u32>,
    rng: SplitMix,
    /// The log probability of its draws so far, ln ½ each.
    log_p: f64,
}

impl Counter {
    fn new(shot: ShotId, seed: u64) -> Self {
        Self {
            shot,
            counts: vec![0; GATES.len()],
            max_qubit: None,
            rng: SplitMix(seed),
            log_p: 0.0,
        }
    }

    /// Counts gate `id`, which acts on qubits up to `max_qubit`.
    #[inline]
    fn count(&mut self, id: usize, max_qubit: u32) {
        self.max_qubit = Some(self.max_qubit.map_or(max_qubit, |c| c.max(max_qubit)));
        self.counts[id] += 1;
    }

    /// A fair draw, as the readout `tag`.
    fn read(&mut self, tag: u32) -> Outcome {
        self.log_p -= std::f64::consts::LN_2;
        Outcome::new(tag, self.rng.bit().into())
    }

    /// Its totals.
    fn take(&mut self) -> Tally {
        Tally {
            counts: std::mem::take(&mut self.counts),
            max_qubit: self.max_qubit.take(),
        }
    }

    /// Its totals, as a state's report: a copy.
    fn tally(&self) -> Tally {
        Tally {
            counts: self.counts.clone(),
            max_qubit: self.max_qubit,
        }
    }
}

/// The report of a profiler: its counts, by gate, and the highest qubit.
#[derive(Debug, Default)]
struct Tally {
    counts: Vec<u64>,
    max_qubit: Option<u32>,
}

impl Fold for Tally {
    fn fold(&mut self, other: Self) {
        if self.counts.len() < other.counts.len() {
            self.counts.resize(other.counts.len(), 0);
        }
        for (count, other) in self.counts.iter_mut().zip(other.counts) {
            *count += other;
        }
        self.max_qubit = self.max_qubit.max(other.max_qubit);
    }
}

/// The counting profiler as an executor of [`Gate`]s: dispatch by the
/// enum's variant.
struct TypedCounting {
    counter: Counter,
}

impl TypedCounting {
    fn new(shot: ShotId, seed: u64) -> Self {
        Self {
            counter: Counter::new(shot, seed),
        }
    }
}

/// The check of a batch of [`Gate`]s: its readout tags are unique.
fn check_gates(batch: &[Gate]) -> Result<(), Rejected> {
    check_unique_tags(
        batch
            .iter()
            .enumerate()
            .filter_map(|(at, gate)| Some((at, gate.readout()?))),
    )
}

/// A counting profiler, whatever its instructions' type: its counter.
trait Counting {
    fn counter(&self) -> &Counter;
}

/// A counting profiler's work on a batch of `I`, which its check accepted:
/// it counts each instruction and gives each readout to `push`.
trait Profile<I>: Counting {
    fn run(&mut self, batch: &[I], push: impl FnMut(Outcome));
}

impl Counting for TypedCounting {
    fn counter(&self) -> &Counter {
        &self.counter
    }
}

impl Profile<Gate> for TypedCounting {
    #[inline]
    fn run(&mut self, batch: &[Gate], mut push: impl FnMut(Outcome)) {
        for &gate in batch {
            self.counter.count(gate.id(), gate.max_qubit());
            if let Some(tag) = gate.readout() {
                push(self.counter.read(tag));
            }
        }
    }
}

impl Executor<InstructionBatch<Gate>> for TypedCounting {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, Gate>) -> Result<Self::Reply, Rejected> {
        let shot = self.counter.shot;
        batch.shots.check_only(shot)?;
        check_gates(batch.instructions)?;
        let mut reply = ImmediateReply::default();
        self.run(batch.instructions, |Outcome { tag, value, .. }| {
            reply.push(ShotOutcome::new(shot, tag, value));
        });
        Ok(reply)
    }
}

impl Close for TypedCounting {
    type Report = Tally;

    fn close(&mut self) -> Tally {
        self.counter.take()
    }
}

/// The run of `program` on shot [`SHOT`] of an executor made with
/// [`executor_fn`] of seed [`SEED`] under [`OnePerShot`]: shot `j`'s
/// profiler is made by `make` at the shot's first call, for `j` and the last
/// executor's seed in the shot; each call is checked by `check`. Gives its
/// outcomes and the executor's report, whose states' report is the
/// profilers' tallies, folded.
fn per_shot_run<I: Instruction, P: Profile<I>>(
    program: Chunked<'_, I>,
    capabilities: Capabilities,
    check: impl FnMut(&CallInput<'_, I>) -> Result<(), Rejected>,
    make: impl Fn(ShotId, u64) -> P,
) -> (Vec<ShotOutcome>, ExecutorReport<Tally>) {
    let state = move |context: ShotContext| Ok(make(context.shot(), context.seed()));
    let program_fn = |profiler: &mut P, input: &CallInput<'_, I>, outcomes: &mut Outcomes<'_>| {
        profiler.run(input.instructions(), |outcome| outcomes.push(outcome));
        Ok(())
    };
    let executor = executor_fn(capabilities, state, program_fn)
        .check(check)
        .log_probability(|profiler: &P| profiler.counter().log_p)
        .state_report(|profiler: &P| profiler.counter().tally())
        .seed(SEED)
        .shots(OnePerShot);
    let chain = compose(DriverExt::<Chunked<'_, I>>::into_chain(InChunks), executor)
        .expect("nothing between to refuse");
    let run = chain.run(program, ShotSet::one(SHOT));
    run.expect("the program's needs fit the profiler").wait()
}

/// The run of `program` on shot [`SHOT`] of an executor of
/// [`TypedCounting`]s, in calls of `batch` instructions ([`per_shot_run`]).
fn typed_run(program: &[Gate], batch: usize) -> (Vec<ShotOutcome>, ExecutorReport<Tally>) {
    let program = Chunked {
        program,
        batch,
        needs: Needs::default(),
    };
    per_shot_run(
        program,
        Capabilities::default().with_resumable_shots(true),
        |input| check_gates(input.instructions()),
        TypedCounting::new,
    )
}

/// A program: `program`, sent to the run's shots in calls of `batch`
/// instructions by its driver ([`InChunks`]); it needs `needs`.
struct Chunked<'a, I> {
    program: &'a [I],
    batch: usize,
    needs: Needs,
}

impl<I: Instruction> executor_protocol::Program for Chunked<'_, I> {
    type Instruction = I;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}

/// The driver of a [`Chunked`] program: it calls each chunk, then waits
/// for it.
struct InChunks;

impl<'a, I: Instruction> Driver<Chunked<'a, I>> for InChunks {
    /// `resumable_shots` when there is more than one chunk: each later one
    /// continues the shot.
    fn needs(&self, program: &Chunked<'a, I>) -> Needs {
        Needs::default().with_resumable_shots(program.program.len() > program.batch)
    }

    async fn drive<E: Executor<InstructionBatch<I>>>(
        &self,
        program: Chunked<'a, I>,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let mut outcomes = Vec::new();
        for batch in program.program.chunks(program.batch) {
            let batch = black_box(batch);
            let reply = (executor.call(InstructionBatchRef {
                shots,
                instructions: batch,
            }))
            .expect("the counting profiler refuses no batch of the mix");
            wait(reply, &mut outcomes).expect("the counting profiler does not fail");
        }
        outcomes
    }
}

/// What the run-time profiler knows of [`gate_set`]: the set, to check each
/// batch against, and, by opcode, the operand that holds the tag of the
/// instruction's readout, if any.
struct RuntimeSet {
    set: Arc<InstructionSet>,
    readouts: OpcodeTable<Option<usize>>,
}

impl RuntimeSet {
    fn new(set: &Arc<InstructionSet>) -> Self {
        Self {
            set: Arc::clone(set),
            readouts: set
                .table(|mnemonic| Some((mnemonic == "mz").then_some(1)))
                .expect("the handler has an entry for every mnemonic"),
        }
    }

    /// The tag of `call`'s readout, if it has one.
    #[inline]
    fn readout(&self, call: &InstructionCall) -> Option<u32> {
        let Operand::Tag(tag) = call.operands[self.readouts[call.opcode]?] else {
            unreachable!("`check_batch` checked the operand's kind against the set")
        };
        Some(tag)
    }

    /// The check of a batch: against the set, then its readout tags are
    /// unique.
    fn check(&self, batch: &[InstructionCall]) -> Result<(), Rejected> {
        self.set.check_batch(batch)?;
        check_unique_tags(
            batch
                .iter()
                .enumerate()
                .filter_map(|(at, call)| Some((at, self.readout(call)?))),
        )
    }
}

/// The counting profiler as an executor of [`InstructionCall`]s of
/// [`gate_set`]: it checks each batch against the set, counts a call by its
/// opcode and finds its readout's operand, if any, in a table by opcode.
struct RuntimeCounting {
    counter: Counter,
    set: Arc<RuntimeSet>,
}

impl RuntimeCounting {
    fn new(set: &Arc<RuntimeSet>, shot: ShotId, seed: u64) -> Self {
        Self {
            counter: Counter::new(shot, seed),
            set: Arc::clone(set),
        }
    }
}

impl Counting for RuntimeCounting {
    fn counter(&self) -> &Counter {
        &self.counter
    }
}

impl Profile<InstructionCall> for RuntimeCounting {
    #[inline]
    fn run(&mut self, batch: &[InstructionCall], mut push: impl FnMut(Outcome)) {
        for call in batch {
            let max_qubit = call
                .operands
                .iter()
                .filter_map(|o| match o {
                    Operand::Block(q) => Some(*q),
                    _ => None,
                })
                .max()
                .expect("each gate acts on a qubit");
            self.counter.count(call.opcode.index(), max_qubit);
            if let Some(tag) = self.set.readout(call) {
                push(self.counter.read(tag));
            }
        }
    }
}

impl Executor<InstructionBatch<InstructionCall>> for RuntimeCounting {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        let shot = self.counter.shot;
        batch.shots.check_only(shot)?;
        self.set.check(batch.instructions)?;
        let mut reply = ImmediateReply::default();
        self.run(batch.instructions, |Outcome { tag, value, .. }| {
            reply.push(ShotOutcome::new(shot, tag, value));
        });
        Ok(reply)
    }
}

impl Close for RuntimeCounting {
    type Report = Tally;

    fn close(&mut self) -> Tally {
        self.counter.take()
    }
}

/// The run of `program`, calls of `set`, on shot [`SHOT`] of an executor of
/// [`RuntimeCounting`]s, by a program that states `set` in its needs, in
/// calls of `batch` instructions ([`per_shot_run`]).
fn runtime_run(
    set: &Arc<RuntimeSet>,
    program: &[InstructionCall],
    batch: usize,
) -> (Vec<ShotOutcome>, ExecutorReport<Tally>) {
    let program = Chunked {
        program,
        batch,
        needs: Needs::default().with_instruction_set(Some(Arc::clone(&set.set))),
    };
    let capabilities = Capabilities::default()
        .with_resumable_shots(true)
        .with_instruction_set(Some(Arc::clone(&set.set)));
    let check_set = Arc::clone(set);
    let set = Arc::clone(set);
    per_shot_run(
        program,
        capabilities,
        move |input| check_set.check(input.instructions()),
        move |shot, seed| RuntimeCounting::new(&set, shot, seed),
    )
}

/// An instruction as QEP passes it to an executor
/// (`QuantumBackendInstruction`): named by a string, with its qubits and
/// its angle inline. QEP holds them in `SmallVec`s; with no crate, the
/// bench holds the qubits in an array and its length.
struct QepInstruction {
    instruction_name: &'static str,
    targets: [usize; 3],
    target_count: u8,
    #[expect(
        dead_code,
        reason = "QEP's profiler ignores it: it is here for QEP's instruction size"
    )]
    angle: Option<f64>,
}

impl QepInstruction {
    fn targets(&self) -> &[usize] {
        &self.targets[..usize::from(self.target_count)]
    }
}

/// QEP's `instruction_name_to_id`: the counter of each of its instructions,
/// by name.
fn qep_id(name: &str) -> Option<usize> {
    Some(match name {
        "ccx" => 0,
        "cx" => 1,
        "cy" => 2,
        "cz" => 3,
        "rx" => 4,
        "rxx" => 5,
        "ry" => 6,
        "ryy" => 7,
        "rz" => 8,
        "rzz" => 9,
        "h" => 10,
        "s" => 11,
        "sadj" => 12,
        "sx" => 13,
        "t" => 14,
        "tadj" => 15,
        "x" => 16,
        "y" => 17,
        "z" => 18,
        "swap" => 19,
        "reset" => 20,
        "mz_store" => 21,
        "mz_store_reset" => 22,
        "read_result" => 23,
        "mz" => 24,
        "mz_reset" => 25,
        _ => return None,
    })
}

/// The instructions [`qep_id`] names.
const QEP_INSTRUCTIONS: usize = 26;

/// QEP's `BatchResult`: the readouts of a batch, in order, the first held
/// inline, as QEP's `SmallVec<[_; 1]>` holds it.
#[derive(Default)]
struct BatchResult {
    first: Option<bool>,
    rest: Vec<bool>,
}

impl BatchResult {
    fn push(&mut self, value: bool) {
        if self.first.is_none() {
            self.first = Some(value);
        } else {
            self.rest.push(value);
        }
    }
}

/// The counting profiler as QEP's (`SequentialCountingProfiler`): its
/// counts by QEP's instruction ids, the names it does not know (QEP's
/// `FxHashSet`; never written in the mix), the highest qubit, and its
/// draws, from the bench's generator rather than QEP's thread's, so that
/// its values can be compared.
struct QepCounting {
    counts: Vec<u64>,
    unexpected: BTreeSet<&'static str>,
    max_qubit: Option<usize>,
    rng: SplitMix,
}

impl QepCounting {
    fn new(seed: u64) -> Self {
        Self {
            counts: vec![0; QEP_INSTRUCTIONS],
            unexpected: BTreeSet::new(),
            max_qubit: None,
            rng: SplitMix(seed),
        }
    }

    /// QEP's `execute_batch`: counts each instruction by its name, and
    /// draws one readout per `read_result`, `mz` or `mz_reset`.
    fn execute_batch(&mut self, instructions: &[QepInstruction]) -> BatchResult {
        let mut result = BatchResult::default();
        for instruction in instructions {
            if let Some(&qubit) = instruction.targets().iter().max() {
                self.max_qubit = Some(self.max_qubit.map_or(qubit, |c| c.max(qubit)));
            }
            if let Some(count) =
                qep_id(instruction.instruction_name).and_then(|id| self.counts.get_mut(id))
            {
                *count += 1;
            } else {
                self.unexpected.insert(instruction.instruction_name);
            }
            if matches!(
                instruction.instruction_name,
                "read_result" | "mz" | "mz_reset"
            ) {
                result.push(self.rng.bit());
            }
        }
        result
    }

    /// Its totals, by gate in [`GATES`]' order.
    fn take(&mut self) -> Tally {
        let counts = std::mem::take(&mut self.counts);
        Tally {
            counts: GATES
                .iter()
                .map(|&(name, _)| counts[qep_id(name).expect("QEP names every gate")])
                .collect(),
            max_qubit: (self.max_qubit.take())
                .map(|q| u32::try_from(q).expect("the qubits came from u32s")),
        }
    }
}

/// Runs `program` on `profiler` in calls of `batch` instructions, as QEP's
/// batcher calls it, and returns the readouts.
fn qep_drive(profiler: &mut QepCounting, program: &[QepInstruction], batch: usize) -> Vec<bool> {
    let mut readouts = Vec::new();
    for chunk in program.chunks(batch) {
        let result = profiler.execute_batch(black_box(chunk));
        readouts.extend(result.first);
        readouts.extend(result.rest);
    }
    readouts
}
