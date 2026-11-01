// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The toy simulator the guide tests run on: classical bits and fair coins.
//!
//! It simulates no quantum state. It has just enough to show what the
//! protocol carries: a random draw from the shot's seed (`Coin`), so that a
//! test sees which seed a shot drew from, and a measurement that the program's
//! next batch may depend on.

use std::sync::Arc;

use executor_protocol::{
    Action, Capabilities, Executor, InstructionBatch, InstructionBatchRef, InstructionSet,
    OnePerShot, OnePerShotExecutor, OperandKind, Outcome, Outcomes, Pauli, ProgramInput, Rejected,
    ShotContext, ShotId, ShotOutcome, ShotSet, TypedInstruction, Value, check_unique_tags,
    executor_fn, wait,
};

/// The toy's instructions, as a type: an executor of
/// `InstructionBatch<Op>` accepts these and nothing else; [`toy_set`]
/// describes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Sets the qubit to a fair random bit, drawn from the shot's seed.
    Coin(u32),
    /// Flips the qubit.
    X(u32),
    /// Flips the target (second) when the control (first) is 1.
    Cx(u32, u32),
    /// Reads the qubit, and names the outcome `tag`.
    Measure { qubit: u32, tag: u32 },
    /// Sets the qubit to 0.
    Reset(u32),
}

/// A toy instruction names itself by its variant, so a driver of them is
/// composed with a chain with no adapter between them.
impl TypedInstruction for Op {}

impl Op {
    fn qubits(self) -> impl Iterator<Item = u32> {
        let (a, b) = match self {
            Op::Coin(q) | Op::X(q) | Op::Reset(q) | Op::Measure { qubit: q, .. } => (q, None),
            Op::Cx(c, t) => (c, Some(t)),
        };
        std::iter::once(a).chain(b)
    }

    /// The tag of its readout, if it makes one.
    pub fn tag(self) -> Option<u32> {
        match self {
            Op::Measure { tag, .. } => Some(tag),
            _ => None,
        }
    }
}

/// The readouts of `batch`: each one's index and tag, as
/// [`check_unique_tags`] takes them.
pub fn readouts(batch: &[Op]) -> impl Iterator<Item = (usize, u32)> {
    batch
        .iter()
        .enumerate()
        .filter_map(|(at, op)| Some((at, op.tag()?)))
}

/// The description of [`Op`], so that an adapter upstream of the toy can check
/// what it sends when it is built. Its blocks are qubits, and each
/// instruction declares its [`action`]. Written by hand, so it must name each
/// variant.
pub fn toy_set() -> InstructionSet {
    use OperandKind::{Block, Tag};
    InstructionSet::new("toy")
        .with_action("coin", [Block], action("coin"))
        .with_action("x", [Block], action("x"))
        .with_action("cx", [Block, Block], action("cx"))
        .with_action("measure", [Block, Tag], action("measure"))
        .with_action("reset", [Block], action("reset"))
}

