// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an instruction call costs, in nanoseconds per call.
//!
//! First, a set's own operations, by the set's size: the check of a call
//! ([`InstructionSet::check_batch`]), the lookup of a mnemonic, and the
//! building of a call by mnemonic ([`InstructionSet::call`]), which a
//! program or an adapter pays once.
//!
//! Then three executors of the same machine (one like the guides' toy:
//! eight bits and a coin), doing the same work: one of a Rust instruction
//! type, which the compiler checks; one of [`InstructionCall`]s, which it
//! checks and runs by opcode, through an [`OpcodeTable`]; and one of calls
//! that name their instruction by mnemonic, which it checks and runs by
//! name.
//!
//! Then the representations of [`InstructionCall`] and [`ImmediateReply`],
//! against holding the same in a `Vec`: a call's operands, held in the
//! call, on building, checking, renumbering and dispatching a batch; and a
//! reply's one outcome, held in the reply.
//!
//! Then what an executor costs per call when a program run on a chain of
//! it (`chain.run`) calls it, against the executor called directly: with
//! the program's set in the toy's numbering, as a program's calls are when
//! it builds them by name with the toy's set; and in another, into which
//! each batch is copied on its way.
//!
//! Then what an adapter composed with the toy's executor ([`compose`])
//! costs at the boundary downstream of it, against the toy's executor
//! alone: calls in the toy's numbering pass as they are; calls in another
//! are copied into it.
//!
//! Last, an adapter that sends a call of its own for each call, as a
//! lowering does: sending in another numbering, whose batches are copied
//! at the boundary, against adopting the toy's numbering when it is built
//! ([`AdapterBoundary::adopt_downstream_numbering`]), whose calls pass as they
//! are.
//!
//! Run with `cargo bench -p executor_protocol --bench instruction_calls`.
//! The numbers depend on the machine and its load, so this asserts only
//! that the variants measured agree, not the numbers: compare the rows of
//! one run, not runs on different machines.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::rc::Rc;
use std::sync::Arc;
use std::task::Waker;
use std::time::Instant;

use executor_protocol::{
    Action, Adapter, AdapterBoundary, AdapterError, AdaptiveProgram, Apply, Below, BelowHandle,
    Capabilities, Close, Describe, Executor, ImmediateReply, InstructionBatch, InstructionBatchRef,
    InstructionCall, InstructionSet, Needs, OnePerShot, Opcode, OpcodeMap, OpcodeTable, Operand,
    OperandKind, Outcomes, Pauli, ProgramInput, Rejected, Reply, ShotContext, ShotId, ShotOutcome,
    ShotSet, adapter_fn, compose, driver, owning_adapter_fn, wait,
};

/// The calls of each batch.
const CALLS: u32 = 10_000;
/// The runs of each measurement; the fastest is reported.
const RUNS: usize = 30;

fn main() {
    println!("ns per call, the fastest of {RUNS} runs of {CALLS} calls");
    sets();
    println!();
    executors();
    println!();
    representations();
    println!();
    replies();
    println!();
    renumbering();
    println!();
    composition();
    println!();
    merging();
    println!();
    adoption();
}

/// A set's own operations, by the set's size.
fn sets() {
    println!(
        "{:>12} {:>10} {:>18} {:>18}",
        "instructions", "check", "mnemonic lookup", "call by mnemonic"
    );
    for size in [5, 20, 50] {
        let set = set_of(size);
        let mnemonics: Vec<String> = set.mnemonics().map(str::to_owned).collect();
        let named: Vec<&str> = (0..CALLS as usize)
            .map(|i| mnemonics[i % size].as_str())
            .collect();
        let operands = || [Operand::Block(0), Operand::Tag(0)];
        let batch: Vec<InstructionCall> = named
            .iter()
            .map(|&m| set.call(m, operands()).expect("each call is of the set"))
            .collect();

        let check = per_call(|| {
            set.check_batch(black_box(&batch))
                .expect("each call is of the set");
        });
        let lookup = per_call(|| {
            for &m in &named {
                black_box(set.opcode(black_box(m)));
            }
        });
        let build = per_call(|| {
            for &m in &named {
                black_box(set.call(black_box(m), operands()).ok());
            }
        });
        println!("{size:>12} {check:>10.1} {lookup:>18.1} {build:>18.1}");
    }
}

/// A set of `size` instructions, each on a block and a tag.
fn set_of(size: usize) -> InstructionSet {
    (0..size).fold(InstructionSet::new("bench"), |set, i| {
        set.with(
            &format!("instruction_{i}"),
            [OperandKind::Block, OperandKind::Tag],
        )
    })
}

/// The three executors, on one batch of the toy's five instructions in
/// turn.
fn executors() {
    let set = toy_set();
    let ops: Vec<Op> = (0..CALLS).map(op_at).collect();
    let by_opcode: Vec<InstructionCall> = ops
        .iter()
        .map(|op| {
            let (mnemonic, operands) = op.named();
            set.call(mnemonic, operands)
                .expect("each call is of the set")
        })
        .collect();
    let by_mnemonic: Vec<NamedCall> = ops
        .iter()
        .map(|op| {
            let (mnemonic, operands) = op.named();
            NamedCall {
                mnemonic: mnemonic.to_owned(),
                operands,
            }
        })
        .collect();

    let fresh_by_mnemonic = || ByMnemonic {
        machine: Machine::default(),
        set: set.clone(),
    };
    let expected = outcomes(&mut Machine::default(), &ops);
    assert_eq!(
        outcomes(&mut ByOpcode::new(set.clone()), &by_opcode),
        expected
    );
    assert_eq!(outcomes(&mut fresh_by_mnemonic(), &by_mnemonic), expected);

    let mut typed = Machine::default();
    let typed = per_call(|| run(&mut typed, &ops));
    let mut opcodes = ByOpcode::new(set.clone());
    let opcodes = per_call(|| run(&mut opcodes, &by_opcode));
    let mut mnemonics = fresh_by_mnemonic();
    let mnemonics = per_call(|| run(&mut mnemonics, &by_mnemonic));

    println!(
        "{:>12} {:>10} {:>10} {:>10} {:>20}",
        "executor of", "Op", "opcodes", "mnemonics", "mnemonics / opcodes"
    );
    println!(
        "{:>12} {typed:>10.1} {opcodes:>10.1} {mnemonics:>10.1} {:>20.1}",
        "the toy",
        mnemonics / opcodes
    );
}

