// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use num_complex::Complex64;
use qdk_simulators::{
    MeasurementResult, Simulator,
    cpu_full_state_simulator::FullStateSimulator,
    execution::{
        CircuitTensorNetwork, FixedOutcomeCircuit, FixedOutcomeOperation, QuantumEvolutionRegion,
        TensorNetworkBuildError, UnitaryOperation,
    },
};
use tensornet::{Index, Indices};

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
    assert_eq!(circuit.output_qubits(), &[0, 1, 2]);
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
    assert!(circuit.output_qubits().is_empty());
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

fn gate(operation: UnitaryOperation) -> FixedOutcomeOperation {
    FixedOutcomeOperation::Unitary(operation)
}

fn measure(qubit: usize, result_id: usize, one: bool, reset: bool) -> FixedOutcomeOperation {
    FixedOutcomeOperation::Measure {
        qubit,
        result_id,
        outcome: if one {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        },
        reset,
    }
}

fn fixed(qubits: usize, operations: Vec<FixedOutcomeOperation>) -> FixedOutcomeCircuit {
    FixedOutcomeCircuit::new(qubits, operations).expect("valid fixed-outcome circuit")
}

fn build_fixed(circuit: &FixedOutcomeCircuit) -> CircuitTensorNetwork {
    let network = CircuitTensorNetwork::from_fixed_outcome_circuit(circuit).expect("valid network");
    assert_bindings(&network);
    network
}

/// Dense contraction by brute force: sums the product of every node's bound
/// coefficient over all values of all axes, into the column-major output.
fn contract(circuit: &CircuitTensorNetwork) -> Vec<Complex64> {
    let nodes = circuit.network().nodes();
    let axes = nodes
        .iter()
        .flat_map(Indices::as_slice)
        .chain(circuit.output_axes().as_slice())
        .copied()
        .collect::<BTreeSet<Index>>()
        .into_iter()
        .collect::<Vec<_>>();
    assert!(axes.len() <= 20, "brute force is for small networks");
    let output = circuit.output_axes();
    let mut amplitudes =
        vec![Complex64::new(0.0, 0.0); output.element_count().expect("small output")];
    let value = |assignment: usize, axis: &Index| {
        let position = axes.binary_search(axis).expect("known axis");
        (assignment >> position) & 1
    };
    for assignment in 0..1_usize << axes.len() {
        let coords = |node: &Indices| {
            node.as_slice()
                .iter()
                .map(|axis| value(assignment, axis))
                .collect::<Vec<_>>()
        };
        let term = nodes
            .iter()
            .enumerate()
            .map(|(node, axes)| coefficient(circuit, node, &coords(axes)))
            .product::<Complex64>();
        amplitudes[output
            .offset_of(&coords(output))
            .expect("output coordinate")] += term;
    }
    amplitudes
}

/// The scalar amplitude of a closed network.
fn amplitude(circuit: &CircuitTensorNetwork) -> Complex64 {
    assert!(
        circuit.output_axes().as_slice().is_empty(),
        "closed network"
    );
    contract(circuit)[0]
}

/// H up to a global phase: S·SX·S = e^{iπ/4} H.
fn hadamard_like(target: usize) -> [FixedOutcomeOperation; 3] {
    [
        gate(UnitaryOperation::S { target }),
        gate(UnitaryOperation::Sx { target }),
        gate(UnitaryOperation::S { target }),
    ]
}

/// GHZ on `qubits` from the builder's gates, measuring qubit `q` into result `q`
/// with outcome bit `q` of `record`. `CX(0, t) = H_t Cz(0, t) H_t`, so the state
/// is e^{iπ/4 (2 qubits - 1)} (|0…0⟩ + |1…1⟩)/√2.
fn ghz(qubits: usize, record: usize) -> FixedOutcomeCircuit {
    let mut operations = hadamard_like(0).to_vec();
    for target in 1..qubits {
        operations.extend(hadamard_like(target));
        operations.push(gate(UnitaryOperation::Cz { control: 0, target }));
        operations.extend(hadamard_like(target));
    }
    for qubit in 0..qubits {
        operations.push(measure(qubit, qubit, (record >> qubit) & 1 == 1, true));
    }
    fixed(qubits, operations)
}

