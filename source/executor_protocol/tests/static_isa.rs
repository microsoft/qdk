// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [Guide: A static ISA: a repetition
//! code](../docs/guides.md#a-static-isa-a-repetition-code):
//! an adapter that lowers a logical instruction set onto a physical
//! executor, and decodes on the way up, as a Qodec code does.
//!
//! The logical instructions are a type, `Logical`, and the physical ones
//! are the toy's, `Op`. `Repetition3`, the executor the code puts upstream
//! of the toy, goes from one to the other, a three-qubit repetition code: each
//! logical instruction goes down as its gadget, and each logical measurement
//! comes back up as the majority of its block's three readouts. The types say
//! what each executor accepts: a driver of `Logical` instructions compiles
//! upstream of the code composed with the toy, and upstream of the toy
//! itself is a compile error, which no test pins. The code also describes
//! both sets, so that a chain checks, when it is built, that the toy runs
//! every instruction the gadgets send, and that the code has every
//! instruction its driver needs.
//!
//! Read `static_isa/logical.rs` (the logical set),
//! `static_isa/repetition3.rs` (what goes down and comes up),
//! `static_isa/repetition_code.rs` (the code as an adapter, composed with
//! the toy simulator and checked when it is),
//! `static_isa/repetition_fn.rs` (the same code as a logic, made an
//! adapter by `adapter_fn`), `static_isa/code_runtime.rs` (the same code as
//! a runtime that owns its backend, as a Qodec runtime does, made an
//! adapter by `owning_adapter_fn`), then the tests in their order in the
//! file.

#[path = "static_isa/code_runtime.rs"]
mod code_runtime;
#[path = "static_isa/logical.rs"]
mod logical;
#[path = "static_isa/repetition3.rs"]
mod repetition3;
#[path = "static_isa/repetition_code.rs"]
mod repetition_code;
#[path = "static_isa/repetition_fn.rs"]
mod repetition_fn;
#[path = "toy/simulator.rs"]
mod toy;

use std::f64::consts::FRAC_PI_4;
use std::sync::Arc;

use executor_protocol::{
    Action, Angle, Below, BuildError, Describe, Drive, Driver, Executor, InstructionBatch,
    InstructionBatchRef, InstructionMismatch, InstructionSet, Need, Needs, OnePerShot, Operand,
    OperandKind, Outcomes, Pauli, ProgramInput, Rejected, ShotOutcome, ShotSet, Value, adapter_fn,
    compose, conformance, owning_adapter_fn, wait,
};

use code_runtime::{
    Backend, BatchResult, CodePlan, CodeRuntime, Lower, instantiated, through_the_runtime,
};
use logical::{Logical, logical_action, logical_set};
use repetition_code::RepetitionCode;
use repetition_fn::{lower_then_decode, within};
use repetition3::{Repetition3, gadget, gadget_instructions};
use toy::{Op, coin_then_feedback, toy_simulator, toy_simulator_described_with};

/// An executor that keeps the instruction set, upstream of another: records
/// each batch that passes through it.
struct BatchRecorder<E> {
    inner: E,
    batches: Vec<Vec<Op>>,
}

impl<E: Executor<InstructionBatch<Op>>> Executor<InstructionBatch<Op>> for BatchRecorder<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        self.batches.push(batch.instructions.to_vec());
        self.inner.call(batch)
    }
}

/// `instructions` on shot 0 alone.
fn on_shot_0<I>(instructions: &[I]) -> InstructionBatchRef<'_, I> {
    const SHOT_0: &ShotSet = &ShotSet::one(0);
    InstructionBatchRef {
        shots: SHOT_0,
        instructions,
    }
}

/// The outcomes of `instructions` on shot 0.
fn run(
    executor: &mut impl Executor<InstructionBatch<Logical>>,
    instructions: &[Logical],
) -> Vec<ShotOutcome> {
    let mut outcomes = Vec::new();
    wait(
        executor
            .call(on_shot_0(instructions))
            .expect("the tests send only qubits the toy has"),
        &mut outcomes,
    )
    .expect("the toy never fails a batch");
    outcomes
}