/// `instructions` on shot 0 alone.
fn on_shot_0<I>(instructions: &[I]) -> InstructionBatchRef<'_, I> {
    const SHOT_0: &ShotSet = &ShotSet::one(0);
    InstructionBatchRef {
        shots: SHOT_0,
        instructions,
    }
}

/// The outcomes of `batch` on shot 0 of `executor`.
fn outcomes<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    batch: &[I],
) -> Vec<ShotOutcome> {
    let reply = executor.call(on_shot_0(batch)).expect("the batch is valid");
    let mut out = Vec::new();
    wait(reply, &mut out).expect("the toy's batches end well");
    out
}

/// Runs `batch` on shot 0 of `executor`, and drops its outcomes.
fn run<I, E: Executor<InstructionBatch<I>>>(executor: &mut E, batch: &[I]) {
    let batch = on_shot_0(black_box(batch));
    black_box(executor.call(batch).expect("the batch is valid"));
}

/// The toy's instructions, as a type.
#[derive(Clone, Copy)]
enum Op {
    Coin(u32),
    X(u32),
    Cx(u32, u32),
    Measure { qubit: u32, tag: u32 },
    Reset(u32),
}

/// The `i`-th instruction of the batch: the five in turn, on the eight
/// qubits in turn. On qubit `q`, a coin, an `x`, a `cx` onto the next
/// qubit, a measure of that qubit, and a reset of `q`: what is measured
/// depends on each of the five, so the executors agree only when each runs
/// every instruction as the toy does.
fn op_at(i: u32) -> Op {
    let q = (i / 5) % 8;
    let next = (q + 1) % 8;
    match i % 5 {
        0 => Op::Coin(q),
        1 => Op::X(q),
        2 => Op::Cx(q, next),
        3 => Op::Measure {
            qubit: next,
            tag: i,
        },
        _ => Op::Reset(q),
    }
}

impl Op {
    /// Its mnemonic and operands, as [`toy_set`] describes it.
    fn named(self) -> (&'static str, Vec<Operand>) {
        let (mnemonic, operands, count) = self.parts();
        (mnemonic, operands[..count].to_vec())
    }

    /// Its mnemonic, and its operands: the first `count` of the array.
    fn parts(self) -> (&'static str, [Operand; 2], usize) {
        let none = Operand::Integer(0);
        match self {
            Op::Coin(q) => ("coin", [Operand::Block(q), none], 1),
            Op::X(q) => ("x", [Operand::Block(q), none], 1),
            Op::Cx(c, t) => ("cx", [Operand::Block(c), Operand::Block(t)], 2),
            Op::Measure { qubit, tag } => {
                ("measure", [Operand::Block(qubit), Operand::Tag(tag)], 2)
            }
            Op::Reset(q) => ("reset", [Operand::Block(q), none], 1),
        }
    }
}

/// The description of [`Op`], each instruction declaring its [`action`].
fn toy_set() -> InstructionSet {
    use OperandKind::{Block, Tag};
    InstructionSet::new("toy")
        .with_action("coin", [Block], action("coin"))
        .with_action("x", [Block], action("x"))
        .with_action("cx", [Block, Block], action("cx"))
        .with_action("measure", [Block, Tag], action("measure"))
        .with_action("reset", [Block], action("reset"))
}

