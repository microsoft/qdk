// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_complex::Complex64;
use qdk_simulators::execution::{
    CircuitTensorNetwork, QuantumEvolutionRegion, TensorNetworkBuildError, UnitaryOperation,
};
use tensornet::Indices;

fn build(qubits: usize, operations: Vec<UnitaryOperation>) -> CircuitTensorNetwork {
    CircuitTensorNetwork::from_zero_state(qubits, &QuantumEvolutionRegion::new(operations))
        .expect("valid circuit")
}

fn buffer(circuit: &CircuitTensorNetwork, node: usize) -> &[Complex64] {
    &circuit.buffers()[circuit.node_buffer_ids()[node]]
}

fn coefficient(circuit: &CircuitTensorNetwork, node: usize, coords: &[usize]) -> Complex64 {
    let offset = circuit.network().nodes()[node]
        .offset_of(coords)
        .expect("coordinate in tensor");
    buffer(circuit, node)[offset]
}

fn assert_close(actual: Complex64, expected: Complex64) {
    assert!(
        (actual - expected).norm() <= 1e-12,
        "{actual} != {expected}"
    );
}

fn assert_bindings(circuit: &CircuitTensorNetwork) {
    assert_eq!(
        circuit.network().nodes().len(),
        circuit.node_buffer_ids().len()
    );
    for (node, &buffer_id) in circuit
        .network()
        .nodes()
        .iter()
        .zip(circuit.node_buffer_ids())
    {
        let values = &circuit.buffers()[buffer_id];
        assert_eq!(node.element_count(), Some(values.len()));
        assert!(
            values
                .iter()
                .all(|value| value.re.is_finite() && value.im.is_finite())
        );
        assert!(node.as_slice().iter().all(|axis| axis.dim() == 2));
    }
    assert!(
        circuit
            .query()
            .expect("valid query")
            .marginalized()
            .as_slice()
            .is_empty()
    );
}

#[test]
fn zero_boundaries_share_storage_and_retain_every_qubit() {
    let circuit = build(3, vec![]);
    assert_bindings(&circuit);
    assert_eq!(circuit.buffers().len(), 1);
    assert_eq!(circuit.node_buffer_ids(), &[0, 0, 0]);
    for qubit in 0..3 {
        assert_eq!(coefficient(&circuit, qubit, &[0]), Complex64::new(1.0, 0.0));
        assert_eq!(coefficient(&circuit, qubit, &[1]), Complex64::new(0.0, 0.0));
        assert_eq!(
            circuit.network().nodes()[qubit].as_slice(),
            &[circuit.output_axes().as_slice()[qubit]]
        );
    }
    let query = circuit.query().expect("valid query");
    assert_eq!(query.keep(), circuit.output_axes());
    assert_eq!(query.keep().strides(), Some(vec![1, 2, 4]));
    assert!(query.hyperedges().as_slice().is_empty());
}

#[test]
fn zero_qubit_identity_is_scalar_without_a_buffer() {
    let circuit = build(0, vec![]);
    assert_bindings(&circuit);
    assert!(circuit.network().nodes().is_empty());
    assert!(circuit.buffers().is_empty());
    assert_eq!(
        circuit
            .query()
            .expect("scalar query")
            .keep()
            .element_count(),
        Some(1)
    );
}

#[test]
fn output_size_does_not_require_allocating_amplitudes() {
    let circuit = build(usize::BITS as usize, vec![]);
    assert_bindings(&circuit);
    assert_eq!(circuit.output_axes().element_count(), None);
    assert_eq!(circuit.buffers().len(), 1);
}

#[test]
fn identity_preserves_the_wire_but_still_validates_its_operand() {
    let circuit = build(1, vec![UnitaryOperation::I { target: 0 }]);
    assert_bindings(&circuit);
    assert_eq!(circuit.network().nodes().len(), 1);
    let error = CircuitTensorNetwork::from_zero_state(
        1,
        &QuantumEvolutionRegion::new(vec![UnitaryOperation::I { target: 1 }]),
    )
    .expect_err("invalid identity operand");
    assert_eq!(
        error,
        TensorNetworkBuildError::QubitOutOfRange {
            operation_index: 0,
            qubit: 1,
            qubit_count: 1,
        }
    );
}

#[test]
fn rotations_bind_every_coordinate_in_column_major_order() {
    for angle in [0.0, 0.7, -0.7, std::f64::consts::PI] {
        let circuit = build(
            3,
            vec![
                UnitaryOperation::Rx { angle, target: 2 },
                UnitaryOperation::Rzz {
                    angle,
                    q1: 2,
                    q2: 0,
                },
            ],
        );
        assert_bindings(&circuit);
        for a in 0..2 {
            for b in 0..2 {
                let rx = if a == b {
                    Complex64::new((angle / 2.0).cos(), 0.0)
                } else {
                    Complex64::new(0.0, -(angle / 2.0).sin())
                };
                let parity = if a == b { 1.0 } else { -1.0 };
                assert_close(coefficient(&circuit, 3, &[a, b]), rx);
                assert_close(
                    coefficient(&circuit, 4, &[a, b]),
                    Complex64::from_polar(1.0, -angle * parity / 2.0),
                );
                assert_eq!(
                    circuit.network().nodes()[3].offset_of(&[a, b]),
                    Some(a + 2 * b)
                );
            }
        }
    }
}

