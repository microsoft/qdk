// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod dense;

use std::num::NonZeroUsize;

use dense::DenseState;
use qdk_mps::{
    CapStatus, CapabilityStatus, ExecutionPolicy, Measurement, MpsEngine, MpsEngineFactory,
    MpsError, MpsSimulator, OneQubitGate, Operation, OperationOutcome, Pauli, PauliObservable,
    PauliTerm, Precision, QubitId, ResourcePolicy, TruncationPolicy, TwoQubitGate,
};

const TOLERANCE: f64 = 1.0e-10;

pub fn gate_parity<F: MpsEngineFactory>(factory: &F) {
    let one_qubit_gates = [
        OneQubitGate::H,
        OneQubitGate::X,
        OneQubitGate::Y,
        OneQubitGate::Z,
        OneQubitGate::S,
        OneQubitGate::SAdj,
        OneQubitGate::Sx,
        OneQubitGate::SxAdj,
        OneQubitGate::T,
        OneQubitGate::TAdj,
        OneQubitGate::Rx(0.37),
        OneQubitGate::Rx(-0.37),
        OneQubitGate::Ry(0.51),
        OneQubitGate::Ry(-0.51),
        OneQubitGate::Rz(0.29),
        OneQubitGate::Rz(-0.29),
    ];
    let preparations: &[&[OneQubitGate]] = &[
        &[],
        &[OneQubitGate::X],
        &[OneQubitGate::H],
        &[OneQubitGate::H, OneQubitGate::S],
    ];
    for gate in one_qubit_gates {
        for preparation in preparations {
            let mut simulator = factory
                .create_simulator(exact_policy(7))
                .expect("one-qubit simulator");
            let qubit = simulator.allocate().expect("qubit");
            let mut oracle = DenseState::zero(1);
            for preparation_gate in *preparation {
                apply_one(&mut simulator, qubit, *preparation_gate);
                oracle.apply_one(0, *preparation_gate);
            }
            apply_one(&mut simulator, qubit, gate);
            oracle.apply_one(0, gate);
            for pauli in [Pauli::X, Pauli::Y, Pauli::Z] {
                assert_close(
                    expectation(&mut simulator, &[(qubit, pauli)]),
                    oracle.expectation(&[(0, pauli)]),
                    &format!("{gate:?} after {preparation:?}, {pauli:?}"),
                );
            }
        }
    }

    let two_qubit_gates = [
        TwoQubitGate::Cx,
        TwoQubitGate::Cy,
        TwoQubitGate::Cz,
        TwoQubitGate::Swap,
        TwoQubitGate::Rxx(0.43),
        TwoQubitGate::Rxx(-0.43),
        TwoQubitGate::Ryy(0.37),
        TwoQubitGate::Ryy(-0.37),
        TwoQubitGate::Rzz(0.61),
        TwoQubitGate::Rzz(-0.61),
    ];
    let preparations: &[&[(usize, OneQubitGate)]] = &[
        &[],
        &[(0, OneQubitGate::X)],
        &[(1, OneQubitGate::X)],
        &[(0, OneQubitGate::H), (1, OneQubitGate::Ry(0.37))],
    ];
    for gate in two_qubit_gates {
        for reverse in [false, true] {
            for preparation in preparations {
                let mut simulator = factory
                    .create_simulator(exact_policy(11))
                    .expect("two-qubit simulator");
                let qubits = [
                    simulator.allocate().expect("first qubit"),
                    simulator.allocate().expect("second qubit"),
                ];
                let mut oracle = DenseState::zero(2);
                for (target, preparation_gate) in *preparation {
                    apply_one(&mut simulator, qubits[*target], *preparation_gate);
                    oracle.apply_one(*target, *preparation_gate);
                }
                let (first, second) = if reverse { (1, 0) } else { (0, 1) };
                apply_two(&mut simulator, qubits[first], qubits[second], gate);
                oracle.apply_two(first, second, gate);
                assert_tomography(&mut simulator, &oracle, qubits, gate, reverse);
            }
        }
    }
}

