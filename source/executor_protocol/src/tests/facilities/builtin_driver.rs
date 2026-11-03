// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::tests::test_double::TestDoubleExecutor;
use crate::tests::{Bits, ByHand, bits_set, block_on, flip_and_toss, shots};
use crate::{AdaptiveProgram, Close, Needs, ShotId, driver};

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