/// A logical batch goes down as one physical batch, its gadgets in order,
/// and its one logical measurement comes back up as one outcome under the
/// logical tag.
#[test]
fn each_logical_instruction_goes_down_as_its_gadget() {
    let mut code = Repetition3 {
        inner: BatchRecorder {
            inner: toy_simulator(0),
            batches: Vec::new(),
        },
        blocks: 2,
    };
    let batch = [
        Logical::X(1),
        Logical::Coin(0),
        Logical::MeasureZ { block: 0, tag: 5 },
    ];
    let outcomes = run(&mut code, &batch);

    let m = |qubit, tag| Op::Measure { qubit, tag };
    let physical = vec![
        // X(1), on block 1: qubits 3, 4, 5.
        Op::X(3),
        Op::X(4),
        Op::X(5),
        // Coin(0): one draw, copied to the block.
        Op::Coin(0),
        Op::Cx(0, 1),
        Op::Cx(0, 2),
        // MeasureZ: logical tag 5 is read as tags 15, 16, 17.
        m(0, 15),
        m(1, 16),
        m(2, 17),
    ];
    assert_eq!(code.inner.batches, vec![physical]);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].tag, 5);
}

/// A program that flips a logical coin on block 1 and reads it, and states
/// `needs`.
struct FlipALogicalCoin {
    needs: Needs,
}

impl Drive for FlipALogicalCoin {
    type Instruction = Logical;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }

    async fn drive<E: Executor<InstructionBatch<Logical>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let batch = [Logical::Coin(1), Logical::MeasureZ { block: 1, tag: 0 }];
        let reply = executor.call(InstructionBatchRef {
            shots,
            instructions: &batch,
        });
        let mut outcomes = Vec::new();
        wait(reply.expect("the code runs the batch"), &mut outcomes)
            .expect("the toy never fails a batch");
        outcomes
    }
}

/// The code passes on the shots' seeds: shot `j` of a logical run draws
/// the coins that shot `j` of the toy draws.
#[test]
fn a_logical_run_draws_each_shots_coins_from_its_physical_seed() {
    let seed = 11;
    let shots = ShotSet::range(0..=7).expect("0..=7 is not empty");
    let program = FlipALogicalCoin {
        needs: Needs::default(),
    };
    let run =
        (Driver::new(program, shots.clone()) >> RepetitionCode::default() >> toy_simulator(seed))
            .expect("the toy runs the code's gadgets");
    let (output, _) = run.wait();

    for (&j, outcome) in shots.iter().zip(&output) {
        assert_eq!(outcome.shot, j);
        // The physical shot's first coin, read directly.
        let physical = coin_then_feedback(&mut toy_simulator(seed), j);
        assert_eq!(outcome.value, physical[0].value, "shot {j}");
    }
    // The shots drew different coins, so each read its own seed.
    let ones = output.iter().filter(|o| o.value == Value::One).count();
    assert!(0 < ones && ones < output.len(), "{output:?}");
}

/// The code is built only against an executor that runs its gadgets: against
/// a toy without `cx`, which `Coin`'s gadget sends, it is refused when
/// composed, before any shot is made.
#[test]
fn the_code_is_built_only_against_an_executor_that_runs_its_gadgets() {
    use OperandKind::{Block, Tag};
    let without_cx = InstructionSet::new("toy without cx")
        .with_action("coin", [Block], toy::action("coin"))
        .with_action("x", [Block], toy::action("x"))
        .with_action("measure", [Block, Tag], toy::action("measure"));
    let downstream = toy_simulator_described_with(0, without_cx);
    let refused = compose(RepetitionCode::default(), downstream).err();
    let errors = refused.map(|faults| faults.0.into_iter().map(|f| f.error).collect());
    assert_eq!(
        errors,
        Some(vec![BuildError::Instructions(vec![
            InstructionMismatch::Missing {
                mnemonic: "cx".into()
            }
        ])])
    );
}