pub fn state_updates<F: MpsEngineFactory>(factory: &F) {
    let mut simulator = factory
        .create_simulator(exact_policy(17))
        .expect("simulator");
    let qubits = (0..6)
        .map(|_| simulator.allocate().expect("qubit"))
        .collect::<Vec<_>>();
    let mut oracle = DenseState::zero(qubits.len());
    apply_one(&mut simulator, qubits[0], OneQubitGate::H);
    oracle.apply_one(0, OneQubitGate::H);
    apply_two(&mut simulator, qubits[0], qubits[1], TwoQubitGate::Cx);
    oracle.apply_two(0, 1, TwoQubitGate::Cx);
    assert_close(
        expectation(
            &mut simulator,
            &[(qubits[0], Pauli::Z), (qubits[1], Pauli::Z)],
        ),
        1.0,
        "Bell ZZ",
    );

    for _ in 0..3 {
        for (first, angle) in [0.17, 0.34, 0.51, 0.68, 0.85].into_iter().enumerate() {
            let gate = TwoQubitGate::Rxx(angle);
            apply_two(&mut simulator, qubits[first], qubits[first + 1], gate);
            oracle.apply_two(first, first + 1, gate);
        }
        for (first, angle) in [-0.11, -0.22, -0.33, -0.44, -0.55]
            .into_iter()
            .enumerate()
            .rev()
        {
            let gate = TwoQubitGate::Rzz(angle);
            apply_two(&mut simulator, qubits[first + 1], qubits[first], gate);
            oracle.apply_two(first + 1, first, gate);
        }
    }
    for factors in [
        vec![(0, Pauli::X)],
        vec![(1, Pauli::Y), (4, Pauli::Z)],
        vec![(0, Pauli::X), (2, Pauli::Y), (5, Pauli::Z)],
    ] {
        let mapped = factors
            .iter()
            .map(|(qubit, pauli)| (qubits[*qubit], *pauli))
            .collect::<Vec<_>>();
        assert_close(
            expectation(&mut simulator, &mapped),
            oracle.expectation(&factors),
            "sweep expectation",
        );
    }
    let report = simulator.report().expect("report");
    assert_close(report.state_norm, 1.0, "sweep norm");
    assert!(report.reached_bond_dimension >= 2);
}

