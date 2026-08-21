// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::num::NonZeroUsize;

use super::*;
use crate::{
    ExecutionPolicy, MpsEngineFactory, OneQubitGate, Operation, PauliObservable, PauliTerm,
    Precision, ResourcePolicy, TruncationPolicy, TwoQubitGate,
};

fn policy(max_bond_dimension: Option<usize>) -> ExecutionPolicy {
    ExecutionPolicy {
        precision: Precision::Complex64,
        truncation: TruncationPolicy {
            max_relative_discarded_squared_weight_per_split: Some(0.0),
            max_bond_dimension: max_bond_dimension.and_then(NonZeroUsize::new),
        },
        shot_seed: 42,
        resources: ResourcePolicy {
            max_cpu_threads: None,
        },
    }
}

#[test]
fn matrix_layout() {
    let first = DynIndex::new_dyn(2);
    let second = DynIndex::new_dyn(2);
    let matrix = [
        [Complex64::new(1.0, 1.0), Complex64::new(2.0, 2.0)],
        [Complex64::new(3.0, 3.0), Complex64::new(4.0, 4.0)],
    ];
    let tensor = matrix_tensor(vec![first, second], &matrix).expect("matrix tensor");
    assert_eq!(
        tensor.to_vec::<Complex64>().expect("dense values"),
        vec![matrix[0][0], matrix[1][0], matrix[0][1], matrix[1][1]]
    );

    let matrix: Matrix4 = [
        [0.0, 1.0, 2.0, 3.0],
        [10.0, 11.0, 12.0, 13.0],
        [20.0, 21.0, 22.0, 23.0],
        [30.0, 31.0, 32.0, 33.0],
    ]
    .map(|row| row.map(|value| Complex64::new(value, 0.0)));
    let indices = (0..4).map(|_| DynIndex::new_dyn(2)).collect();
    let tensor = matrix_tensor(indices, &matrix).expect("two-site matrix tensor");
    let expected = (0..4)
        .flat_map(|column| (0..4).map(move |row| matrix[row][column]))
        .collect::<Vec<_>>();
    assert_eq!(
        tensor.to_vec::<Complex64>().expect("dense values"),
        expected
    );
}

#[test]
fn canonical_metadata() {
    let mut engine = factory().create_engine(&policy(None)).expect("engine");
    let first = engine.append_zero_site().expect("first site");
    let second = engine.append_zero_site().expect("second site");
    engine
        .apply_one(first, &OneQubitGate::H.matrix())
        .expect("H");
    engine
        .apply_adjacent_two(first, second, &TwoQubitGate::Cx.matrix())
        .expect("CX");

    assert_eq!(engine.state.canonical_region().len(), 1);
    assert!(engine.state.canonical_region().contains(&second.0));
    let first_node = engine.state.node_index(&first.0).expect("first node");
    let (edge, _) = engine.state.edges_for_node(first_node)[0];
    assert_eq!(engine.state.ortho_towards_node(edge), Some(&second.0));
}

#[test]
fn retained_regression() {
    let mut simulator = factory()
        .create_simulator(policy(Some(4)))
        .expect("simulator");
    let qubits = (0..8)
        .map(|_| simulator.allocate().expect("qubit"))
        .collect::<Vec<_>>();
    for qubit in &qubits[4..] {
        simulator
            .apply(Operation::One {
                gate: OneQubitGate::X,
                target: *qubit,
            })
            .expect("X");
    }
    for _ in 0..12 {
        for pair in qubits.windows(2) {
            simulator
                .apply(Operation::Two {
                    gate: TwoQubitGate::Cx,
                    first: pair[0],
                    second: pair[1],
                })
                .expect("CX");
            simulator
                .apply(Operation::One {
                    gate: OneQubitGate::Rz(0.3),
                    target: pair[1],
                })
                .expect("Rz");
            simulator
                .apply(Operation::Two {
                    gate: TwoQubitGate::Cx,
                    first: pair[0],
                    second: pair[1],
                })
                .expect("CX");
        }
        for qubit in &qubits {
            simulator
                .apply(Operation::One {
                    gate: OneQubitGate::Rx(0.3),
                    target: *qubit,
                })
                .expect("Rx");
        }
    }

    let observable = PauliObservable {
        terms: qubits
            .windows(2)
            .map(|pair| PauliTerm {
                coefficient: 1.0,
                factors: vec![(pair[0], Pauli::Z), (pair[1], Pauli::Z)],
            })
            .collect(),
    };
    let expectation = simulator.expectation(&observable).expect("expectation");
    let report = simulator.report().expect("report");
    assert!((expectation - 2.380_955_225_846_558_5).abs() < 1.0e-10);
    assert!((report.state_norm - 0.991_736_427_415_759_5).abs() < 1.0e-10);
    assert_eq!(report.reached_bond_dimension, 4);
}

#[test]
fn invalid_contraction_reports_engine_failure() {
    let first = IdxTensor::from_dense(
        vec![DynIndex::new_dyn(2)],
        vec![Complex64::ONE, Complex64::ZERO],
    )
    .expect("first tensor");
    let second = IdxTensor::from_dense(
        vec![DynIndex::new_dyn(2)],
        vec![Complex64::ONE, Complex64::ZERO],
    )
    .expect("second tensor");

    let error = contract(&[&first, &second])
        .map_err(engine_error)
        .expect_err("disconnected contraction should fail");
    let MpsError::EngineFailure(message) = error else {
        panic!("expected a portable engine failure");
    };
    assert!(message.contains("Disconnected tensor network"));
}

#[test]
fn context_reporting() {
    let engine = factory().create_engine(&policy(None)).expect("engine");
    let resources = engine.info().resources;
    assert!(resources.max_cpu_threads.get() >= 1);
    assert!(resources.caller_limit_honored);
}