#[test]
fn bell_and_ghz_amplitudes_cover_every_record() {
    for qubits in [2_u32, 3] {
        let phase = Complex64::from_polar(
            std::f64::consts::FRAC_1_SQRT_2,
            std::f64::consts::FRAC_PI_4 * f64::from(2 * qubits - 1),
        );
        let all_ones = (1_usize << qubits) - 1;
        for record in 0..=all_ones {
            let network = build_fixed(&ghz(qubits as usize, record));
            let expected = if record == 0 || record == all_ones {
                phase
            } else {
                Complex64::new(0.0, 0.0)
            };
            assert_close(amplitude(&network), expected);
        }
    }
}

#[test]
fn a_flipped_deterministic_cap_gives_exactly_zero() {
    // Sx·Sx = X, so the outcome is 1 with certainty.
    let circuit = fixed(
        1,
        vec![
            gate(UnitaryOperation::Sx { target: 0 }),
            gate(UnitaryOperation::Sx { target: 0 }),
            measure(0, 0, true, true),
        ],
    );
    assert_close(amplitude(&build_fixed(&circuit)), Complex64::new(1.0, 0.0));
    let flipped = circuit.with_outcome(0, false).expect("known result");
    assert_eq!(amplitude(&build_fixed(&flipped)), Complex64::new(0.0, 0.0));

    let bell = ghz(2, 0b11);
    let flipped = bell.with_outcome(1, false).expect("known result");
    assert_eq!(amplitude(&build_fixed(&flipped)), Complex64::new(0.0, 0.0));
}

#[test]
fn s_and_sx_bind_their_matrices_and_phases() {
    let half = |re, im| Complex64::new(re, im) / 2.0;
    for (one, expected) in [(false, half(1.0, 1.0)), (true, half(1.0, -1.0))] {
        let circuit = fixed(
            1,
            vec![
                gate(UnitaryOperation::Sx { target: 0 }),
                measure(0, 0, one, false),
            ],
        );
        assert_close(amplitude(&build_fixed(&circuit)), expected);
    }
    // S Sx |0⟩: the |1⟩ amplitude gains the phase i.
    let circuit = fixed(
        1,
        vec![
            gate(UnitaryOperation::Sx { target: 0 }),
            gate(UnitaryOperation::S { target: 0 }),
            measure(0, 0, true, false),
        ],
    );
    assert_close(
        amplitude(&build_fixed(&circuit)),
        half(1.0, -1.0) * Complex64::new(0.0, 1.0),
    );
    let network = build_fixed(&circuit);
    let sx = 1;
    let s = 2;
    for a in 0..2 {
        for b in 0..2 {
            let expected = if a == b {
                half(1.0, 1.0)
            } else {
                half(1.0, -1.0)
            };
            assert_eq!(coefficient(&network, sx, &[a, b]), expected);
        }
    }
    assert_eq!(coefficient(&network, s, &[0]), Complex64::new(1.0, 0.0));
    assert_eq!(coefficient(&network, s, &[1]), Complex64::new(0.0, 1.0));
    let nodes = network.network().nodes();
    assert_eq!(nodes[s].as_slice(), &[nodes[sx].as_slice()[0]]);
}

#[test]
fn open_outputs_match_the_cpu_full_state_simulator() {
    let operations = [
        UnitaryOperation::Sx { target: 0 },
        UnitaryOperation::Rx {
            angle: 0.7,
            target: 1,
        },
        UnitaryOperation::S { target: 1 },
        UnitaryOperation::Cz {
            control: 0,
            target: 2,
        },
        UnitaryOperation::Sx { target: 2 },
        UnitaryOperation::Rzz {
            angle: -0.4,
            q1: 2,
            q2: 1,
        },
        UnitaryOperation::Cz {
            control: 1,
            target: 0,
        },
        UnitaryOperation::S { target: 0 },
        UnitaryOperation::Sx { target: 1 },
    ];
    let network = build_fixed(&fixed(3, operations.iter().copied().map(gate).collect()));
    assert_eq!(network.output_axes().as_slice().len(), 3);
    let amplitudes = contract(&network);

    let mut simulator = FullStateSimulator::new(3, 0, 0, Default::default());
    for operation in operations {
        match operation {
            UnitaryOperation::S { target } => simulator.s(target),
            UnitaryOperation::Sx { target } => simulator.sx(target),
            UnitaryOperation::Rx { angle, target } => simulator.rx(angle, target),
            UnitaryOperation::Cz { control, target } => simulator.cz(control, target),
            UnitaryOperation::Rzz { angle, q1, q2 } => simulator.rzz(angle, q1, q2),
            _ => unreachable!("gates of this test"),
        }
    }
    let state = simulator.state_dump().data();
    // Both index basis states as sum(b[q] * 2^q): the network's output is
    // column-major over q0, q1, q2, and the simulator's q0 is the lowest bit.
    assert_eq!(state.len(), amplitudes.len());
    for (&actual, expected) in amplitudes.iter().zip(state.iter()) {
        assert_close(actual, *expected);
    }
}