/// What the toy's instruction `mnemonic` does, as Qodec declares it, so
/// that a set that names it otherwise is matched to it: `coin` prepares
/// `|+⟩`, which reads as a fair bit; `reset` prepares `|0⟩`; `measure`
/// observes `Z`, ending its block as a Qodec measurement does.
///
/// # Panics
///
/// When the toy has no instruction `mnemonic`.
pub fn action(mnemonic: &str) -> Action {
    match mnemonic {
        "coin" => Action::new([], [1]).stabilize([Pauli::x(0)]),
        "x" => Action::new([1], [1]).pauli(Pauli::x(0)),
        "cx" => Action::new([1, 1], [1, 1]).clifford([
            (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
            (Pauli::z(1), Pauli::z(0) * Pauli::z(1)),
        ]),
        "measure" => Action::new([1], []).observe([Pauli::z(0)]),
        "reset" => Action::new([], [1]).stabilize([Pauli::z(0)]),
        other => panic!("the toy has no instruction `{other}`"),
    }
}

/// The qubits of one toy shot.
pub const QUBITS: u32 = 8;

/// The toy simulator: [`ToyState`]s run by [`ToyState::run`], one per shot
/// ([`OnePerShot`]), each made at its shot's first call and drawing from
/// the last executor's seed in the shot, so that shot `j` draws the same
/// coins whichever other shots it runs with. Built by [`toy_simulator`];
/// named so that a test can name the executor's type. Its states report
/// nothing (`()`).
pub type ToySimulator = OnePerShotExecutor<
    Op,
    ToyState,
    StartToy,
    RunToy,
    CheckToy,
    fn(&ToyState) -> f64,
    fn(&ToyState),
>;

/// The toy's state closure, [`ToyState::start`].
type StartToy = fn(ShotContext) -> Result<ToyState, Rejected>;

/// The toy's logic, [`ToyState::run`].
type RunToy = fn(&mut ToyState, &ProgramInput<'_, Op>, &mut Outcomes<'_>) -> Result<(), String>;

/// The toy's check of a batch, [`check`].
type CheckToy = fn(&ProgramInput<'_, Op>) -> Result<(), Rejected>;

/// The toy simulator whose shots draw from `seed`: `QUBITS` qubits, and
/// the toy's instruction set.
pub fn toy_simulator(seed: u64) -> ToySimulator {
    toy_simulator_described_with(seed, toy_set())
}

/// The toy simulator whose shots draw from `seed`, described as having the
/// instructions of `set`, for a test of what a chain checks against an
/// executor's description: it still runs the toy's instructions.
pub fn toy_simulator_described_with(seed: u64, set: InstructionSet) -> ToySimulator {
    let promise = Capabilities::default()
        .with_qubits(QUBITS)
        .with_instruction_set(Some(Arc::new(set)));
    executor_fn(
        promise,
        ToyState::start as StartToy,
        ToyState::run as RunToy,
    )
    .check(check as CheckToy)
    .seed(seed)
    .shots(OnePerShot)
}

/// The bits of one shot of the toy, whose coins come from the shot's seed
/// only.
pub struct ToyState {
    bits: Vec<bool>,
    rng: u64,
}

impl ToyState {
    /// `QUBITS` zeros, whose coins come from `rng_seed`.
    pub fn new(rng_seed: u64) -> Self {
        Self {
            bits: vec![false; QUBITS as usize],
            rng: rng_seed,
        }
    }

    /// The state of the shot of `ctx`, drawing from the last executor's seed
    /// in it ([`ShotContext::seed`]).
    ///
    /// # Errors
    ///
    /// None: the toy makes any shot's state.
    pub fn start(ctx: ShotContext) -> Result<Self, Rejected> {
        Ok(Self::new(ctx.seed()))
    }

    /// Runs `input`'s instructions, which [`check`] accepted, giving each
    /// readout.
    ///
    /// # Errors
    ///
    /// None: the toy fails no batch.
    pub fn run(
        &mut self,
        input: &ProgramInput<'_, Op>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        for &op in input.instructions() {
            self.apply(op, outcomes);
        }
        Ok(())
    }

    /// Runs `op`, giving its readout if it makes one, for an executor that
    /// takes the toy's instructions in another form: as calls of
    /// [`toy_set`].
    ///
    /// # Panics
    ///
    /// When `op` names a qubit the toy does not have. [`check`] refuses
    /// those in a program of [`Op`]s; an executor that takes them in
    /// another form checks them itself, or a call naming one panics: the
    /// panic reaches the caller, and the executor refuses every later call.
    pub fn apply(&mut self, op: Op, outcomes: &mut Outcomes<'_>) {
        match op {
            Op::Coin(q) => self.bits[q as usize] = self.coin(),
            Op::X(q) => self.bits[q as usize] ^= true,
            Op::Cx(c, t) => self.bits[t as usize] ^= self.bits[c as usize],
            Op::Measure { qubit, tag } => {
                outcomes.push(Outcome::new(tag, self.bits[qubit as usize].into()));
            }
            Op::Reset(q) => self.bits[q as usize] = false,
        }
    }

    /// One step of `SplitMix64`; the low bit is the coin.
    fn coin(&mut self) -> bool {
        self.rng = self.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) & 1 == 1
    }
}

/// Checks a batch's instructions, as the toy does, once per batch, before
/// any shot runs them.
///
/// # Errors
///
/// [`Rejected::Instruction`] for the first instruction on a qubit the toy
/// does not have, or the first readout that repeats a tag
/// ([`check_unique_tags`]).
pub fn check(input: &ProgramInput<'_, Op>) -> Result<(), Rejected> {
    let batch = input.instructions();
    if let Some(at) = batch.iter().position(|op| op.qubits().any(|q| q >= QUBITS)) {
        return Err(Rejected::Instruction {
            at,
            reason: format!("the toy has {QUBITS} qubits"),
        });
    }
    check_unique_tags(readouts(batch))
}

/// A program with feedback, as a user writes it against any executor of the
/// toy's instructions, run on shot `j`. Toss a coin; if it shows 1, flip
/// qubit 1; copy qubit 1 onto qubit 2; read both.
pub fn coin_then_feedback<E: Executor<InstructionBatch<Op>>>(
    executor: &mut E,
    j: ShotId,
) -> Vec<ShotOutcome> {
    let shots = ShotSet::one(j);
    let mut outcomes = Vec::new();
    let first = [Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }];
    let reply = executor
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: &first,
        })
        .expect("the program uses qubits 0 to 2, which a toy executor has");
    wait(reply, &mut outcomes).expect("the executor does not fail the program");
    let mut next = Vec::new();
    if outcomes[0].value == Value::One {
        next.push(Op::X(1));
    }
    next.extend([
        Op::Cx(1, 2),
        Op::Measure { qubit: 1, tag: 1 },
        Op::Measure { qubit: 2, tag: 2 },
        Op::Reset(0),
    ]);
    let reply = executor
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: &next,
        })
        .expect("the program uses qubits 0 to 2, which a toy executor has");
    wait(reply, &mut outcomes).expect("the executor does not fail the program");
    outcomes
}
