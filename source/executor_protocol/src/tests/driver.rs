// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod builtin_driver;

use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel};
use crate::{
    CallInput, Capabilities, ChainHead, Describe, Driver, Executor, InstructionBatch,
    InstructionBatchRef, Outcomes, Rejected, ShotSet, executor_fn, wait_async,
};

/// A driver that reads every shot of its job once, and keeps the
/// default needs.
struct ReadsOnce;

impl Driver<()> for ReadsOnce {
    type Instruction = bool;
    type Output = Result<(), Rejected>;

    async fn drive<E>(&self, (): (), executor: &mut E, shots: &ShotSet) -> Self::Output
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
    let chain = (ChainHead::new(ReadsOnce) >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (output, _) = chain
        .run((), [4, 9])
        .expect("the default needs ask no resumable shots")
        .wait();
    assert_eq!(output, Ok(()));
}

/// A driver that reads every shot of its job, then reads each again in a
/// second call, continuing it, and keeps the default needs.
struct ReadsTwice;

impl Driver<()> for ReadsTwice {
    type Instruction = bool;
    type Output = Result<(), Rejected>;

    async fn drive<E>(&self, (): (), executor: &mut E, shots: &ShotSet) -> Self::Output
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
    let chain = (ChainHead::new(ReadsTwice) >> simulator)
        .expect("the simulator takes any batch of `bool`s");
    let (output, _) = chain
        .run((), [4])
        .expect("the driver states no resumable shots")
        .wait();
    assert_eq!(output, Ok(()));
}
