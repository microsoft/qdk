// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel};
use crate::{
    CallInput, Capabilities, Describe, Driver, DriverExt, Executor, InstructionBatch,
    InstructionBatchRef, Outcomes, Program, Rejected, ShotSet, executor_fn, wait_async,
};

/// A program of reads, with no content: its driver chooses how often it
/// reads each shot. It gives whether each call was taken.
struct Reads;

impl Program for Reads {
    type Instruction = bool;
    type Output = Result<(), Rejected>;
}

/// A driver that reads every shot of its job once, and adds no need.
struct ReadsOnce;

impl Driver<Reads> for ReadsOnce {
    async fn drive<E>(&self, _: Reads, executor: &mut E, shots: &ShotSet) -> Result<(), Rejected>
    where
        E: Executor<InstructionBatch<bool>>,
    {
        let reply = executor.call(InstructionBatchRef {
            shots,
            instructions: &[true],
        })?;
        wait_async(reply).await.1.expect("the double does not fail");
        Ok(())
    }
}

/// A driver that states no needs of its own runs a job on an
/// executor that offers no resumable shots: the default needs ask for nothing
/// but the run's shots, held at once.
#[test]
fn a_drivers_default_needs_ask_no_resumable_shots() {
    let model = TestDoubleModel {
        without_resumable_shots: true,
        ..TestDoubleModel::default()
    };
    let chain = (ReadsOnce.into_chain() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (output, _) = chain
        .run(Reads, [4, 9])
        .expect("the default needs ask no resumable shots")
        .wait();
    assert_eq!(output, Ok(()));
}

/// A runtime of the user's, which drives one program type, joins a chain
/// by `.into_chain()` with no type named, and the chain runs its program.
#[test]
fn a_driver_of_one_program_type_joins_a_chain_by_into_chain() {
    let simulator = executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    );
    let chain =
        (ReadsOnce.into_chain() >> simulator).expect("the simulator takes any batch of `bool`s");
    let (output, _) = chain.run(Reads, [4]).expect("one shot, held").wait();
    assert_eq!(output, Ok(()));
}

/// A circuit: its instructions, sent in one call.
struct Circuit(Vec<bool>);

impl Program for Circuit {
    type Instruction = bool;
    type Output = Result<(), Rejected>;
}

/// A single instruction.
struct Single(bool);

impl Program for Single {
    type Instruction = bool;
    type Output = Result<(), Rejected>;
}

/// A runtime that drives two program forms: a [`Circuit`] and a
/// [`Single`] instruction.
struct TwoForms;

impl Driver<Circuit> for TwoForms {
    async fn drive<E>(
        &self,
        circuit: Circuit,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Result<(), Rejected>
    where
        E: Executor<InstructionBatch<bool>>,
    {
        let reply = executor.call(InstructionBatchRef {
            shots,
            instructions: &circuit.0,
        })?;
        wait_async(reply)
            .await
            .1
            .expect("the simulator does not fail");
        Ok(())
    }
}

impl Driver<Single> for TwoForms {
    async fn drive<E>(
        &self,
        single: Single,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Result<(), Rejected>
    where
        E: Executor<InstructionBatch<bool>>,
    {
        self.drive(Circuit(vec![single.0]), executor, shots).await
    }
}

/// A runtime of two program forms joins a chain naming one of them,
/// `DriverExt::<P>::into_chain(runtime)`, only so that the call resolves:
/// the chain runs a program of either form.
#[test]
fn a_driver_of_several_program_types_joins_a_chain_naming_one() {
    let chain = || {
        let simulator = executor_fn(
            Capabilities::default(),
            |_| Ok(()),
            |(): &mut (), _: &CallInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
        );
        (DriverExt::<Circuit>::into_chain(TwoForms) >> simulator)
            .expect("the simulator takes any batch of `bool`s")
    };
    let (circuit, _) = chain()
        .run(Circuit(vec![true, false]), [4])
        .expect("one shot, held")
        .wait();
    let (instruction, _) = chain()
        .run(Single(true), [4])
        .expect("one shot, held")
        .wait();
    assert_eq!((circuit, instruction), (Ok(()), Ok(())));
}

/// A driver that reads every shot of its job, then reads each again in a
/// second call, continuing it, and adds no need.
struct ReadsTwice;

impl Driver<Reads> for ReadsTwice {
    async fn drive<E>(&self, _: Reads, executor: &mut E, shots: &ShotSet) -> Result<(), Rejected>
    where
        E: Executor<InstructionBatch<bool>>,
    {
        for _ in 0..2 {
            let reply = executor.call(InstructionBatchRef {
                shots,
                instructions: &[true],
            })?;
            wait_async(reply)
                .await
                .1
                .expect("the simulator does not fail");
        }
        Ok(())
    }
}

/// A driver that continues its shot in a second call but does not state
/// `resumable_shots`, on a simulator that does not offer them: no call
/// checks the need, so both calls run; only the needs a driver states
/// refuse its job.
#[test]
fn resumable_shots_a_driver_does_not_state_are_checked_by_no_call() {
    let simulator = executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    );
    assert!(
        !simulator.capabilities().resumable_shots,
        "the simulator offers no resumable shots"
    );
    let chain =
        (ReadsTwice.into_chain() >> simulator).expect("the simulator takes any batch of `bool`s");
    let (output, _) = chain
        .run(Reads, [4])
        .expect("the driver states no resumable shots")
        .wait();
    assert_eq!(output, Ok(()));
}