/// What the toy's instruction `mnemonic` does, as Qodec declares it, so
/// that a set that numbers it otherwise is matched to it.
fn action(mnemonic: &str) -> Action {
    match mnemonic {
        "coin" => Action::new([], [1]).stabilize([Pauli::x(0)]),
        "x" => Action::new([1], [1]).pauli(Pauli::x(0)),
        "cx" => Action::new([1, 1], [1, 1]).clifford([
            (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
            (Pauli::z(1), Pauli::z(0) * Pauli::z(1)),
        ]),
        "measure" => Action::new([1], []).observe([Pauli::z(0)]),
        "reset" => Action::new([], [1]).stabilize([Pauli::z(0)]),
        other => unreachable!("the toy has no instruction `{other}`"),
    }
}

/// The toy's eight bits and its coin, of one shot, which each executor
/// runs on.
#[derive(Default)]
struct Machine {
    shot: ShotId,
    bits: [bool; 8],
    rng: u64,
}

impl Machine {
    fn apply(&mut self, op: Op, outcomes: &mut Vec<ShotOutcome>) {
        match op {
            Op::Coin(q) => {
                self.rng = self
                    .rng
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                self.bits[q as usize] = self.rng >> 63 == 1;
            }
            Op::X(q) => self.bits[q as usize] ^= true,
            Op::Cx(c, t) => self.bits[t as usize] ^= self.bits[c as usize],
            Op::Measure { qubit, tag } => outcomes.push(ShotOutcome::new(
                self.shot,
                tag,
                self.bits[qubit as usize].into(),
            )),
            Op::Reset(q) => self.bits[q as usize] = false,
        }
    }
}

impl Executor<InstructionBatch<Op>> for Machine {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        batch.shots.check_only(self.shot)?;
        let mut outcomes = Vec::new();
        for &op in batch.instructions {
            self.apply(op, &mut outcomes);
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

/// The [`Op`] of a call's operands, for one instruction of the toy.
type ToOp = fn(&[Operand]) -> Op;

/// The toy, accepting [`InstructionCall`]s: it checks a batch, then runs
/// each call through the entry of its opcode in its table, built from its
/// set once.
struct ByOpcode {
    machine: Machine,
    set: InstructionSet,
    table: OpcodeTable<ToOp>,
}

impl ByOpcode {
    fn new(set: InstructionSet) -> Self {
        let table = toy_table(&set);
        Self {
            machine: Machine::default(),
            set,
            table,
        }
    }
}

/// The table of the toy's instructions, by their opcodes in `set`.
fn toy_table(set: &InstructionSet) -> OpcodeTable<ToOp> {
    set.table(|mnemonic| -> Option<ToOp> {
        Some(match mnemonic {
            "coin" => |o| Op::Coin(u(o, 0)),
            "x" => |o| Op::X(u(o, 0)),
            "cx" => |o| Op::Cx(u(o, 0), u(o, 1)),
            "measure" => |o| Op::Measure {
                qubit: u(o, 0),
                tag: u(o, 1),
            },
            "reset" => |o| Op::Reset(u(o, 0)),
            _ => return None,
        })
    })
    .expect("the toy runs each instruction of its set")
}

/// Operand `i`, a block or a tag.
fn u(operands: &[Operand], i: usize) -> u32 {
    match operands[i] {
        Operand::Block(n) | Operand::Tag(n) => n,
        _ => unreachable!("the toy's set takes blocks and tags"),
    }
}

impl Executor<InstructionBatch<InstructionCall>> for ByOpcode {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        batch.shots.check_only(self.machine.shot)?;
        self.set.check_batch(batch.instructions)?;
        let mut outcomes = Vec::new();
        for call in batch.instructions {
            let op = self.table[call.opcode](&call.operands);
            self.machine.apply(op, &mut outcomes);
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

/// A call that names its instruction by mnemonic, the alternative to an
/// opcode.
#[derive(Clone)]
struct NamedCall {
    mnemonic: String,
    operands: Vec<Operand>,
}

/// The toy, accepting [`NamedCall`]s: it checks each call by looking up its
/// mnemonic in the set, then runs it by matching the mnemonic.
struct ByMnemonic {
    machine: Machine,
    set: InstructionSet,
}

impl Executor<InstructionBatch<NamedCall>> for ByMnemonic {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, NamedCall>) -> Result<Self::Reply, Rejected> {
        batch.shots.check_only(self.machine.shot)?;
        for (at, call) in batch.instructions.iter().enumerate() {
            let kinds = self.set.operands(&call.mnemonic);
            if !kinds.is_some_and(|k| {
                k.iter()
                    .copied()
                    .eq(call.operands.iter().map(Operand::kind))
            }) {
                let reason = format!("no `{}` with these operands", call.mnemonic);
                return Err(Rejected::Instruction { at, reason });
            }
        }
        let mut outcomes = Vec::new();
        for call in batch.instructions {
            let o = call.operands.as_slice();
            let op = match call.mnemonic.as_str() {
                "coin" => Op::Coin(u(o, 0)),
                "x" => Op::X(u(o, 0)),
                "cx" => Op::Cx(u(o, 0), u(o, 1)),
                "measure" => Op::Measure {
                    qubit: u(o, 0),
                    tag: u(o, 1),
                },
                "reset" => Op::Reset(u(o, 0)),
                other => unreachable!("the toy has no `{other}`"),
            };
            self.machine.apply(op, &mut outcomes);
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

/// The nanoseconds per call of `batch`, which handles [`CALLS`] calls: the
/// fastest of [`RUNS`] runs, the one least disturbed by the machine.
fn per_call(mut batch: impl FnMut()) -> f64 {
    let fastest = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            batch();
            start.elapsed()
        })
        .min()
        .expect("RUNS is not 0");
    fastest.as_secs_f64() * 1e9 / f64::from(CALLS)
}

/// The calls' representation: [`VecCall`], whose operands are a `Vec`,
/// against [`InstructionCall`], whose operands are held inline, on the four
/// things done with a batch: building it, checking it, renumbering it
/// into another set's opcodes (into a buffer kept across batches), and
/// dispatching it on the toy.
fn representations() {
    let toy = toy_set();
    let program_set = reversed(&toy);
    let ops: Vec<Op> = (0..CALLS).map(op_at).collect();
    let opcodes: Vec<Opcode> = ops
        .iter()
        .map(|op| {
            program_set
                .opcode(op.parts().0)
                .expect("the program's set has each")
        })
        .collect();
    let map = program_set.opcodes_in(&toy).expect("the same instructions");
    let kinds: Vec<Vec<OperandKind>> = toy
        .mnemonics()
        .map(|m| toy.operands(m).expect("its own mnemonic").to_vec())
        .collect();
    let table = toy_table(&toy);
    let expected = outcomes(&mut Machine::default(), &ops);

    let vec = measure_steps::<VecCall>(&ops, &opcodes, &map, &kinds, &table, &expected);
    let inline = measure_steps::<InstructionCall>(&ops, &opcodes, &map, &kinds, &table, &expected);

    println!(
        "{:>14} {:>10} {:>10} {:>10} {:>10}  (call size)",
        "operands", "build", "check", "renumber", "dispatch"
    );
    let row = |name: &str, [b, c, r, d]: [f64; 4], size: usize| {
        println!("{name:>14} {b:>10.1} {c:>10.1} {r:>10.1} {d:>10.1}  ({size} bytes)");
    };
    row("Vec", vec, size_of::<VecCall>());
    row("inline", inline, size_of::<InstructionCall>());
    let [b, c, r, d] = [0, 1, 2, 3].map(|k| inline[k] / vec[k]);
    println!(
        "{:>14} {b:>10.2} {c:>10.2} {r:>10.2} {d:>10.2}",
        "inline / Vec"
    );
}

/// A call whose operands are a `Vec`, the alternative to
/// [`InstructionCall`]'s, held inline: memory is allocated for each call
/// built or copied.
#[derive(Clone)]
struct VecCall {
    opcode: Opcode,
    operands: Vec<Operand>,
}

/// What the measured steps need of a call, whatever holds its operands.
trait Call: Sized {
    fn new(opcode: Opcode, operands: &[Operand]) -> Self;
    fn opcode(&self) -> Opcode;
    fn operands(&self) -> &[Operand];
    /// This call at `opcode`, with `other`'s operands, reusing its memory.
    fn renumber_from(&mut self, other: &Self, opcode: Opcode);
}

impl Call for InstructionCall {
    fn new(opcode: Opcode, operands: &[Operand]) -> Self {
        InstructionCall::new(opcode, operands.iter().copied())
    }

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn operands(&self) -> &[Operand] {
        &self.operands
    }

    fn renumber_from(&mut self, other: &Self, opcode: Opcode) {
        self.opcode = opcode;
        self.operands.clone_from(&other.operands);
    }
}

impl Call for VecCall {
    fn new(opcode: Opcode, operands: &[Operand]) -> Self {
        Self {
            opcode,
            operands: operands.to_vec(),
        }
    }

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn operands(&self) -> &[Operand] {
        &self.operands
    }

    fn renumber_from(&mut self, other: &Self, opcode: Opcode) {
        self.opcode = opcode;
        self.operands.clone_from(&other.operands);
    }
}

/// The four steps on `ops`, built as calls of type `C` at `opcodes`, in ns
/// per call: build, check against `kinds`, renumber through `map`,
/// dispatch through `table`. Asserts first that the steps run the toy as
/// [`Op`]s do.
fn measure_steps<C: Call + Clone>(
    ops: &[Op],
    opcodes: &[Opcode],
    map: &OpcodeMap,
    kinds: &[Vec<OperandKind>],
    table: &OpcodeTable<ToOp>,
    expected: &[ShotOutcome],
) -> [f64; 4] {
    let build = || -> Vec<C> {
        ops.iter()
            .zip(opcodes)
            .map(|(op, &opcode)| {
                let (_, operands, count) = op.parts();
                C::new(opcode, &operands[..count])
            })
            .collect()
    };
    let batch = build();
    let mut renumbered = Vec::new();
    renumber(&batch, map, &mut renumbered);
    assert!(check(kinds, &renumbered), "each call is of the toy's set");
    let mut out = Vec::new();
    dispatch(&mut Machine::default(), table, &renumbered, &mut out);
    assert_eq!(out, expected, "the calls run as the Ops do");

    let building = per_call(|| {
        black_box(build());
    });
    let checking = per_call(|| {
        assert!(check(kinds, black_box(&renumbered)));
    });
    let renumbering = per_call(|| renumber(black_box(&batch), map, &mut renumbered));
    let mut machine = Machine::default();
    let dispatching = per_call(|| {
        let mut outcomes = Vec::new();
        dispatch(&mut machine, table, black_box(&renumbered), &mut outcomes);
        black_box(outcomes);
    });
    [building, checking, renumbering, dispatching]
}

/// Whether each call of `batch` takes operands of the kinds its opcode's
/// instruction takes, as [`InstructionSet::check_batch`] checks them.
fn check<C: Call>(kinds: &[Vec<OperandKind>], batch: &[C]) -> bool {
    batch.iter().all(|call| {
        kinds.get(call.opcode().index()).is_some_and(|kinds| {
            kinds
                .iter()
                .copied()
                .eq(call.operands().iter().map(Operand::kind))
        })
    })
}

/// `batch` with each opcode replaced by its match in `map`, into `kept`,
/// whose calls' memory is reused, as the executor a program of another
/// numbering is given ([`compose`]) does.
fn renumber<C: Call + Clone>(batch: &[C], map: &OpcodeMap, kept: &mut Vec<C>) {
    for (at, call) in batch.iter().enumerate() {
        let opcode = map
            .get(call.opcode())
            .expect("each opcode is the program's");
        match kept.get_mut(at) {
            Some(slot) => slot.renumber_from(call, opcode),
            None => kept.push(C::new(opcode, call.operands())),
        }
    }
    kept.truncate(batch.len());
}

/// Runs `batch` on `machine` through `table`, its outcomes into `out`.
fn dispatch<C: Call>(
    machine: &mut Machine,
    table: &OpcodeTable<ToOp>,
    batch: &[C],
    out: &mut Vec<ShotOutcome>,
) {
    for call in batch {
        machine.apply(table[call.opcode()](call.operands()), out);
    }
}

/// The toy's instructions declared in the reverse order: a program's set
/// that numbers them otherwise.
fn reversed(set: &InstructionSet) -> InstructionSet {
    let mnemonics: Vec<&str> = set.mnemonics().collect();
    mnemonics
        .iter()
        .rev()
        .fold(InstructionSet::new("reversed"), |reversed, &m| {
            reversed.with_action(
                m,
                set.operands(m).expect("its own mnemonic").iter().copied(),
                action(m),
            )
        })
}

/// A reply of one outcome, the toy's reply to a batch that ends in a `measure`:
/// an [`ImmediateReply`] built from a `Vec` ([`ImmediateReply::done`]) against
/// one built by [`ImmediateReply::push`], which holds its outcome inline, each
/// taken whole by [`wait`]. In ns per reply alone, then per call of batches of
/// five calls, which the toy runs first.
fn replies() {
    let outcome = |i: u32| ShotOutcome::new(0, i, (i % 2 == 1).into());
    let from_vec = |i| ImmediateReply::done(vec![outcome(i)]);
    let pushed = |i| {
        let mut reply = ImmediateReply::default();
        reply.push(outcome(i));
        reply
    };
    assert_eq!(from_vec(1), pushed(1), "the replies agree");
    let mut out = Vec::with_capacity(1);
    let mut take = |reply: &dyn Fn(u32) -> ImmediateReply<ShotOutcome>| {
        per_call(|| {
            for i in 0..CALLS {
                wait(reply(i), &mut out).expect("done");
                black_box(&out);
                out.clear();
            }
        })
    };
    let replies = [take(&from_vec), take(&pushed)];

    let ops: Vec<Op> = (0..CALLS).map(op_at).collect();
    let mut batch_from_vec = |machine: &mut Machine, batch: &[Op]| {
        let mut outcomes = Vec::new();
        for &op in batch {
            machine.apply(op, &mut outcomes);
        }
        ImmediateReply::done(outcomes)
    };
    let mut scratch = Vec::with_capacity(1);
    let mut batch_pushed = |machine: &mut Machine, batch: &[Op]| {
        let mut reply = ImmediateReply::default();
        for &op in batch {
            machine.apply(op, &mut scratch);
            scratch.drain(..).for_each(|outcome| reply.push(outcome));
        }
        reply
    };
    let mut batches =
        |reply: &mut dyn FnMut(&mut Machine, &[Op]) -> ImmediateReply<ShotOutcome>| {
            let mut machine = Machine::default();
            let mut all = Vec::new();
            for batch in ops.chunks(5) {
                wait(reply(&mut machine, batch), &mut all).expect("done");
            }
            let time = per_call(|| {
                for batch in ops.chunks(5) {
                    wait(reply(&mut machine, batch), &mut out).expect("done");
                    black_box(&out);
                    out.clear();
                }
            });
            (all, time)
        };
    let (expected, batches_from_vec) = batches(&mut batch_from_vec);
    let (outcomes, batches_pushed) = batches(&mut batch_pushed);
    assert_eq!(outcomes, expected, "the batches' replies agree");

    println!(
        "{:>22} {:>10} {:>10} {:>14}",
        "one outcome", "from Vec", "pushed", "pushed / Vec"
    );
    let row = |name: &str, [from_vec, pushed]: [f64; 2]| {
        println!(
            "{name:>22} {from_vec:>10.1} {pushed:>10.1} {:>14.2}",
            pushed / from_vec
        );
    };
    row("per reply", replies);
    row("per call, batches of 5", [batches_from_vec, batches_pushed]);
}

/// The toy's executor of one shot, called with batches of [`CALLS`] calls,
/// then of one call: directly (bare), and by a program run on a chain of
/// it (`chain.run`) whose set is the toy's (the same numbering) or the
/// toy's reversed (another), whose batches are each copied into the toy's
/// numbering on their way. Calls built by name with the toy's set, as a
/// program that cannot afford the copy builds them, are those of the same
/// numbering, so they cost what it costs. Each measurement runs on a new
/// executor, after a first batch of [`CALLS`] calls that makes its shot
/// and whose outcomes are checked against the bare executor's. In ns per
/// call.
fn renumbering() {
    let toy = toy_set();
    let program_set = reversed(&toy);
    let shots = ShotSet::one(1);
    let ops: Vec<Op> = (0..CALLS).map(op_at).collect();
    let calls_of = |set: &InstructionSet| -> Vec<InstructionCall> {
        ops.iter()
            .map(|op| {
                let (mnemonic, operands) = op.named();
                set.call(mnemonic, operands)
                    .expect("each call is of the set")
            })
            .collect()
    };
    let (in_toy, in_program) = (calls_of(&toy), calls_of(&program_set));
    let bare = |size| Timed::new(&in_toy, size, None).on(&mut toy_executor(), &shots);
    let ran = |calls, set: &InstructionSet, size| {
        let program = Timed::new(calls, size, Some(set));
        let chain = compose(driver(), toy_executor()).expect("nothing between to refuse");
        let run = chain.run(program, shots.clone());
        run.expect("the toy accepts the program's set").wait().0
    };
    let expected = bare(CALLS as usize).0;

    println!(
        "{:>16} {:>8} {:>8} {:>8} {:>12} {:>12}",
        "program", "bare", "same", "copied", "same / bare", "copy / bare"
    );
    for size in [CALLS as usize, 1] {
        let variants: [&dyn Fn() -> (Vec<ShotOutcome>, f64); 3] =
            [&|| bare(size), &|| ran(&in_toy, &toy, size), &|| {
                ran(&in_program, &program_set, size)
            }];
        // The three in turn, three times, so that a disturbance of the
        // machine slows a round of each rather than one of them.
        let mut fastest = [f64::INFINITY; 3];
        for _ in 0..3 {
            for (variant, fastest) in variants.iter().zip(&mut fastest) {
                let (first, time) = variant();
                assert_eq!(first, expected, "the variants agree");
                *fastest = fastest.min(time);
            }
        }
        let [bare, same, copied] = fastest;
        println!(
            "{:>16} {bare:>8.1} {same:>8.1} {copied:>8.1} {:>12.2} {:>12.2}",
            format!("batches of {size}"),
            same / bare,
            copied / bare
        );
    }
}

/// An adaptive program that times its calls: it sends `calls` once, whole,
/// which makes the executor's shot, then times them in batches of `size`
/// ([`per_call`]); it gives the first batch's outcomes and the time. It
/// states the set its calls are of, if any, in its needs.
struct Timed<'a> {
    calls: &'a [InstructionCall],
    size: usize,
    set: Option<Arc<InstructionSet>>,
}

impl<'a> Timed<'a> {
    fn new(calls: &'a [InstructionCall], size: usize, set: Option<&InstructionSet>) -> Self {
        let set = set.map(|set| Arc::new(set.clone()));
        Self { calls, size, set }
    }

    /// The first batch's outcomes and the time, on `executor`.
    fn on<E>(&self, executor: &mut E, shots: &ShotSet) -> (Vec<ShotOutcome>, f64)
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        let first = shot_outcomes(executor, shots, self.calls);
        let time = per_call(|| {
            for batch in self.calls.chunks(self.size) {
                run_batch(executor, shots, batch);
            }
        });
        (first, time)
    }
}

impl AdaptiveProgram for Timed<'_> {
    type Instruction = InstructionCall;
    type Output = (Vec<ShotOutcome>, f64);

    fn needs(&self) -> Needs {
        Needs::default().with_instruction_set(self.set.clone())
    }

    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> (Vec<ShotOutcome>, f64) {
        self.on(executor, shots)
    }
}

/// The toy's executor of one shot, called as in [`renumbering`]: alone
/// (bare); with an adapter composed with it that passes each call down, in
/// the toy's numbering, so that the boundary passes each batch as it is:
/// an adapter written by hand that maps the executor ([`Passing`], by
/// hand), and an [`adapter_fn`] whose logic sends each
/// call down ([`pass`], `adapter_fn`); and with one that passes each call
/// down in the toy's reversed numbering, so that each batch is copied into
/// the toy's on its way (copied). Each is called directly, as the executor
/// it is. In ns per call.
fn composition() {
    let toy = toy_set();
    let program_set = Arc::new(reversed(&toy));
    let shots = ShotSet::one(1);
    let ops: Vec<Op> = (0..CALLS).map(op_at).collect();
    let calls_of = |set: &InstructionSet| -> Vec<InstructionCall> {
        ops.iter()
            .map(|op| {
                let (mnemonic, operands) = op.named();
                set.call(mnemonic, operands)
                    .expect("each call is of the set")
            })
            .collect()
    };
    let (in_toy, in_program) = (calls_of(&toy), calls_of(&program_set));
    let mut by_hand = compose(Passing, toy_executor()).expect("it sends what it gets");
    let mut by_fn =
        compose(adapter_fn(|_| Ok(()), pass), toy_executor()).expect("it sends what it gets");
    let mut copied =
        compose(Sending(program_set), toy_executor()).expect("the toy accepts the reversed set");
    let mut bare = toy_executor();

    let expected = shot_outcomes(&mut bare, &shots, &in_toy);
    assert_eq!(shot_outcomes(&mut by_hand, &shots, &in_toy), expected);
    assert_eq!(shot_outcomes(&mut by_fn, &shots, &in_toy), expected);
    assert_eq!(shot_outcomes(&mut copied, &shots, &in_program), expected);

    println!(
        "{:>16} {:>8} {:>9} {:>10} {:>8} {:>16} {:>12}",
        "composed", "bare", "by hand", "adapter_fn", "copied", "adapter_fn / bare", "copy / bare"
    );
    for size in [CALLS as usize, 1] {
        let mut executors: [(&mut RunBatch, &[InstructionCall]); 4] = [
            (&mut |b| run_batch(&mut bare, &shots, b), &in_toy),
            (&mut |b| run_batch(&mut by_hand, &shots, b), &in_toy),
            (&mut |b| run_batch(&mut by_fn, &shots, b), &in_toy),
            (&mut |b| run_batch(&mut copied, &shots, b), &in_program),
        ];
        let mut fastest = [f64::INFINITY; 4];
        for _ in 0..3 {
            for ((executor, batch), fastest) in executors.iter_mut().zip(&mut fastest) {
                let time = per_call(|| batch.chunks(size).for_each(&mut **executor));
                *fastest = fastest.min(time);
            }
        }
        let [bare, by_hand, by_fn, copied] = fastest;
        println!(
            "{:>16} {bare:>8.1} {by_hand:>9.1} {by_fn:>10.1} {copied:>8.1} {:>16.2} {:>12.2}",
            format!("batches of {size}"),
            by_fn / bare,
            copied / bare
        );
    }
}

/// The toy's executor on 1, 10 and 100 shots, each shot running
/// [`CALLS`] / shots calls, in batches of 1 and of 100: alone (bare); with
/// an [`adapter_fn`] whose logic sends each call down ([`pass`]), with one
/// state (one state); and with a state per shot, whose shots' equal sends
/// are merged back into one call below (per shot), what the merge costs.
/// Then the same two, their states owning their way below
/// ([`owning_adapter_fn`], [`pass_owned`]), against the lent form: what
/// owning costs. In ns per call per shot. Each is first checked to read
/// what the toy alone reads, and to send each batch below in one call, as
/// its shots' equal sends merge.
fn merging() {
    let toy = toy_set();
    let calls: Vec<InstructionCall> = (0..CALLS)
        .map(|i| {
            let (mnemonic, operands) = op_at(i).named();
            toy.call(mnemonic, operands)
                .expect("each call is of the set")
        })
        .collect();
    println!(
        "{:>22} {:>8} {:>10} {:>9} {:>10} {:>9} {:>17} {:>16} {:>11} {:>14}",
        "merging",
        "bare",
        "one state",
        "per shot",
        "owned one",
        "owned per",
        "per shot / bare",
        "per shot / one",
        "owned / one",
        "owned / per"
    );
    for count in [1, 10, 100] {
        let shots = ShotSet::range(0..=count - 1).expect("not empty");
        let calls = &calls[..(CALLS / count) as usize];
        let below = Rc::new(Cell::new(0));
        let toy = || ToyExecutor::counted_by(&below);
        let mut bare = toy();
        let mut one_state =
            compose(adapter_fn(|_| Ok(()), pass), toy()).expect("it sends what it gets");
        let mut per_shot = compose(adapter_fn(|_| Ok(()), pass).shots(OnePerShot), toy())
            .expect("it sends what it gets");
        let mut owned_one =
            compose(owning_adapter_fn(own, pass_owned), toy()).expect("it sends what it gets");
        let mut owned_per = compose(owning_adapter_fn(own, pass_owned).shots(OnePerShot), toy())
            .expect("it sends what it gets");
        let expected = shot_outcomes(&mut bare, &shots, calls);
        below.set(0);
        let agrees = |variant: &str, outcomes: Vec<ShotOutcome>| {
            assert_eq!(
                outcomes, expected,
                "{variant} reads what the toy alone reads"
            );
            assert_eq!(below.replace(0), 1, "{variant} sends the batch below once");
        };
        agrees("one state", shot_outcomes(&mut one_state, &shots, calls));
        agrees("per shot", shot_outcomes(&mut per_shot, &shots, calls));
        agrees("owned one", shot_outcomes(&mut owned_one, &shots, calls));
        agrees("owned per", shot_outcomes(&mut owned_per, &shots, calls));
        for size in [1, 100] {
            let mut executors: [&mut RunBatch; 5] = [
                &mut |b| run_batch(&mut bare, &shots, b),
                &mut |b| run_batch(&mut one_state, &shots, b),
                &mut |b| run_batch(&mut per_shot, &shots, b),
                &mut |b| run_batch(&mut owned_one, &shots, b),
                &mut |b| run_batch(&mut owned_per, &shots, b),
            ];
            let mut fastest = [f64::INFINITY; 5];
            for _ in 0..3 {
                for (executor, fastest) in executors.iter_mut().zip(&mut fastest) {
                    let time = per_call(|| calls.chunks(size).for_each(&mut **executor));
                    *fastest = fastest.min(time);
                }
            }
            let [bare, one_state, per_shot, owned_one, owned_per] = fastest;
            println!(
                "{:>22} {bare:>8.1} {one_state:>10.1} {per_shot:>9.1} {owned_one:>10.1} \
                 {owned_per:>9.1} {:>17.2} {:>16.2} {:>11.2} {:>14.2}",
                format!("{count} shots, batches of {size}"),
                per_shot / bare,
                per_shot / one_state,
                owned_one / one_state,
                owned_per / per_shot
            );
        }
    }
}

/// An adapter written by hand that passes each call down as it is: the
/// executor it gives is the executor downstream.
struct Passing;

impl Adapter for Passing {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Passing {
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// The logic of an [`adapter_fn`] that passes each call down as it is.
fn pass(
    (): &mut (),
    input: &ProgramInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// The state closure of an [`owning_adapter_fn`] whose state is its way
/// below.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature a state closure has"
)]
fn own(
    _: ShotContext,
    below: BelowHandle<InstructionCall>,
) -> Result<BelowHandle<InstructionCall>, Rejected> {
    Ok(below)
}

/// The logic of an [`owning_adapter_fn`] that passes each call down as it
/// is, through its state's way below.
fn pass_owned(
    below: &mut BelowHandle<InstructionCall>,
    input: &ProgramInput<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// An adapter that passes each call down, numbered by a set other than
/// the toy's, so that each batch is copied into the toy's numbering on its
/// way down.
struct Sending(Arc<InstructionSet>);

impl Adapter for Sending {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.0))
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.0))
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Sending {
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// The toy's executor of one shot, called as in [`composition`]: alone
/// (bare), and with an adapter composed with it that sends one call of its
/// own for each call it is called with, the same
/// instruction. The adapter sends in the toy's reversed numbering, so that
/// each batch it sends is copied into the toy's on its way down (copied);
/// or it adopts the toy's numbering when it is built
/// ([`AdapterBoundary::adopt_downstream_numbering`]), so that its calls pass as
/// they are (adopted). The same calls in each; in ns per call. The
/// run-time guide's `Encoding` adopts the numbering in the same way,
/// though it sends a gadget, several calls, for each call.
fn adoption() {
    let toy = toy_set();
    let own = Arc::new(reversed(&toy));
    let shots = ShotSet::one(1);
    let in_toy: Vec<InstructionCall> = (0..CALLS)
        .map(|i| {
            let (mnemonic, operands) = op_at(i).named();
            toy.call(mnemonic, operands)
                .expect("each call is of the set")
        })
        .collect();
    let emitting = |adopts| {
        compose(Emitting::new(&toy, &own, adopts), toy_executor())
            .expect("the toy accepts the reversed set")
    };
    let (mut copied, mut adopted) = (emitting(false), emitting(true));
    let mut bare = toy_executor();

    let expected = shot_outcomes(&mut bare, &shots, &in_toy);
    assert_eq!(shot_outcomes(&mut copied, &shots, &in_toy), expected);
    assert_eq!(shot_outcomes(&mut adopted, &shots, &in_toy), expected);

    println!(
        "{:>16} {:>8} {:>8} {:>8} {:>12} {:>12} {:>14}",
        "emitting", "bare", "copied", "adopted", "copy / bare", "adopt / bare", "adopt / copy"
    );
    for size in [CALLS as usize, 1] {
        let mut executors: [&mut RunBatch; 3] = [
            &mut |b| run_batch(&mut bare, &shots, b),
            &mut |b| run_batch(&mut copied, &shots, b),
            &mut |b| run_batch(&mut adopted, &shots, b),
        ];
        let mut fastest = [f64::INFINITY; 3];
        for _ in 0..3 {
            for (executor, fastest) in executors.iter_mut().zip(&mut fastest) {
                let time = per_call(|| in_toy.chunks(size).for_each(&mut **executor));
                *fastest = fastest.min(time);
            }
        }
        let [bare, copied, adopted] = fastest;
        println!(
            "{:>16} {bare:>8.1} {copied:>8.1} {adopted:>8.1} {:>12.2} {:>12.2} {:>14.2}",
            format!("batches of {size}"),
            copied / bare,
            adopted / bare,
            adopted / copied
        );
    }
}

/// An adapter that sends a call of its own for each call it is called
/// with: the same instruction, at the opcode its table gives, resolved
/// once. Its table gives opcodes in the set it sends, or, when it adopts
/// the downstream numbering, in the downstream set.
struct Emitting {
    accepts: Arc<InstructionSet>,
    sends: Arc<InstructionSet>,
    /// The opcode it sends for each opcode it accepts.
    emits: OpcodeTable<Opcode>,
    adopts: bool,
}

impl Emitting {
    /// Accepting `accepts` and sending `sends`; adopting the downstream
    /// numbering when it is built if `adopts`.
    fn new(accepts: &InstructionSet, sends: &Arc<InstructionSet>, adopts: bool) -> Self {
        let emits = accepts
            .table(|mnemonic| sends.opcode(mnemonic))
            .expect("it sends each instruction it accepts");
        Self {
            accepts: Arc::new(accepts.clone()),
            sends: Arc::clone(sends),
            emits,
            adopts,
        }
    }
}

impl Adapter for Emitting {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.sends))
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.accepts))
    }

    /// When it adopts, its table is mapped into the downstream numbering,
    /// through the map its set was matched with, once.
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        if !self.adopts {
            return self;
        }
        let Some(map) = boundary.adopt_downstream_numbering() else {
            return self;
        };
        let sends = boundary
            .downstream()
            .instruction_set
            .clone()
            .expect("a set its own was matched in");
        let emits = self
            .accepts
            .table(|mnemonic| map.get(self.emits[self.accepts.opcode(mnemonic)?]))
            .expect("the map matches each instruction it sends");
        Self {
            sends,
            emits,
            ..self
        }
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Emitting {
    type Output = Emitted<E>;

    fn apply(self, downstream: E) -> Emitted<E> {
        Emitted {
            accepts: Arc::clone(&self.accepts),
            emits: self.emits.clone(),
            batch: Vec::new(),
            inner: downstream,
        }
    }
}