/// The code's instructions are a type, and it still describes them: a
/// driver that states the instructions it will send is composed with the
/// code when the code has them all, and is refused when composed, before
/// any shot is made, when it lacks one.
#[test]
fn a_driver_is_composed_only_with_a_code_that_has_the_instructions_it_needs() {
    let composes = |program: InstructionSet| {
        let needs = Needs::default().with_instructions(Some(Arc::new(program)));
        let program = FlipALogicalCoin { needs };
        Driver::new(program, ShotSet::one(0)) >> RepetitionCode::default() >> toy_simulator(0)
    };
    let program = InstructionSet::new("program")
        .with_action("coin", [OperandKind::Block], logical_action("coin"))
        .with_action(
            "measure_z",
            [OperandKind::Block, OperandKind::Tag],
            logical_action("measure_z"),
        );
    assert!(composes(program.clone()).is_ok());
    let t = Action::new([1], [1]).rotate(Pauli::z(0), Angle::Literal(FRAC_PI_4));
    let refused = composes(program.with_action("t", [OperandKind::Block], t)).err();
    let errors: Option<Vec<_>> =
        refused.map(|faults| faults.0.into_iter().map(|f| f.error).collect());
    let Some([BuildError::Unmet(unmet)]) = errors.as_deref() else {
        panic!("{errors:?}")
    };
    assert_eq!(unmet.need, Need::Instructions);
    assert_eq!(
        unmet.mismatches,
        [InstructionMismatch::Missing {
            mnemonic: "t".into()
        }]
    );
}

/// Each description names every instruction it stands for, with its
/// operands: every logical instruction is a call of [`logical_set`], and
/// every physical instruction of every gadget a call of
/// [`gadget_instructions`]. The matches below are exhaustive, so a new
/// variant does not compile until it is described here too.
#[test]
fn each_description_names_every_instruction_it_stands_for() {
    use Operand::{Block, Tag};
    let logical_call = |instruction| match instruction {
        Logical::X(b) => ("x", vec![Block(b)]),
        Logical::Coin(b) => ("coin", vec![Block(b)]),
        Logical::MeasureZ { block, tag } => ("measure_z", vec![Block(block), Tag(tag)]),
    };
    let physical_call = |op| match op {
        Op::X(q) => ("x", vec![Block(q)]),
        Op::Coin(q) => ("coin", vec![Block(q)]),
        Op::Cx(c, t) => ("cx", vec![Block(c), Block(t)]),
        Op::Measure { qubit, tag } => ("measure", vec![Block(qubit), Tag(tag)]),
        Op::Reset(_) => unreachable!("no gadget resets"),
    };
    let every_logical = [
        Logical::X(1),
        Logical::Coin(1),
        Logical::MeasureZ { block: 1, tag: 0 },
    ];
    for instruction in every_logical {
        let (mnemonic, operands) = logical_call(instruction);
        assert!(
            logical_set().call(mnemonic, operands).is_ok(),
            "{instruction:?}"
        );
        for op in gadget(instruction) {
            let (mnemonic, operands) = physical_call(op);
            assert!(
                gadget_instructions().call(mnemonic, operands).is_ok(),
                "{op:?} in the gadget of {instruction:?}"
            );
        }
    }
}

/// The code composed with the toy offers a logical qubit per block of three
/// of the toy's qubits: 8 qubits hold 2 blocks, and a call on a third is
/// refused.
#[test]
fn the_code_offers_a_third_of_the_qubits() {
    let mut code = compose(RepetitionCode::default(), toy_simulator(0))
        .expect("the toy runs the code's gadgets");
    assert_eq!(code.capabilities().qubits, Some(2));
    let refused = code.call(on_shot_0(&[Logical::X(0), Logical::X(2)])).err();
    let Some(Rejected::Instruction { at: 1, reason }) = refused else {
        panic!("block 2 is refused: {refused:?}");
    };
    assert_eq!(reason, "2 logical qubits", "the code refuses it");
}

