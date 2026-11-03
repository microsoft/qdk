// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::tests::test_double::TestDoubleExecutor;
use crate::tests::{Bits, bits_set, block_on, flip_and_toss, shots};
use crate::{Close, Driver, Needs, ShotId, StaticDriver, StaticProgram, static_driver};

/// A static program, given to a chain of the static driver as any job
/// is, its shots a slice: the run gives the outcomes and the report that
/// the static driver gives driving the same executor directly.
#[test]
fn a_static_program_given_to_a_chain_runs_as_the_driver_runs_it_on_the_executor_directly() {
    let bits = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let program = || {
        StaticProgram::new(
            Needs::default().with_instruction_set(Some(Arc::new(bits_set()))),
            flip_and_toss(&bits_set()),
        )
    };
    let ids: Vec<ShotId> = vec![2, 5, 6, 11];
    let chain = (static_driver() >> bits()).expect("the chain runs the bits' set");
    let given = chain
        .run(program(), ids.as_slice())
        .expect("shots, needs met")
        .wait();
    let mut executor = bits();
    let output = block_on(StaticDriver.drive(program(), &mut executor, &shots(&ids)));
    assert_eq!(given, (output, executor.close()));
}