/// An [`Emitting`] adapter upstream of an executor: it checks each batch,
/// then sends its own calls, built in a batch it keeps.
struct Emitted<E> {
    accepts: Arc<InstructionSet>,
    emits: OpcodeTable<Opcode>,
    batch: Vec<InstructionCall>,
    inner: E,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for Emitted<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        self.accepts.check_batch(batch.instructions)?;
        self.batch.clear();
        self.batch.extend(batch.instructions.iter().map(|call| {
            InstructionCall::new(self.emits[call.opcode], call.operands.iter().copied())
        }));
        self.inner.call(InstructionBatchRef {
            shots: batch.shots,
            instructions: &self.batch,
        })
    }
}

impl<E: Close> Close for Emitted<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// Runs a batch on an executor, and drops its outcomes.
type RunBatch<'a> = dyn FnMut(&[InstructionCall]) + 'a;

/// The outcomes of `batch` on `shots` of `executor`.
fn shot_outcomes<E: Executor<InstructionBatch<InstructionCall>>>(
    executor: &mut E,
    shots: &ShotSet,
    batch: &[InstructionCall],
) -> Vec<ShotOutcome> {
    let reply = executor
        .call(InstructionBatchRef {
            shots,
            instructions: batch,
        })
        .expect("the batch is valid");
    let mut out = Vec::new();
    wait(reply, &mut out).expect("the toy's batches end well");
    out
}