/// A logical instruction outside the code's blocks is refused before anything
/// goes down; a physical instruction the downstream executor refuses is
/// reported at the logical instruction it came from.
#[test]
fn a_refusal_names_the_logical_instruction() {
    let mut code = Repetition3 {
        inner: BatchRecorder {
            inner: toy_simulator(0),
            batches: Vec::new(),
        },
        blocks: 2,
    };
    let refused = code.call(on_shot_0(&[Logical::X(0), Logical::X(2)])).err();
    assert!(matches!(refused, Some(Rejected::Instruction { at: 1, .. })));
    assert!(code.inner.batches.is_empty(), "nothing went down");

    // A code that claims a third block, on 8 qubits: block 2's `Coin`
    // sends `Cx(6, 8)`, the 6th physical instruction, which the toy refuses.
    code.blocks = 3;
    let refused = (code.call(on_shot_0(&[Logical::X(0), Logical::Coin(2)]))).err();
    assert!(matches!(refused, Some(Rejected::Instruction { at: 1, .. })));
}

/// Closing the code closes the shot it wraps, and the shot's
/// `Closed` comes back up unchanged: only a refused instruction is renamed
/// to its logical origin.
#[test]
fn closing_the_code_closes_what_it_wraps() {
    let mut code = Repetition3 {
        inner: toy_simulator(0),
        blocks: 2,
    };
    conformance::rejects_calls_once_closed::<InstructionBatch<Logical>, _>(
        &mut code,
        on_shot_0(&[Logical::X(0)]),
    );
}

/// The toy refuses a batch that reads two qubits under one tag, at the
/// instruction that repeats it, and accepts a tag used again in a later
/// batch: the conformance kit's checks of readout tags. The code needs no
/// check of its own: its logical tags map to distinct physical ones, so the
/// toy refuses a logical repeat, at the logical instruction.
#[test]
fn the_toy_and_the_code_refuse_a_tag_repeated_within_a_batch() {
    let read = |qubit, tag| Op::Measure { qubit, tag };
    let (mut toy, shot_0) = (toy_simulator(0), ShotSet::one(0));
    let (a, b, a_again) = (read(0, 0), read(1, 1), read(2, 0));
    conformance::refuses_a_tag_repeated_within_a_batch(&mut toy, &shot_0, &a, &b, &a_again);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut toy,
        &shot_0,
        &a,
        &Op::X(1),
        &a_again,
    );
    let mut code = Repetition3 {
        inner: toy_simulator(0),
        blocks: 2,
    };
    let read = |block, tag| Logical::MeasureZ { block, tag };
    let (a, b, a_again) = (read(0, 0), read(1, 1), read(1, 0));
    conformance::refuses_a_tag_repeated_within_a_batch(&mut code, &shot_0, &a, &b, &a_again);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut code,
        &shot_0,
        &a,
        &Logical::X(1),
        &a_again,
    );
}

/// A static program of logical batches, run in order on every shot.
struct LogicalBatches(Vec<Vec<Logical>>);

impl Drive for LogicalBatches {
    type Instruction = Logical;
    type Output = Vec<ShotOutcome>;

    async fn drive<E: Executor<InstructionBatch<Logical>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let mut outcomes = Vec::new();
        for batch in &self.0 {
            let reply = executor.call(InstructionBatchRef {
                shots,
                instructions: batch,
            });
            wait(reply.expect("the code runs the batch"), &mut outcomes)
                .expect("the toy never fails a batch");
        }
        outcomes
    }
}

/// A program that tosses a logical coin on block 0 and flips block 1, then
/// reads block 1 under tag 1 before block 0 under tag 0.
fn toss_flip_read() -> LogicalBatches {
    LogicalBatches(vec![
        vec![Logical::Coin(0), Logical::X(1)],
        vec![
            Logical::MeasureZ { block: 1, tag: 1 },
            Logical::MeasureZ { block: 0, tag: 0 },
        ],
    ])
}

/// [`toss_flip_read`], then block 1 read again under tag 2, in the same
/// batch: the logical tags are read 1, 0, 2, in an order neither
/// ascending nor descending.
fn toss_flip_read_thrice() -> LogicalBatches {
    let LogicalBatches(mut batches) = toss_flip_read();
    let reads = batches
        .last_mut()
        .expect("toss_flip_read ends with its reads");
    reads.push(Logical::MeasureZ { block: 1, tag: 2 });
    LogicalBatches(batches)
}

