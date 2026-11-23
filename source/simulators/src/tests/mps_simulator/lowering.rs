// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering's rules: each call acts on the site the qubit order gives its
//! qubit; a rotation turns by its angle, sign included, and one by an angle
//! that is not finite is the engine's to refuse; a measurement in X
//! or Y reads, and leaves, the state in that basis; a shot that reaches a
//! call the MPS simulation method does not run fails, naming it; and a
//! call no shot reaches fails nothing.

use std::f64::consts::FRAC_PI_2;

use crate::bytecode::Instruction;
use crate::mps_simulator::qubit_order::QubitOrder;
use crate::mps_simulator::{MpsResumer, MpsResumerError};
use crate::qir::{QirInstruction, QirProgram, QirResumeError, QirShotId};
use tensornet::MpsEngineError;
use tensornet::reference::ExactReference;

use super::programs::{
    branch, correlated_noise, gate, gate1, gate2, jump, measure, mz, peek_loss, program,
    program_with, readout_noise, reset, ret, set,
};
use super::{ids, results, run};
use crate::bytecode::encoding::{
    OPID_CX, OPID_CZ, OPID_H, OPID_MRESETZ, OPID_MX, OPID_MY, OPID_MZ, OPID_RX, OPID_RY, OPID_S,
    OPID_S_ADJ, OPID_X,
};

/// A shot that reaches a call the MPS simulation method does not run yet
/// fails, naming the instruction and the planned item. Each row is a call:
/// readout noise, correlated noise, and a loss check.
#[test]
fn an_unsupported_call_fails_its_shot_naming_the_planned_item() {
    let rows = [
        (
            program(1, &[&[readout_noise(0.1, 0), ret()]]),
            QirInstruction::ReadoutNoise,
        ),
        (
            program_with(1, 1, &[0], &[&[set(0, 0), correlated_noise(1, 0), ret()]]),
            QirInstruction::CorrelatedNoise,
        ),
        (
            program(1, &[&[peek_loss(0, 0), ret()]]),
            QirInstruction::PeekLoss,
        ),
    ];
    for (program, instruction) in rows {
        let error = run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &program,
            &ids(&[3, 4]),
        )
        .expect_err("noise and loss are not run");
        let QirResumeError::Resumer { shot, error } = error else {
            panic!("{instruction:?}: the resumer refuses the call, no trap: {error:?}");
        };
        assert_eq!(shot, QirShotId(3), "{instruction:?}");
        assert!(
            matches!(&error, MpsResumerError::Unsupported { instruction: called } if *called == instruction),
            "{instruction:?}: {error:?}"
        );
        assert!(
            error.to_string().contains("planned"),
            "{instruction:?}: {error}"
        );
    }
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
/// the qubit order move every qubit to a site that, in the identity order,
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

/// Each call acts on its qubit's site, after the qubit order moved every
/// qubit. Each row is a call, and what the program records: `mz` of each
/// qubit reads each qubit's own value; `mresetz` of qubit 3 reads its 1,
/// and a later `mz` of it reads 0; `reset` of qubit 0 leaves it at 0, and
/// qubit 3 keeps its 1.
#[test]
fn each_call_acts_on_its_qubit_where_the_order_placed_it() {
    let rows = [
        (
            "a measurement",
            vec![mz(0), mz(1), mz(2), mz(3)],
            MOVED_VALUES,
        ),
        (
            "a measure-reset",
            vec![measure(OPID_MRESETZ, 3, 0), measure(OPID_MZ, 3, 1)],
            [1, 0, 0, 0],
        ),
        (
            "a reset",
            vec![reset(0), measure(OPID_MZ, 0, 0), measure(OPID_MZ, 3, 1)],
            [0, 1, 0, 0],
        ),
    ];
    for (call, tail, values) in rows {
        let records = run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &moved(&tail),
            &ids(&[0]),
        )
        .expect("the program runs");
        assert_eq!(records, vec![results(&values)], "{call}");
    }
}