/// Runs `batch` on `shots` of `executor`, and drops its outcomes.
fn run_batch<E: Executor<InstructionBatch<InstructionCall>>>(
    executor: &mut E,
    shots: &ShotSet,
    batch: &[InstructionCall],
) {
    black_box(shot_outcomes(executor, shots, black_box(batch)));
}

/// The toy's executor of [`InstructionCall`]s.
fn toy_executor() -> ToyExecutor {
    ToyExecutor::new()
}

/// The toy's executor of [`InstructionCall`]s: it describes `set`, holds
/// a toy per shot, made at the shot's first call, which runs calls through
/// `table`, and counts the calls it is given in `calls`.
struct ToyExecutor {
    set: Arc<InstructionSet>,
    table: OpcodeTable<ToOp>,
    shots: BTreeMap<ShotId, ByOpcode>,
    calls: Rc<Cell<usize>>,
}

impl ToyExecutor {
    /// The executor, in the toy's own numbering.
    fn new() -> Self {
        Self::counted_by(&Rc::default())
    }

    /// The executor, in the toy's own numbering, counting its calls in
    /// `calls`, which the bench keeps once the executor is composed.
    fn counted_by(calls: &Rc<Cell<usize>>) -> Self {
        let set = toy_set();
        let table = toy_table(&set);
        Self {
            set: Arc::new(set),
            table,
            shots: BTreeMap::new(),
            calls: Rc::clone(calls),
        }
    }
}