/// A program that tosses a logical coin on block 0 and flips block 1, then
/// reads block 0 under tag 0 and flips block 1 back in one batch, then
/// reads block 1 under tag 1: a runtime that feeds a readout forward
/// flushes twice in the second batch, a run then a send.
fn toss_flip_read_unflip_read() -> LogicalBatches {
    LogicalBatches(vec![
        vec![Logical::Coin(0), Logical::X(1)],
        vec![Logical::MeasureZ { block: 0, tag: 0 }, Logical::X(1)],
        vec![Logical::MeasureZ { block: 1, tag: 1 }],
    ])
}

/// `outcomes` by shot, each shot's in the order the chain gave them: the
/// order of the shots depends on how the chain's adapters split them, but
/// each shot's order is compared, since a code gives its logical outcomes
/// in the order it decodes them.
fn by_shot(mut outcomes: Vec<ShotOutcome>) -> Vec<ShotOutcome> {
    outcomes.sort_by_key(|outcome| outcome.shot);
    outcomes
}

/// The code written as a logic, [`lower_then_decode`], with one state and
/// with a state per shot, on shots that are not a range: the program reads
/// through it what it reads through the code's own adapter, in the same
/// order, with each shot's coins drawn from that shot's seed below.
#[test]
fn the_code_as_a_logic_reads_what_its_adapter_reads() {
    let seed = 11;
    let shots = ShotSet::new(vec![6, 2, 9, 4, 13]).expect("distinct shots");
    let code = || {
        adapter_fn(|_| Ok(()), lower_then_decode)
            .accepts(logical_set())
            .sends(gadget_instructions())
            .check(within(toy::QUBITS / 3))
    };
    let (by_adapter, _) = (Driver::new(toss_flip_read(), shots.clone())
        >> RepetitionCode::default()
        >> toy_simulator(seed))
    .expect("the toy runs the code's gadgets")
    .wait();
    let (one_state, _) =
        (Driver::new(toss_flip_read(), shots.clone()) >> code() >> toy_simulator(seed))
            .expect("the toy runs the logic's gadgets")
            .wait();
    let (per_shot, _) = (Driver::new(toss_flip_read(), shots.clone())
        >> code().shots(OnePerShot)
        >> toy_simulator(seed))
    .expect("the toy runs the logic's gadgets")
    .wait();
    let by_adapter = by_shot(by_adapter);
    assert_eq!(by_adapter.len(), 10, "two readouts per shot");
    let ones = by_adapter.iter().filter(|o| o.value == Value::One).count();
    assert!(
        (6..10).contains(&ones),
        "block 1 reads 1 on every shot, and block 0 coins that differ: {by_adapter:?}"
    );
    assert_eq!(by_shot(one_state), by_adapter);
    assert_eq!(by_shot(per_shot), by_adapter);
}

/// A scheduler's logic: it measures each run of readouts in a batch in
/// rounds, each block's first qubit, then each one's second, then each
/// one's third, as a device that measures in parallel does, and sends the
/// rest as it is.
fn measure_in_rounds(
    (): &mut (),
    input: &ProgramInput<'_, Op>,
    below: &mut Below<'_, Op>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    let mut batch = input.instructions().to_vec();
    for readouts in batch.chunk_by_mut(|a, b| a.tag().is_some() && b.tag().is_some()) {
        readouts.sort_by_key(|op| match op {
            Op::Measure { qubit, .. } => qubit % 3,
            _ => 0,
        });
    }
    below.send(&batch)
}

