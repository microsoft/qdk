// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The lowering's refusals: a shot that reaches a call this MPS simulation
//! method does not run fails, naming it, and a call no shot reaches fails
//! nothing.

use crate::mps_simulator::{MpsResumer, MpsResumerError};
use crate::qir::{QirInstruction, QirProgram, QirResumeError, QirShotId};
use tensornet::reference::ExactReference;

use super::super::programs::{
    branch, correlated_noise, gate2, jump, peek_loss, program, program_with, readout_noise, ret,
    set,
};
use super::{ids, results, run};
use crate::bytecode::encoding::OPID_CX;

/// The refusal of a noise or loss call: its shot fails, naming the
/// instruction and the planned item.
fn assert_refused(program: &QirProgram, instruction: QirInstruction) {
    let error = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        program,
        &ids(&[3, 4]),
    )
    .expect_err("noise and loss are not run");
    let QirResumeError::Resumer { shot, error } = error else {
        panic!("the resumer refuses the call, no trap: {error:?}");
    };
    assert_eq!(shot, QirShotId(3));
    assert!(
        matches!(&error, MpsResumerError::Unsupported { instruction: called } if *called == instruction),
        "{error:?}"
    );
    assert!(error.to_string().contains("planned"), "{error}");
}

/// Readout noise is not run on an MPS yet: a shot that reaches it fails.
#[test]
fn a_readout_noise_call_fails_its_shot_naming_the_planned_item() {
    let noisy = program(1, &[&[readout_noise(0.1, 0), ret()]]);
    assert_refused(&noisy, QirInstruction::ReadoutNoise);
}

/// Correlated noise is not run on an MPS yet: a shot that reaches it fails.
#[test]
fn a_correlated_noise_call_fails_its_shot_naming_the_planned_item() {
    let noisy = program_with(1, 1, &[0], &[&[set(0, 0), correlated_noise(1, 0), ret()]]);
    assert_refused(&noisy, QirInstruction::CorrelatedNoise);
}

/// A loss check is not run on an MPS yet: a shot that reaches it fails.
#[test]
fn a_loss_check_fails_its_shot_naming_the_planned_item() {
    let lossy = program(1, &[&[peek_loss(0, 0), ret()]]);
    assert_refused(&lossy, QirInstruction::PeekLoss);
}

/// A noise call on a path no shot takes fails nothing: a call is refused
/// when a shot reaches it.
#[test]
fn a_noise_call_no_shot_reaches_fails_nothing() {
    let (noisy, clean) = (1, 2);
    let program = program(
        1,
        &[
            &[set(0, 0), branch(0, noisy, clean)],
            &[readout_noise(0.1, 0), jump(clean)],
            &[ret()],
        ],
    );
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("no shot reaches the noise");
    assert_eq!(records, vec![results(&[0])]);
}

/// A two-qubit gate on one qubit twice fails its shot, naming the qubit.
#[test]
fn a_gate_on_one_qubit_twice_fails_naming_the_qubit() {
    let program = program(2, &[&[gate2(OPID_CX, 1, 1), ret()]]);
    let error = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect_err("cx(1, 1) has no matrix on distinct qubits");
    assert!(matches!(
        error,
        QirResumeError::Resumer {
            error: MpsResumerError::RepeatedQubit { qubit: 1, .. },
            ..
        }
    ));
}