impl Describe for ToyExecutor {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_instruction_set(Some(Arc::clone(&self.set)))
    }
}

impl Executor<InstructionBatch<InstructionCall>> for ToyExecutor {
    type Reply = ImmediateReply<ShotOutcome>;

    /// Runs the instructions on each shot's toy, in the batch's order. The
    /// first shot's refusal refuses the call: the toys refuse only a batch
    /// that is not of the set, so every shot refuses it, before it runs.
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        self.calls.set(self.calls.get() + 1);
        let mut reply = ImmediateReply::default();
        let mut outcomes = Vec::new();
        for (k, &j) in batch.shots.iter().enumerate() {
            let (set, table) = (&self.set, &self.table);
            let shot = self.shots.entry(j).or_insert_with(|| ByOpcode {
                machine: Machine {
                    shot: j,
                    ..Machine::default()
                },
                set: InstructionSet::clone(set),
                table: table.clone(),
            });
            let one = InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: batch.instructions,
            };
            let mut part = match shot.call(one) {
                Ok(part) => part,
                Err(rejected) if k == 0 => return Err(rejected),
                Err(_) => unreachable!("a batch the first shot ran, every shot runs"),
            };
            let _ = part.poll_events(&mut outcomes, Waker::noop());
            outcomes.drain(..).for_each(|outcome| reply.push(outcome));
        }
        Ok(reply)
    }
}

impl Close for ToyExecutor {
    type Report = ();

    fn close(&mut self) {
        self.shots.clear();
    }
}