/// The code as a logic over a scheduler that measures the blocks in
/// rounds, so that a block's three readouts are not read together: the
/// program, which reads its logical tags 1, 0, then 2, reads what it reads
/// through the code's own adapter, in the same order.
#[test]
fn the_code_as_a_logic_decodes_readouts_in_any_order() {
    let seed = 11;
    let shots = ShotSet::new(vec![6, 2, 9]).expect("distinct shots");
    let (by_adapter, _) = (Driver::new(toss_flip_read_thrice(), shots.clone())
        >> RepetitionCode::default()
        >> toy_simulator(seed))
    .expect("the toy runs the code's gadgets")
    .wait();
    let code = adapter_fn(|_| Ok(()), lower_then_decode)
        .accepts(logical_set())
        .sends(gadget_instructions());
    let (in_rounds, _) = (Driver::new(toss_flip_read_thrice(), shots)
        >> code
        >> adapter_fn(|_| Ok(()), measure_in_rounds)
        >> toy_simulator(seed))
    .expect("the toy runs the logic's gadgets")
    .wait();
    assert_eq!(by_shot(in_rounds), by_shot(by_adapter));
}

/// A noise adapter's logic: runs each call below, and flips each block's
/// first vote, the readout under each physical tag `3t`, so that each
/// block's three votes disagree, two against one.
fn flip_first_votes(
    (): &mut (),
    input: &ProgramInput<'_, Op>,
    below: &mut Below<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    let flipped = |value: Value| match value {
        Value::Zero => Value::One,
        Value::One => Value::Zero,
        lost @ Value::Error(_) => lost,
    };
    let read = below.run(input.instructions())?;
    outcomes.extend(read.into_iter().map(|outcome| match outcome.tag % 3 {
        0 => ShotOutcome::new(outcome.shot, outcome.tag, flipped(outcome.value)),
        _ => outcome,
    }));
    Ok(())
}

/// The code as a logic over noise that flips each block's first vote, on
/// shots whose blocks read both 0 and 1: each logical readout is the
/// majority of its block's votes, so the program reads what it reads with
/// no noise.
#[test]
fn the_code_as_a_logic_reads_the_majority_of_votes_that_disagree() {
    let seed = 11;
    let shots = ShotSet::new(vec![6, 2, 9, 4, 13]).expect("distinct shots");
    let code = || {
        adapter_fn(|_| Ok(()), lower_then_decode)
            .accepts(logical_set())
            .sends(gadget_instructions())
    };
    let (noiseless, _) =
        (Driver::new(toss_flip_read(), shots.clone()) >> code() >> toy_simulator(seed))
            .expect("the toy runs the logic's gadgets")
            .wait();
    let reads = |value| noiseless.iter().any(|o| o.value == value);
    assert!(
        reads(Value::Zero) && reads(Value::One),
        "the blocks read both 0 and 1: {noiseless:?}"
    );
    let (noisy, _) = (Driver::new(toss_flip_read(), shots)
        >> code()
        >> adapter_fn(|_| Ok(()), flip_first_votes)
        >> toy_simulator(seed))
    .expect("the toy runs the logic's gadgets")
    .wait();
    assert_eq!(by_shot(noisy), by_shot(noiseless));
}

/// The code as a logic refuses a call on a block beyond its own at the
/// logical instruction, before anything goes below, as `Repetition3`
/// does.
#[test]
fn the_code_as_a_logic_refuses_a_block_beyond_its_own_as_repetition3_does() {
    let batch = [Logical::X(0), Logical::X(2)];
    let mut code = (adapter_fn(|_| Ok(()), lower_then_decode).check(within(2)) >> toy_simulator(0))
        .expect("the toy runs the logic's gadgets");
    let mut repetition3 = Repetition3 {
        inner: toy_simulator(0),
        blocks: 2,
    };
    let refused = code.call(on_shot_0(&batch)).err();
    assert!(matches!(refused, Some(Rejected::Instruction { at: 1, .. })));
    assert_eq!(refused, repetition3.call(on_shot_0(&batch)).err());
}

