// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::tests::declared;
use crate::tests::test_double::{Model, TestDoubleExecutor};
use crate::{
    Capabilities, Close, ComposedJob, Describe, Executor, ImmediateReply, InstructionBatch,
    InstructionBatchRef, InstructionSet, Needs, OperandKind, Rejected, ShotContext, ShotId,
    ShotOutcome, ShotSet, StaticProgram, TypedInstruction, Value, compose,
};

/// A typed instruction: each names itself by its variant.
#[derive(Clone, Copy)]
enum Op {
    Reset,
    X,
    Measure,
}

impl TypedInstruction for Op {}

/// Makes shots of one bit that run [`Op`]s, and describe them as a set,
/// `reset` first.
struct OneBit;

impl Describe for OneBit {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_instruction_set(Some(Arc::new(declared(
            InstructionSet::new("one bit")
                .with("reset", [])
                .with("x", [])
                .with("measure", [OperandKind::Tag]),
        ))))
    }
}

impl Model for OneBit {
    type Instruction = Op;
    type State = OneBitState;

    fn state(&self, context: &ShotContext) -> Result<OneBitState, String> {
        Ok(OneBitState {
            shot: context.shot(),
            bit: false,
        })
    }
}

/// A shot of [`OneBit`].
struct OneBitState {
    shot: ShotId,
    bit: bool,
}

impl Executor<InstructionBatch<Op>> for OneBitState {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        batch.shots.check_only(self.shot)?;
        let mut outcomes = Vec::new();
        for op in batch.instructions {
            match op {
                Op::Reset => self.bit = false,
                Op::X => self.bit = !self.bit,
                Op::Measure => outcomes.push(ShotOutcome::new(self.shot, 0, self.bit.into())),
            }
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

impl Close for OneBitState {
    type Report = ();

    fn close(&mut self) {}
}

/// A typed program that resets, flips and reads its bit, needing `needs`.
fn reset_flip_read(needs: Needs) -> StaticProgram<Op> {
    StaticProgram::new(needs, vec![vec![Op::Reset, Op::X, Op::Measure]])
}

/// A typed program states its set in another order than the executor's,
/// which does not concern it, since its instructions name themselves: it
/// composes with the executor, with no adapter, and its `X` runs as `x`.
#[test]
fn a_typed_program_in_another_order_runs_its_instructions_as_they_are() {
    let set = declared(
        InstructionSet::new("program")
            .with("x", [])
            .with("reset", [])
            .with("measure", [OperandKind::Tag]),
    );
    let program = reset_flip_read(Needs::default().with_instruction_set(Some(Arc::new(set))));
    let (outcomes, _) = compose(
        ComposedJob::new(program, ShotSet::one(1)),
        TestDoubleExecutor::with_model(OneBit, 7),
    )
    .expect("the executor accepts the program's set")
    .wait();
    let outcomes = outcomes.expect("the bit neither refuses nor fails");
    assert_eq!(outcomes[0].value, Value::One);
}

/// Makes the shots of [`OneBit`], but describes no instruction set.
struct Undescribed;

impl Describe for Undescribed {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Model for Undescribed {
    type Instruction = Op;
    type State = OneBitState;

    fn state(&self, context: &ShotContext) -> Result<OneBitState, String> {
        OneBit.state(context)
    }
}

/// A typed program that states no set composes with an executor that
/// describes none: the compiler checks that both run the same type, and
/// an instruction the executor does not run is refused at run time.
#[test]
fn a_typed_job_and_an_executor_that_describe_no_set_compose() {
    let program = reset_flip_read(Needs::default());
    let (outcomes, _) = compose(
        ComposedJob::new(program, ShotSet::one(1)),
        TestDoubleExecutor::with_model(Undescribed, 7),
    )
    .expect("a typed boundary needs no description")
    .wait();
    let outcomes = outcomes.expect("the bit neither refuses nor fails");
    assert_eq!(outcomes[0].value, Value::One);
}