#[test]
fn sharing_is_by_gate_and_exact_angle_not_wire_or_buffer_length() {
    let angle = 0.7_f64;
    let neighboring_angle = f64::from_bits(angle.to_bits() + 1);
    let circuit = build(
        3,
        vec![
            UnitaryOperation::Rx { angle, target: 0 },
            UnitaryOperation::Rx { angle, target: 2 },
            UnitaryOperation::Rzz {
                angle,
                q1: 0,
                q2: 2,
            },
            UnitaryOperation::Rzz {
                angle,
                q1: 2,
                q2: 0,
            },
            UnitaryOperation::Rx {
                angle: -angle,
                target: 0,
            },
            UnitaryOperation::Rx {
                angle: neighboring_angle,
                target: 2,
            },
        ],
    );
    assert_bindings(&circuit);
    let ids = circuit.node_buffer_ids();
    assert_eq!(ids[3], ids[4]);
    assert_eq!(ids[5], ids[6]);
    for distinct in [ids[5], ids[7], ids[8]] {
        assert_ne!(ids[3], distinct);
    }
    assert_eq!(circuit.buffers().len(), 5);
    assert_ne!(circuit.network().nodes()[3], circuit.network().nodes()[4]);
    assert_close(
        coefficient(&circuit, 3, &[1, 0]),
        Complex64::new(0.0, -(angle / 2.0).sin()),
    );
    assert_close(
        coefficient(&circuit, 7, &[1, 0]),
        Complex64::new(0.0, (angle / 2.0).sin()),
    );
}

#[test]
fn nonadjacent_diagonal_factors_connect_wire_versions_not_sites() {
    let circuit = build(
        3,
        vec![
            UnitaryOperation::Rx {
                angle: 0.7,
                target: 2,
            },
            UnitaryOperation::Rzz {
                angle: 0.4,
                q1: 2,
                q2: 0,
            },
            UnitaryOperation::Rx {
                angle: -0.3,
                target: 0,
            },
            UnitaryOperation::Rzz {
                angle: 0.4,
                q1: 0,
                q2: 2,
            },
            UnitaryOperation::Rx {
                angle: 0.2,
                target: 2,
            },
        ],
    );
    assert_bindings(&circuit);
    let nodes = circuit.network().nodes();
    let axes = |node: usize| nodes[node].as_slice();
    assert_eq!(axes(3)[1], axes(2)[0]);
    assert_ne!(axes(3)[0], axes(3)[1]);
    assert_eq!(axes(4), &[axes(3)[0], axes(0)[0]]);
    assert_eq!(axes(5)[1], axes(0)[0]);
    assert_eq!(axes(6), &[axes(5)[0], axes(3)[0]]);
    assert_eq!(axes(7)[1], axes(3)[0]);
    assert_eq!(
        circuit.output_axes().as_slice(),
        &[axes(5)[0], axes(1)[0], axes(7)[0]]
    );
    assert_eq!(circuit.node_buffer_ids()[4], circuit.node_buffer_ids()[6]);
    let query = circuit.query().expect("valid query");
    for axis in [axes(0)[0], axes(3)[0], axes(5)[0]] {
        assert!(query.hyperedges().contains(axis));
    }
}

#[test]
fn buffers_outlive_the_input_region_and_other_networks() {
    let circuit = {
        let region = QuantumEvolutionRegion::new(vec![UnitaryOperation::Rx {
            angle: 0.7,
            target: 0,
        }]);
        CircuitTensorNetwork::from_zero_state(1, &region).expect("valid circuit")
    };
    let before = circuit.buffers().to_vec();
    drop(build(
        1,
        vec![UnitaryOperation::Rx {
            angle: -1.2,
            target: 0,
        }],
    ));
    assert_eq!(circuit.buffers(), before);
    assert_bindings(&circuit);
    let query = circuit.query().expect("borrowed query");
    assert_eq!(query.network(), circuit.network());
}