/// The code as a runtime that owns its backend for each shot's life, as a
/// Qodec runtime does, made an adapter with no change to it, on shots that
/// are not a range, with a batch the runtime flushes twice, a run then a
/// send: the program reads through it what it reads through the code
/// written as a logic lent its way below, each shot's coins drawn from
/// that shot's seed below.
#[test]
fn a_runtime_that_owns_its_backend_reads_what_the_code_as_a_logic_reads() {
    let seed = 11;
    let shots = ShotSet::new(vec![6, 2, 9, 4, 13]).expect("distinct shots");
    let as_a_logic = adapter_fn(|_| Ok(()), lower_then_decode)
        .accepts(logical_set())
        .sends(gadget_instructions())
        .check(within(toy::QUBITS / 3))
        .shots(OnePerShot);
    let as_a_runtime = owning_adapter_fn(instantiated(CodePlan::default()), through_the_runtime)
        .accepts(logical_set())
        .sends(gadget_instructions())
        .check(within(toy::QUBITS / 3))
        .shots(OnePerShot);
    let (lent, _) = (Driver::new(toss_flip_read_unflip_read(), shots.clone())
        >> as_a_logic
        >> toy_simulator(seed))
    .expect("the toy runs the logic's gadgets")
    .wait();
    let (owned, _) =
        (Driver::new(toss_flip_read_unflip_read(), shots) >> as_a_runtime >> toy_simulator(seed))
            .expect("the toy runs the runtime's gadgets")
            .wait();
    let lent = by_shot(lent);
    assert_eq!(lent.len(), 10, "two readouts per shot");
    let ones = lent.iter().filter(|o| o.value == Value::One).count();
    assert!(
        (1..5).contains(&ones),
        "block 1 reads 0 on every shot, and block 0 coins that differ: {lent:?}"
    );
    assert_eq!(by_shot(owned), lent);
}

/// The code as a runtime that owns its backend, served by one state on
/// several shots, by mistake: its first batch, which reads nothing, goes
/// below, but its first batch that reads fails the call, as the run below
/// gives the reads of every shot where the glue expects one shot's.
#[test]
fn a_runtime_with_one_state_fails_its_first_reading_call() {
    let shots = ShotSet::new(vec![6, 9, 4, 13]).expect("distinct shots");
    let mut executor = (owning_adapter_fn(instantiated(CodePlan::default()), through_the_runtime)
        .accepts(logical_set())
        .sends(gadget_instructions())
        .check(within(toy::QUBITS / 3))
        >> toy_simulator(11))
    .expect("the toy runs the runtime's gadgets");
    let mut call = |instructions: &[Logical]| {
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions,
        };
        let reply = executor.call(batch).expect("the call is in the code's set");
        wait(reply, &mut Vec::new())
    };
    call(&[Logical::Coin(0), Logical::X(1)]).expect("a batch that reads nothing goes below");
    let failed = call(&[Logical::MeasureZ { block: 0, tag: 0 }]);
    assert_eq!(failed, Err("readouts: 3, reads below: 12".to_owned()));
}

/// The code's runtime with a bug, that loses the last result of each
/// batch.
struct LosesItsLastResult(CodeRuntime<Lower>);

impl Backend<Logical> for LosesItsLastResult {
    type Error = String;

    fn execute_batch(&mut self, instructions: &[Logical]) -> Result<BatchResult, String> {
        let mut batch = self.0.execute_batch(instructions)?;
        batch.results.pop();
        Ok(batch)
    }
}

/// A runtime with a bug gives one result for a batch that reads twice:
/// the glue fails the call, rather than leave the shot short of an
/// outcome.
#[test]
fn glue_given_fewer_results_than_readouts_fails_the_call() {
    let plan = CodePlan::default();
    let state =
        move |_, below| Ok::<_, Rejected>(LosesItsLastResult(plan.instantiate(Lower(below))));
    let mut executor = (owning_adapter_fn(state, through_the_runtime)
        .accepts(logical_set())
        .sends(gadget_instructions())
        .check(within(toy::QUBITS / 3))
        .shots(OnePerShot)
        >> toy_simulator(0))
    .expect("the toy runs the runtime's gadgets");
    let batch = [
        Logical::MeasureZ { block: 0, tag: 0 },
        Logical::MeasureZ { block: 1, tag: 1 },
    ];
    let reply = (executor.call(on_shot_0(&batch))).expect("the call is in the code's set");
    let failed = wait(reply, &mut Vec::new());
    assert_eq!(
        failed,
        Err("readouts: 2, results from the runtime: 1".to_owned())
    );
}
