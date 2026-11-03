// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod builtin_driver;

use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel};
use crate::{
    ChainHead, Driver, Executor, InstructionBatch, InstructionBatchRef, Rejected, ShotSet,
    wait_async,
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
