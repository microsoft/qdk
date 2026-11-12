// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only: the Pauli faults injected after each
//! two-site gate.

use crate::mps_simulator::PauliFaults;
use crate::mps_simulator::resumer::lowering::{Operator, Step};
use crate::qir::{QirGate, QirShotId};

/// A batch of a two-site gate, a one-site gate and a measurement.
fn batch() -> Vec<Step> {
    vec![
        Step::Apply(Operator {
            site_slots: [1, 3],
            matrix: QirGate::Cx.matrix_at(None),
        }),
        Step::Apply(Operator {
            site_slots: [2, 0],
            matrix: QirGate::H.matrix_at(None),
        }),
        Step::Measure {
            site: 1,
            result: 0,
            reset: false,
        },
    ]
}

/// The sites of the steps' one-site Paulis, in order.
fn pauli_sites(steps: &[Step]) -> Vec<usize> {
    let paulis = [QirGate::X, QirGate::Y, QirGate::Z].map(|gate| gate.matrix_at(None));
    steps
        .iter()
        .filter_map(|step| match step {
            Step::Apply(operator) if paulis.contains(&operator.matrix) => {
                Some(operator.site_slots[0])
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_certain_fault_follows_a_two_site_gate_on_each_of_its_sites() {
    let faults = PauliFaults {
        probability: 1.0,
        seed: 3,
    };
    let steps = faults.inject(batch(), &mut faults.stream(QirShotId(0)));
    assert_eq!(steps.len(), 5, "two faults, after the CX only");
    assert_eq!(pauli_sites(&steps[1..3]), [1, 3], "on the CX's sites");
    assert_eq!(steps[3..], batch()[1..], "the H and the draw unfaulted");
}

#[test]
fn no_fault_at_probability_zero() {
    let faults = PauliFaults {
        probability: 0.0,
        seed: 3,
    };
    let steps = faults.inject(batch(), &mut faults.stream(QirShotId(0)));
    assert_eq!(steps, batch());
}