#[test]
fn invalid_rotations_and_operands_fail_explicitly() {
    for angle in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for operation in [
            UnitaryOperation::Rx { angle, target: 0 },
            UnitaryOperation::Rzz {
                angle,
                q1: 0,
                q2: 1,
            },
        ] {
            let result = CircuitTensorNetwork::from_zero_state(
                2,
                &QuantumEvolutionRegion::new(vec![operation]),
            );
            assert_eq!(
                result.expect_err("nonfinite angle"),
                TensorNetworkBuildError::NonfiniteAngle { operation_index: 0 }
            );
        }
    }
    for operation in [
        UnitaryOperation::Rx {
            angle: 0.1,
            target: 2,
        },
        UnitaryOperation::Rzz {
            angle: 0.1,
            q1: 2,
            q2: 0,
        },
        UnitaryOperation::Rzz {
            angle: 0.1,
            q1: 0,
            q2: 2,
        },
    ] {
        let result =
            CircuitTensorNetwork::from_zero_state(2, &QuantumEvolutionRegion::new(vec![operation]));
        assert!(matches!(
            result,
            Err(TensorNetworkBuildError::QubitOutOfRange { qubit: 2, .. })
        ));
    }
    let result = CircuitTensorNetwork::from_zero_state(
        2,
        &QuantumEvolutionRegion::new(vec![
            UnitaryOperation::I { target: 0 },
            UnitaryOperation::Rzz {
                angle: 0.1,
                q1: 1,
                q2: 1,
            },
        ]),
    );
    assert_eq!(
        result.expect_err("repeated operand"),
        TensorNetworkBuildError::RepeatedOperand {
            operation_index: 1,
            qubit: 1
        }
    );
}

#[test]
fn every_other_unitary_is_rejected_without_silent_omission() {
    for operation in [
        UnitaryOperation::X { target: 0 },
        UnitaryOperation::Y { target: 0 },
        UnitaryOperation::Z { target: 0 },
        UnitaryOperation::H { target: 0 },
        UnitaryOperation::S { target: 0 },
        UnitaryOperation::SAdj { target: 0 },
        UnitaryOperation::Sx { target: 0 },
        UnitaryOperation::SxAdj { target: 0 },
        UnitaryOperation::T { target: 0 },
        UnitaryOperation::TAdj { target: 0 },
        UnitaryOperation::Ry {
            angle: 0.1,
            target: 0,
        },
        UnitaryOperation::Rz {
            angle: 0.1,
            target: 0,
        },
        UnitaryOperation::Cx {
            control: 0,
            target: 1,
        },
        UnitaryOperation::Cy {
            control: 0,
            target: 1,
        },
        UnitaryOperation::Cz {
            control: 0,
            target: 1,
        },
        UnitaryOperation::Rxx {
            angle: 0.1,
            q1: 0,
            q2: 1,
        },
        UnitaryOperation::Ryy {
            angle: 0.1,
            q1: 0,
            q2: 1,
        },
        UnitaryOperation::Swap { q1: 0, q2: 1 },
    ] {
        let result = CircuitTensorNetwork::from_zero_state(
            2,
            &QuantumEvolutionRegion::new(vec![
                UnitaryOperation::Rx {
                    angle: 0.2,
                    target: 0,
                },
                operation,
            ]),
        );
        assert_eq!(
            result.expect_err("unsupported operation"),
            TensorNetworkBuildError::UnsupportedOperation {
                operation_index: 1,
                operation
            }
        );
    }
}

#[test]
fn wire_count_overflow_is_rejected_before_allocating() {
    let region = QuantumEvolutionRegion::new(vec![UnitaryOperation::Rx {
        angle: 0.1,
        target: 0,
    }]);
    let result = CircuitTensorNetwork::from_zero_state(u32::MAX as usize, &region);
    assert_eq!(
        result.expect_err("too many wire indices"),
        TensorNetworkBuildError::TooManyWireIndices
    );
    let result = CircuitTensorNetwork::from_zero_state(usize::MAX, &region);
    assert_eq!(
        result.expect_err("wire count overflow"),
        TensorNetworkBuildError::TooManyWireIndices
    );
}

#[test]
fn small_amplitude_contraction_uses_bound_buffers_and_ordered_outputs() {
    let angle = 0.7;
    let circuit = build(2, vec![UnitaryOperation::Rx { angle, target: 1 }]);
    let nodes = circuit.network().nodes();
    let output: &Indices = circuit.output_axes();
    let mut amplitudes =
        vec![Complex64::new(0.0, 0.0); output.element_count().expect("small output")];
    for q0 in 0..2 {
        for q1 in 0..2 {
            let offset = output.offset_of(&[q0, q1]).expect("output coordinate");
            for input in 0..2 {
                assert_eq!(nodes[2].as_slice()[1], nodes[1].as_slice()[0]);
                amplitudes[offset] += coefficient(&circuit, 0, &[q0])
                    * coefficient(&circuit, 1, &[input])
                    * coefficient(&circuit, 2, &[q1, input]);
            }
        }
    }
    for (actual, expected) in amplitudes.into_iter().zip([
        Complex64::new((angle / 2.0).cos(), 0.0),
        Complex64::new(0.0, 0.0),
        Complex64::new(0.0, -(angle / 2.0).sin()),
        Complex64::new(0.0, 0.0),
    ]) {
        assert_close(actual, expected);
    }
}