#[test]
fn measured_qubits_are_reused_from_their_outcome_or_zero_after_reset() {
    // After Measure(b) the wire restarts from |b⟩, or from |0⟩ when reset, so
    // a second Sx gives ⟨c|SX|b⟩ or ⟨c|SX|0⟩.
    let half = |re, im| Complex64::new(re, im) / 2.0;
    let sx = |b: bool, c: bool| {
        if b == c {
            half(1.0, 1.0)
        } else {
            half(1.0, -1.0)
        }
    };
    for first in [false, true] {
        for second in [false, true] {
            for reset in [false, true] {
                let circuit = fixed(
                    1,
                    vec![
                        gate(UnitaryOperation::Sx { target: 0 }),
                        measure(0, 0, first, reset),
                        gate(UnitaryOperation::Sx { target: 0 }),
                        measure(0, 1, second, true),
                    ],
                );
                let start = first && !reset;
                let network = build_fixed(&circuit);
                assert_close(amplitude(&network), sx(false, first) * sx(start, second));
                let nodes = network.network().nodes();
                assert_eq!(nodes.len(), 6, "start, Sx, cap, start, Sx, cap");
                assert_ne!(nodes[0], nodes[3], "the second wire is new");
                assert_eq!(
                    network.node_buffer_ids()[3],
                    network.node_buffer_ids()[if start { 2 } else { 0 }],
                    "starts share the basis buffers with caps"
                );
            }
        }
    }
    // A measurement right after another sees the basis state it left.
    for (first, second) in [(false, false), (true, true), (true, false), (false, true)] {
        let circuit = fixed(
            1,
            vec![
                gate(UnitaryOperation::Sx { target: 0 }),
                gate(UnitaryOperation::Sx { target: 0 }),
                measure(0, 0, first, false),
                measure(0, 1, second, false),
            ],
        );
        let expected = if first && second {
            Complex64::new(1.0, 0.0)
        } else {
            Complex64::new(0.0, 0.0)
        };
        assert_close(amplitude(&build_fixed(&circuit)), expected);
    }
}

#[test]
fn idle_wires_have_no_nodes_and_unmeasured_wires_stay_open() {
    let circuit = fixed(
        4,
        vec![
            gate(UnitaryOperation::I { target: 0 }),
            gate(UnitaryOperation::Sx { target: 1 }),
            measure(1, 0, true, true),
            measure(3, 1, false, false),
            gate(UnitaryOperation::Sx { target: 2 }),
        ],
    );
    let network = build_fixed(&circuit);
    let nodes = network.network().nodes();
    // q0 (identity only) and q1 after its reset are idle; q3 is measured with
    // no gate, so its start and cap remain.
    assert_eq!(nodes.len(), 7);
    assert_eq!(network.output_axes().as_slice(), &[nodes[6].as_slice()[0]]);
    // An identity-only qubit has no node and so no output axis.
    assert_eq!(network.output_qubits(), &[2]);
    let amplitudes = contract(&network);
    let sx1 = Complex64::new(0.5, -0.5);
    assert_close(amplitudes[0], sx1 * Complex64::new(0.5, 0.5));
    assert_close(amplitudes[1], sx1 * sx1);

    let empty = build_fixed(&fixed(usize::MAX, vec![]));
    assert!(empty.network().nodes().is_empty());
    assert_eq!(contract(&empty), [Complex64::new(1.0, 0.0)]);
}

