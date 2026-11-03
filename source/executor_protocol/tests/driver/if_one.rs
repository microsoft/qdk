// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A program with a branch on a readout ([`IfOne`]), as data, and the
//! driver that runs it ([`IfOneDriver`]): what a user writes for a program
//! form that the crate's driver does not run.

use executor_protocol::{
    Driver, Executor, InstructionBatch, InstructionBatchRef, Needs, Program, ShotOutcome, ShotSet,
    Value, wait_async,
};

use crate::toy::Op;

/// A program with one branch on a readout: `first` runs on every shot,
/// then `then` runs on the shots whose readout `tag` is 1, each where
/// `first` left it.
pub struct IfOne {
    /// What its instructions need: its qubits, and anything else.
    pub needs: Needs,
    /// The batch every shot runs.
    pub first: Vec<Op>,
    /// The readout of `first` the branch reads.
    pub tag: u32,
    /// The batch the shots whose readout `tag` is 1 run next.
    pub then: Vec<Op>,
}

impl Program for IfOne {
    type Instruction = Op;
    /// The readouts of both batches, or why a call was refused or failed.
    type Output = Result<Vec<ShotOutcome>, String>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}

/// The driver of an [`IfOne`]: it sends `first` to every shot, reads each
/// shot's readout `tag`, and sends `then` to the shots that read 1.
pub struct IfOneDriver;

impl Driver<IfOne> for IfOneDriver {
    /// `then` continues the shots `first` ran, so the executor keeps each
    /// shot's state between calls, whatever the program states.
    fn needs(&self, _: &IfOne) -> Needs {
        Needs::default().with_resumable_shots(true)
    }

    async fn drive<E>(
        &self,
        program: IfOne,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Result<Vec<ShotOutcome>, String>
    where
        E: Executor<InstructionBatch<Op>>,
    {
        let first = InstructionBatchRef {
            shots,
            instructions: &program.first,
        };
        let reply = executor.call(first).map_err(|r| r.to_string())?;
        let (mut outcomes, end) = wait_async(reply).await;
        end?;
        let read_one = |j| {
            (outcomes.iter()).any(|o| o.shot == j && o.tag == program.tag && o.value == Value::One)
        };
        let Some(ones) = shots.subset(read_one) else {
            return Ok(outcomes);
        };
        let then = InstructionBatchRef {
            shots: &ones,
            instructions: &program.then,
        };
        let reply = executor.call(then).map_err(|r| r.to_string())?;
        let (then_read, end) = wait_async(reply).await;
        outcomes.extend(then_read);
        end.map(|()| outcomes)
    }
}