pub fn truncation_regression<F: MpsEngineFactory>(factory: &F) {
    let mut policy = exact_policy(19);
    policy.truncation.max_bond_dimension = NonZeroUsize::new(4);
    let mut simulator = factory.create_simulator(policy).expect("simulator");
    let qubits = (0..8)
        .map(|_| simulator.allocate().expect("qubit"))
        .collect::<Vec<_>>();
    for qubit in &qubits[4..] {
        apply_one(&mut simulator, *qubit, OneQubitGate::X);
    }
    for _ in 0..12 {
        for pair in qubits.windows(2) {
            apply_two(&mut simulator, pair[0], pair[1], TwoQubitGate::Cx);
            apply_one(&mut simulator, pair[1], OneQubitGate::Rz(0.3));
            apply_two(&mut simulator, pair[0], pair[1], TwoQubitGate::Cx);
        }
        for qubit in &qubits {
            apply_one(&mut simulator, *qubit, OneQubitGate::Rx(0.3));
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
    assert_close(
        simulator.expectation(&observable).expect("expectation"),
        2.380_955_225_846_558_5,
        "retained bond-4 expectation",
    );
    let report = simulator.report().expect("report");
    assert_close(
        report.state_norm,
        0.991_736_427_415_759_5,
        "retained bond-4 norm",
    );
    assert_eq!(report.reached_bond_dimension, 4);
    assert_eq!(report.cap_status, CapStatus::ReachedCapIndeterminate);
}

pub fn truncation_policy<F: MpsEngineFactory>(factory: &F) {
    let exact = bell_report(factory, exact_policy(37));
    assert_close(exact.state_norm, 1.0, "exact Bell norm");
    assert_eq!(exact.reached_bond_dimension, 2);
    assert_eq!(exact.cap_status, CapStatus::NotConfigured);

    let mut no_criterion = exact_policy(41);
    no_criterion
        .truncation
        .max_relative_discarded_squared_weight_per_split = None;
    let no_criterion = bell_report(factory, no_criterion);
    assert_close(no_criterion.state_norm, 1.0, "no-criterion Bell norm");
    assert_eq!(no_criterion.reached_bond_dimension, 2);

    let mut cap_only = exact_policy(43);
    cap_only
        .truncation
        .max_relative_discarded_squared_weight_per_split = None;
    cap_only.truncation.max_bond_dimension = NonZeroUsize::new(1);
    let cap_only = bell_report(factory, cap_only);
    assert_close(
        cap_only.state_norm,
        std::f64::consts::FRAC_1_SQRT_2,
        "cap-only Bell norm",
    );
    assert_eq!(cap_only.reached_bond_dimension, 1);
    assert_eq!(cap_only.cap_status, CapStatus::ReachedCapIndeterminate);

    let mut combined = exact_policy(47);
    combined.truncation.max_bond_dimension = NonZeroUsize::new(1);
    let combined = bell_report(factory, combined);
    assert_close(
        combined.state_norm,
        std::f64::consts::FRAC_1_SQRT_2,
        "combined Bell norm",
    );
    assert_eq!(combined.reached_bond_dimension, 1);

    let mut threshold_only = exact_policy(53);
    threshold_only
        .truncation
        .max_relative_discarded_squared_weight_per_split = Some(0.02);
    let mut simulator = factory
        .create_simulator(threshold_only)
        .expect("threshold-only simulator");
    let first = simulator.allocate().expect("first qubit");
    let second = simulator.allocate().expect("second qubit");
    let angle = 2.0 * 0.01_f64.sqrt().asin();
    apply_one(&mut simulator, first, OneQubitGate::Ry(angle));
    apply_two(&mut simulator, first, second, TwoQubitGate::Cx);
    let threshold_report = simulator.report().expect("threshold report");
    assert_close(
        threshold_report.state_norm,
        0.99_f64.sqrt(),
        "threshold-only norm",
    );
    assert_eq!(threshold_report.reached_bond_dimension, 1);

    let mut rank_deficient = factory
        .create_simulator(exact_policy(59))
        .expect("rank-deficient simulator");
    let first = rank_deficient.allocate().expect("first qubit");
    let second = rank_deficient.allocate().expect("second qubit");
    apply_two(&mut rank_deficient, first, second, TwoQubitGate::Cx);
    assert_eq!(
        rank_deficient
            .report()
            .expect("rank-deficient report")
            .reached_bond_dimension,
        1
    );
}

pub fn lifecycle<F: MpsEngineFactory>(factory: &F) {
    let mut simulator = factory
        .create_simulator(exact_policy(23))
        .expect("simulator");
    let first = simulator.allocate().expect("first qubit");
    let second = simulator.allocate().expect("second qubit");
    assert_eq!((first, second), (QubitId(0), QubitId(1)));

    apply_one(&mut simulator, first, OneQubitGate::X);
    assert!(!simulator.release(first).expect("release one").was_zero);
    let recycled = simulator.allocate().expect("recycled qubit");
    assert_eq!(recycled, first);
    assert_eq!(measure(&mut simulator, recycled), Measurement::Zero);

    apply_one(&mut simulator, second, OneQubitGate::X);
    simulator.swap_ids(first, second).expect("swap IDs");
    assert_eq!(measure(&mut simulator, first), Measurement::One);
    assert_eq!(measure(&mut simulator, second), Measurement::Zero);

    apply_one(&mut simulator, second, OneQubitGate::X);
    assert_eq!(
        simulator
            .apply(Operation::MeasureResetZ { target: second })
            .expect("measure-reset"),
        OperationOutcome::Measurement(Measurement::One)
    );
    assert_eq!(measure(&mut simulator, second), Measurement::Zero);
    apply_one(&mut simulator, second, OneQubitGate::X);
    assert_eq!(
        simulator
            .apply(Operation::ResetZ { target: second })
            .expect("reset"),
        OperationOutcome::Unit
    );
    assert_eq!(measure(&mut simulator, second), Measurement::Zero);
    assert!(simulator.release(second).expect("release zero").was_zero);
}

pub fn measurement<F: MpsEngineFactory>(factory: &F) {
    fn sequence<F: MpsEngineFactory>(factory: &F) -> Vec<Measurement> {
        let mut simulator = factory
            .create_simulator(exact_policy(42))
            .expect("sequence simulator");
        let qubits = (0..12)
            .map(|_| simulator.allocate().expect("qubit"))
            .collect::<Vec<_>>();
        for qubit in &qubits {
            apply_one(&mut simulator, *qubit, OneQubitGate::H);
        }
        qubits
            .into_iter()
            .map(|qubit| measure(&mut simulator, qubit))
            .collect()
    }

    let first = sequence(factory);
    assert_eq!(first, sequence(factory));
    assert_eq!(
        first,
        vec![
            Measurement::One,
            Measurement::One,
            Measurement::Zero,
            Measurement::One,
            Measurement::Zero,
            Measurement::One,
            Measurement::Zero,
            Measurement::Zero,
            Measurement::One,
            Measurement::Zero,
            Measurement::One,
            Measurement::Zero,
        ]
    );

    let mut ones = 0_u32;
    for shot in 0..10_000 {
        let mut simulator = factory
            .create_simulator(exact_policy(qdk_mps::derive_shot_seed(61, shot)))
            .expect("shot simulator");
        let qubit = simulator.allocate().expect("qubit");
        apply_one(&mut simulator, qubit, OneQubitGate::H);
        ones += u32::from(measure(&mut simulator, qubit) == Measurement::One);
    }
    let frequency = f64::from(ones) / 10_000.0;
    assert!(
        (0.48..=0.52).contains(&frequency),
        "Hadamard frequency {frequency} is outside [0.48, 0.52]"
    );
}

pub fn observables<F: MpsEngineFactory>(factory: &F) {
    let mut simulator = factory
        .create_simulator(exact_policy(67))
        .expect("simulator");
    let qubits = (0..3)
        .map(|_| simulator.allocate().expect("qubit"))
        .collect::<Vec<_>>();
    let mut oracle = DenseState::zero(3);
    for (target, gate) in [
        (0, OneQubitGate::H),
        (1, OneQubitGate::Ry(0.37)),
        (2, OneQubitGate::Sx),
    ] {
        apply_one(&mut simulator, qubits[target], gate);
        oracle.apply_one(target, gate);
    }
    for (first, second, gate) in [(0, 1, TwoQubitGate::Cx), (2, 1, TwoQubitGate::Ryy(-0.29))] {
        apply_two(&mut simulator, qubits[first], qubits[second], gate);
        oracle.apply_two(first, second, gate);
    }

    let terms = [
        (0.75, vec![(0, Pauli::X)]),
        (-0.5, vec![(1, Pauli::Y), (2, Pauli::Z)]),
        (1.25, vec![(0, Pauli::Z), (1, Pauli::X), (2, Pauli::Y)]),
        (0.125, Vec::new()),
    ];
    let observable = PauliObservable {
        terms: terms
            .iter()
            .map(|(coefficient, factors)| PauliTerm {
                coefficient: *coefficient,
                factors: factors
                    .iter()
                    .map(|(qubit, pauli)| (qubits[*qubit], *pauli))
                    .collect(),
            })
            .collect(),
    };
    let expected = terms
        .iter()
        .map(|(coefficient, factors)| coefficient * oracle.expectation(factors))
        .sum();
    assert_close(
        simulator.expectation(&observable).expect("observable"),
        expected,
        "weighted Pauli sum",
    );

    let duplicate = PauliObservable {
        terms: vec![PauliTerm {
            coefficient: 1.0,
            factors: vec![(qubits[0], Pauli::X), (qubits[0], Pauli::Y)],
        }],
    };
    assert_eq!(
        simulator
            .expectation(&duplicate)
            .expect_err("duplicate factor should fail"),
        MpsError::DuplicateQubit(qubits[0])
    );
}

pub fn capabilities<F: MpsEngineFactory>(factory: &F) {
    let capabilities = factory.capabilities();
    assert_eq!(capabilities.complex64, CapabilityStatus::Available);
    assert_eq!(capabilities.maximum_gate_arity, 2);
    assert_eq!(capabilities.dynamic_allocation, CapabilityStatus::Available);
    assert_eq!(capabilities.measurement_reset, CapabilityStatus::Available);
    assert_eq!(capabilities.observables, CapabilityStatus::Available);
    assert!(matches!(
        capabilities.non_local_routing,
        CapabilityStatus::Planned { .. }
    ));
    assert!(matches!(
        capabilities.noise,
        CapabilityStatus::Planned { .. }
    ));
    assert!(matches!(
        capabilities.discarded_weight_diagnostics,
        CapabilityStatus::Planned { .. }
    ));
    assert!(matches!(
        capabilities.constrained_cpu_resources,
        CapabilityStatus::Planned { .. }
    ));

    let mut simulator = factory
        .create_simulator(exact_policy(71))
        .expect("simulator");
    let first = simulator.allocate().expect("first qubit");
    simulator.allocate().expect("middle qubit");
    let third = simulator.allocate().expect("third qubit");
    let error = simulator
        .apply(Operation::Two {
            gate: TwoQubitGate::Cx,
            first,
            second: third,
        })
        .expect_err("non-local gate should fail");
    assert!(matches!(error, MpsError::CapabilityNotImplemented(_)));
    let report = simulator.report().expect("report");
    assert_eq!(report.operation_counts.two_qubit, 0);
    assert_close(report.state_norm, 1.0, "state after rejected capability");
}

pub fn resource_policy_rejection<F: MpsEngineFactory>(factory: &F) {
    let mut policy = exact_policy(29);
    policy.resources.max_cpu_threads = NonZeroUsize::new(2);
    let Err(error) = factory.create_simulator(policy) else {
        panic!("explicit resource limit should reject this engine");
    };
    assert_eq!(error, MpsError::NoEngineSatisfiesPolicy);
}

pub fn report<F: MpsEngineFactory>(factory: &F) {
    let mut policy = exact_policy(31);
    policy.truncation.max_bond_dimension = NonZeroUsize::new(2);
    let mut simulator = factory.create_simulator(policy.clone()).expect("simulator");
    let first = simulator.allocate().expect("first qubit");
    let second = simulator.allocate().expect("second qubit");
    apply_one(&mut simulator, first, OneQubitGate::H);
    apply_two(&mut simulator, first, second, TwoQubitGate::Cx);
    let _ = measure(&mut simulator, first);
    let observable = PauliObservable {
        terms: vec![PauliTerm {
            coefficient: 1.0,
            factors: vec![(first, Pauli::Z)],
        }],
    };
    simulator.expectation(&observable).expect("observable");
    let report = simulator.report().expect("report");

    assert_eq!(report.requested_policy, policy);
    assert_eq!(report.resolved_seed, 31);
    assert!(!report.engine.descriptor.name.is_empty());
    assert!(!report.engine.descriptor.version.is_empty());
    assert_eq!(report.engine.descriptor.device, "cpu");
    assert!(report.engine.resources.max_cpu_threads.get() >= 1);
    assert!(report.engine.resources.caller_limit_honored);
    assert_eq!(report.operation_counts.one_qubit, 1);
    assert_eq!(report.operation_counts.two_qubit, 1);
    assert_eq!(report.operation_counts.measurement, 1);
    assert_eq!(report.operation_counts.observable, 1);
    assert!(report.norm_before_first_non_unitary.is_some());
    assert_close(report.state_norm, 1.0, "reported norm");
    assert_eq!(report.reached_bond_dimension, 2);
    assert_eq!(report.cap_status, CapStatus::ReachedCapIndeterminate);
    assert_eq!(report.local_threshold, Some(0.0));
    assert!(matches!(
        report.discarded_weight,
        CapabilityStatus::Unavailable { .. }
    ));
}

fn bell_report<F: MpsEngineFactory>(
    factory: &F,
    policy: ExecutionPolicy,
) -> qdk_mps::ExecutionReport {
    let mut simulator = factory.create_simulator(policy).expect("Bell simulator");
    let first = simulator.allocate().expect("first qubit");
    let second = simulator.allocate().expect("second qubit");
    apply_one(&mut simulator, first, OneQubitGate::H);
    apply_two(&mut simulator, first, second, TwoQubitGate::Cx);
    simulator.report().expect("Bell report")
}

pub fn exact_policy(seed: u64) -> ExecutionPolicy {
    ExecutionPolicy {
        precision: Precision::Complex64,
        truncation: TruncationPolicy {
            max_relative_discarded_squared_weight_per_split: Some(0.0),
            max_bond_dimension: None,
        },
        shot_seed: seed,
        resources: ResourcePolicy {
            max_cpu_threads: None,
        },
    }
}

fn apply_one<E: MpsEngine>(simulator: &mut MpsSimulator<E>, target: QubitId, gate: OneQubitGate) {
    assert_eq!(
        simulator
            .apply(Operation::One { gate, target })
            .expect("one-qubit gate"),
        OperationOutcome::Unit
    );
}

fn apply_two<E: MpsEngine>(
    simulator: &mut MpsSimulator<E>,
    first: QubitId,
    second: QubitId,
    gate: TwoQubitGate,
) {
    assert_eq!(
        simulator
            .apply(Operation::Two {
                gate,
                first,
                second,
            })
            .expect("two-qubit gate"),
        OperationOutcome::Unit
    );
}

fn measure<E: MpsEngine>(simulator: &mut MpsSimulator<E>, target: QubitId) -> Measurement {
    let OperationOutcome::Measurement(measurement) = simulator
        .apply(Operation::MeasureZ { target })
        .expect("measurement")
    else {
        panic!("expected a measurement outcome");
    };
    measurement
}

fn expectation<E: MpsEngine>(simulator: &mut MpsSimulator<E>, factors: &[(QubitId, Pauli)]) -> f64 {
    simulator
        .expectation(&PauliObservable {
            terms: vec![PauliTerm {
                coefficient: 1.0,
                factors: factors.to_vec(),
            }],
        })
        .expect("expectation")
}

fn assert_tomography<E: MpsEngine>(
    simulator: &mut MpsSimulator<E>,
    oracle: &DenseState,
    qubits: [QubitId; 2],
    gate: TwoQubitGate,
    reverse: bool,
) {
    for first_pauli in [Pauli::I, Pauli::X, Pauli::Y, Pauli::Z] {
        for second_pauli in [Pauli::I, Pauli::X, Pauli::Y, Pauli::Z] {
            if first_pauli == Pauli::I && second_pauli == Pauli::I {
                continue;
            }
            let mut actual_factors = Vec::new();
            let mut oracle_factors = Vec::new();
            if first_pauli != Pauli::I {
                actual_factors.push((qubits[0], first_pauli));
                oracle_factors.push((0, first_pauli));
            }
            if second_pauli != Pauli::I {
                actual_factors.push((qubits[1], second_pauli));
                oracle_factors.push((1, second_pauli));
            }
            assert_close(
                expectation(simulator, &actual_factors),
                oracle.expectation(&oracle_factors),
                &format!("{gate:?}, reverse={reverse}, {first_pauli:?}{second_pauli:?}"),
            );
        }
    }
}

fn assert_close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "{context}: expected {expected:.16}, got {actual:.16}"
    );
}