#[test]
fn output_qubits_name_each_open_axis_and_a_trailing_identity_stays_closed() {
    // q3 is added first and ends in |1⟩ (Sx·Sx = X); q1 ends in Sx|0⟩.
    let circuit = fixed(
        4,
        vec![
            gate(UnitaryOperation::Sx { target: 3 }),
            gate(UnitaryOperation::Sx { target: 3 }),
            gate(UnitaryOperation::Sx { target: 1 }),
            measure(0, 0, false, false),
            gate(UnitaryOperation::I { target: 0 }),
        ],
    );
    let network = build_fixed(&circuit);
    assert_eq!(network.output_qubits(), &[1, 3]);
    // Column-major output: the first axis (q1) varies fastest.
    let sx = [Complex64::new(0.5, 0.5), Complex64::new(0.5, -0.5)];
    let expected = [Complex64::default(), Complex64::default(), sx[0], sx[1]];
    for (actual, expected) in contract(&network).into_iter().zip(expected) {
        assert_close(actual, expected);
    }

    let closed = build_fixed(&fixed(
        1,
        vec![
            measure(0, 0, true, false),
            gate(UnitaryOperation::I { target: 0 }),
        ],
    ));
    assert!(closed.output_qubits().is_empty());
    assert!(closed.output_axes().as_slice().is_empty());
}

#[test]
fn fixed_outcome_gates_share_buffers_by_kind() {
    let circuit = fixed(
        2,
        vec![
            gate(UnitaryOperation::Sx { target: 0 }),
            gate(UnitaryOperation::Sx { target: 1 }),
            gate(UnitaryOperation::S { target: 0 }),
            gate(UnitaryOperation::S { target: 1 }),
            gate(UnitaryOperation::Cz {
                control: 0,
                target: 1,
            }),
            gate(UnitaryOperation::Cz {
                control: 1,
                target: 0,
            }),
            measure(0, 0, false, true),
            measure(1, 1, true, true),
        ],
    );
    let network = build_fixed(&circuit);
    let ids = network.node_buffer_ids();
    // Nodes: |0⟩ q0, Sx, |0⟩ q1, Sx, S, S, Cz, Cz, ⟨0|, ⟨1|.
    assert_eq!(ids.len(), 10);
    assert_eq!(ids[0], ids[2]);
    assert_eq!(ids[0], ids[8], "a ⟨0| cap shares the |0⟩ start buffer");
    assert_eq!(ids[1], ids[3]);
    assert_eq!(ids[4], ids[5]);
    assert_eq!(ids[6], ids[7]);
    assert_eq!(network.buffers().len(), 5);
    for (a, b, expected) in [(0, 0, 1.0), (0, 1, 1.0), (1, 0, 1.0), (1, 1, -1.0)] {
        assert_eq!(
            coefficient(&network, 6, &[a, b]),
            Complex64::new(expected, 0.0)
        );
    }
    assert_eq!(coefficient(&network, 9, &[1]), Complex64::new(1.0, 0.0));
    assert_eq!(coefficient(&network, 9, &[0]), Complex64::new(0.0, 0.0));
}

#[test]
fn fixed_outcome_builder_rejects_unsupported_and_invalid_gates() {
    for operation in [
        UnitaryOperation::X { target: 0 },
        UnitaryOperation::H { target: 0 },
        UnitaryOperation::SAdj { target: 0 },
        UnitaryOperation::SxAdj { target: 0 },
        UnitaryOperation::T { target: 0 },
        UnitaryOperation::Cx {
            control: 0,
            target: 1,
        },
    ] {
        let circuit = fixed(2, vec![measure(0, 0, false, true), gate(operation)]);
        assert_eq!(
            CircuitTensorNetwork::from_fixed_outcome_circuit(&circuit)
                .expect_err("unsupported operation"),
            TensorNetworkBuildError::UnsupportedOperation {
                operation_index: 1,
                operation
            }
        );
    }
    let circuit = fixed(
        2,
        vec![gate(UnitaryOperation::Cz {
            control: 1,
            target: 1,
        })],
    );
    assert_eq!(
        CircuitTensorNetwork::from_fixed_outcome_circuit(&circuit).expect_err("repeated operand"),
        TensorNetworkBuildError::RepeatedOperand {
            operation_index: 0,
            qubit: 1
        }
    );
    let circuit = fixed(1, vec![gate(UnitaryOperation::S { target: 1 })]);
    assert_eq!(
        CircuitTensorNetwork::from_fixed_outcome_circuit(&circuit).expect_err("qubit range"),
        TensorNetworkBuildError::QubitOutOfRange {
            operation_index: 0,
            qubit: 1,
            qubit_count: 1
        }
    );
    let circuit = fixed(
        1,
        vec![gate(UnitaryOperation::Rx {
            angle: f64::NAN,
            target: 0,
        })],
    );
    assert_eq!(
        CircuitTensorNetwork::from_fixed_outcome_circuit(&circuit).expect_err("nonfinite angle"),
        TensorNetworkBuildError::NonfiniteAngle { operation_index: 0 }
    );
}
