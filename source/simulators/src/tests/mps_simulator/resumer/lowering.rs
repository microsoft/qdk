// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering's rules: each call acts on the site the layout gives its
//! qubit; a shot that reaches a call this MPS simulation method does not
//! run fails, naming it; and a call no shot reaches fails nothing.

use crate::bytecode::Instruction;
use crate::mps_simulator::resumer::layout::QubitOrder;
use crate::mps_simulator::{MpsResumer, MpsResumerError};
use crate::qir::{QirInstruction, QirProgram, QirResumeError, QirShotId};
use tensornet::reference::ExactReference;

use super::super::programs::{
    branch, correlated_noise, gate1, gate2, jump, measure, mz, peek_loss, program, program_with,
    readout_noise, reset, ret, set,
};
use super::{ids, results, run};
use crate::bytecode::encoding::{OPID_CX, OPID_CZ, OPID_MRESETZ, OPID_MZ, OPID_X};

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

/// The values of qubits 0 to 3 in [`moved`]'s programs.
const MOVED_VALUES: [u8; 4] = [1, 0, 0, 1];

/// A program on four qubits whose `cz`s (no change to a basis state) make
/// the layout move every qubit to a site that, in the identity order,
/// holds a qubit of the other value; `x`s give the qubits
/// [`MOVED_VALUES`]; then `tail`, into 4 results. So a call lowered onto
/// any site but its qubit's reads or resets a qubit of the other value.
fn moved(tail: &[Instruction<u64>]) -> QirProgram {
    let mut block = vec![gate1(OPID_X, 0), gate1(OPID_X, 3)];
    for (a, b) in [(1, 3), (3, 0), (0, 2)] {
        block.extend([
            gate2(OPID_CZ, a, b),
            gate2(OPID_CZ, a, b),
            gate2(OPID_CZ, a, b),
        ]);
    }
    block.extend_from_slice(tail);
    block.push(ret());
    let program = program_with(4, 4, &[], &[&block]);
    let order = QubitOrder::for_program(&program);
    for qubit in 0..4 {
        let site = order.site(qubit);
        assert_ne!(
            MOVED_VALUES[qubit as usize], MOVED_VALUES[site],
            "qubit {qubit} sits on site {site}, whose qubit in the identity order has the other value"
        );
    }
    program
}

/// A measurement acts on its qubit's site: `mz` of each qubit, after the
/// layout moved them all, reads each qubit's own value.
#[test]
fn a_measurement_reads_its_qubit_where_the_order_placed_it() {
    let program = moved(&[mz(0), mz(1), mz(2), mz(3)]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("the program runs");
    assert_eq!(records, vec![results(&MOVED_VALUES)]);
}

/// A measure-reset acts on its qubit's site: `mresetz` of qubit 3, after
/// the layout moved it, reads its 1, and a later `mz` of it reads 0.
#[test]
fn a_measure_reset_acts_on_its_qubit_where_the_order_placed_it() {
    let program = moved(&[measure(OPID_MRESETZ, 3, 0), measure(OPID_MZ, 3, 1)]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("the program runs");
    assert_eq!(records, vec![results(&[1, 0, 0, 0])]);
}

/// A reset acts on its qubit's site: `reset` of qubit 0, after the layout
/// moved it, leaves it at 0, and qubit 3 keeps its 1.
#[test]
fn a_reset_acts_on_its_qubit_where_the_order_placed_it() {
    let program = moved(&[reset(0), measure(OPID_MZ, 0, 0), measure(OPID_MZ, 3, 1)]);
    let records = run(
        &mut MpsResumer::new(ExactReference::new(), 7),
        &program,
        &ids(&[0]),
    )
    .expect("the program runs");
    assert_eq!(records, vec![results(&[0, 1, 0, 0])]);
}
