// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel};
use crate::tests::{Bits, ByHand, bits_set, block_on, flip_and_toss, shots};
use crate::{
    AdaptiveProgram, ChainHead, Close, Driver, Executor, InstructionBatch, InstructionBatchRef,
    Needs, Rejected, ShotId, ShotSet, driver, wait_async,
};

/// An adaptive program, given to a chain of the crate's driver as any job
/// is, its shots a slice: the driver lets it drive, so it gives the
/// outcomes and the report that it gives driving the same executor itself.
#[test]
fn an_adaptive_program_given_to_a_chain_runs_as_it_runs_driving_the_executor_itself() {
    let bits = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let by_hand = || ByHand {
        needs: Needs::default().with_instruction_set(Some(Arc::new(bits_set()))),
        batches: flip_and_toss(&bits_set()),
    };
    let ids: Vec<ShotId> = vec![2, 5, 6, 11];
    let chain = (driver() >> bits()).expect("the chain runs the bits' set");
    let given = chain
        .run(by_hand(), ids.as_slice())
        .expect("shots, needs met")
        .wait();
    let mut executor = bits();
    let output = block_on(by_hand().drive(&mut executor, &shots(&ids)));
    assert_eq!(given, (output, executor.close()));
}

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
/// but a state per shot.
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