/// A rotation turns by its angle, sign included. `Ry(θ)` is exp(−iθY/2),
/// so on |+⟩, `Ry(π/2)` gives |1⟩ and `Ry(−π/2)` gives |0⟩: `h`, then
/// `ry` by each angle, then `mz` reads 1, or 0, in every shot. A rotation
/// by the opposite angle reads the other value.
#[test]
fn a_rotation_turns_by_its_angle_sign_included() {
    for (angle, value) in [(FRAC_PI_2, 1), (-FRAC_PI_2, 0)] {
        let block = [gate1(OPID_H, 0), gate(OPID_RY, 0, 0, angle), mz(0), ret()];
        let records = run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &program(1, &[&block]),
            &QirShotId::first(16),
        )
        .expect("the program runs");
        assert_eq!(records, vec![results(&[value]); 16], "ry by {angle}");
    }
}

/// A rotation by an angle that is not finite has no unitary matrix:
/// lowering passes it on, the engine refuses it, and the run fails naming
/// its first shot, the engine's refusal as its error. Each row is an angle:
/// NaN, +∞ and −∞.
#[test]
fn a_rotation_by_an_angle_that_is_not_finite_is_the_engines_refusal() {
    for angle in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let block = [gate(OPID_RX, 0, 0, angle), mz(0), ret()];
        let error = run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &program(1, &[&block]),
            &ids(&[2, 0]),
        )
        .expect_err("the engine refuses the rotation");
        assert!(
            matches!(
                &error,
                QirResumeError::Resumer {
                    shot: QirShotId(2),
                    error: MpsResumerError::Engine(refused),
                } if refused.refusal().is_some()
            ),
            "rx by {angle}: {error:?}"
        );
    }
}

/// A measurement in X or Y reads the qubit in that basis: each of the
/// basis's two states, prepared from |0⟩, reads its own value in every
/// shot, 0 for |+⟩ and |+i⟩, 1 for |−⟩ and |−i⟩.
#[test]
fn a_measurement_in_x_or_y_reads_the_qubit_in_that_basis() {
    let rows: [(&str, &[u64], u64, u8); 4] = [
        ("|+⟩ read by mx", &[OPID_H], OPID_MX, 0),
        ("|−⟩ read by mx", &[OPID_X, OPID_H], OPID_MX, 1),
        ("|+i⟩ read by my", &[OPID_H, OPID_S], OPID_MY, 0),
        ("|−i⟩ read by my", &[OPID_H, OPID_S_ADJ], OPID_MY, 1),
    ];
    for (label, preparation, measurement, value) in rows {
        let mut block: Vec<_> = preparation.iter().map(|&gate| gate1(gate, 0)).collect();
        block.extend([measure(measurement, 0, 0), ret()]);
        let records = run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &program(1, &[&block]),
            &QirShotId::first(16),
        )
        .expect("the program runs");
        assert_eq!(records, vec![results(&[value]); 16], "{label}");
    }
}

/// A measurement in X or Y leaves the state it read: a qubit at |0⟩, read
/// by `mx` (or `my`), then turned from that basis to Z by `h` (or `s_adj`
/// then `h`) and read by `mz`, gives in every shot the value the first
/// read gave, though 16 shots read both values.
#[test]
fn a_measurement_in_x_or_y_leaves_the_state_it_read() {
    let rows: [(&str, u64, &[u64]); 2] = [
        ("mx", OPID_MX, &[OPID_H]),
        ("my", OPID_MY, &[OPID_S_ADJ, OPID_H]),
    ];
    for (label, measurement, to_z) in rows {
        let mut block = vec![measure(measurement, 0, 0)];
        block.extend(to_z.iter().map(|&gate| gate1(gate, 0)));
        block.extend([measure(OPID_MZ, 0, 1), ret()]);
        let records = run(
            &mut MpsResumer::new(ExactReference::new(), 7),
            &program_with(1, 2, &[], &[&block]),
            &QirShotId::first(16),
        )
        .expect("the program runs");
        assert!(
            records.contains(&results(&[0, 0])) && records.contains(&results(&[1, 1])),
            "{label}: 16 shots read both values: {records:?}"
        );
        assert!(
            records.iter().all(|shot| shot[0] == shot[1]),
            "{label}: {records:?}"
        );
    }
}
