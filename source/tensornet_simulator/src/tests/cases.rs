// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Cases that hold for any engine, run on `ExactReference` by the other
//! modules and on cuTensorNet by `qualification`.

use executor_protocol::{Close as _, PerShot, ShotSet, SubmissionWrapper, open};
use num_complex::Complex64;
use qdk_simulators::{
    MeasurementResult, OutputRecord,
    execution::{PauliSum, PreparedAdaptiveProgram, RuntimeAdapter},
};
use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use tensornet::MpsEngine;

use super::{
    Bytecode, OPID_CZ, OPID_Y, bell, iterative_phase_estimation, pauli,
    phase_estimation_distribution,
};
use crate::{Evaluate, MpsStack, StateQuery, StateQueryValue, TensorNetShots};

/// A record's value, for a record of a measurement that was not lost.
pub(in crate::tests) fn bit(record: &OutputRecord) -> bool {
    match record {
        OutputRecord::Result(MeasurementResult::One) => true,
        OutputRecord::Result(MeasurementResult::Zero) => false,
        other => panic!("expected a measurement result, got {other:?}"),
    }
}

/// How many items `items` yields, as a `u32`.
pub(in crate::tests) fn count<T>(items: impl Iterator<Item = T>) -> u32 {
    u32::try_from(items.count()).expect("fewer than 2^32")
}

/// Each shot's records as bits.
pub(in crate::tests) fn bits(shots: &[Vec<OutputRecord>]) -> Vec<Vec<bool>> {
    shots
        .iter()
        .map(|records| records.iter().map(bit).collect())
        .collect()
}

/// The expectation of each query, which must all be expectations.
pub(in crate::tests) fn expectations<X: Evaluate>(
    evaluator: &X,
    program: &PreparedAdaptiveProgram<u64>,
    observables: &[PauliSum],
) -> Vec<Complex64> {
    let queries: Vec<_> = observables
        .iter()
        .cloned()
        .map(StateQuery::Expectation)
        .collect();
    evaluator
        .evaluate(program, &queries)
        .expect("the queries are answered")
        .into_iter()
        .map(|value| match value {
            StateQueryValue::Expectation(e) => e,
            other @ StateQueryValue::Cost(_) => panic!("expected an expectation, got {other:?}"),
        })
        .collect()
}

/// ⟨labels⟩ on `qubits`, real part, after checking the imaginary part is
/// round-off.
pub(in crate::tests) fn expectation<X: Evaluate>(
    evaluator: &X,
    program: &PreparedAdaptiveProgram<u64>,
    labels: &str,
    qubits: &[usize],
) -> f64 {
    let [value] = expectations(evaluator, program, &[pauli(1.0, labels, qubits)])[..] else {
        unreachable!("one query, one value");
    };
    assert!(value.im.abs() < 1e-12, "⟨{labels}⟩ = {value}");
    value.re
}

/// Asserts `actual` is `expected` to `tolerance`, element by element.
pub(in crate::tests) fn close(actual: &[Complex64], expected: &[f64], tolerance: f64) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{actual:?} against {expected:?}"
    );
    for (k, (a, &e)) in actual.iter().zip(expected).enumerate() {
        assert!((a - e).norm() <= tolerance, "value {k}: {a} against {e}");
    }
}

/// Bell pairs sampled by the stack: the two records of a shot agree, and
/// both values occur with frequency about one half.
pub(in crate::tests) fn bell_samples_are_correlated<E: MpsEngine>(stack: &MpsStack<E>, shots: u32) {
    let rows = bits(&stack.sample(&bell(), shots, Some(7)).expect("samples"));
    assert_eq!(rows.len(), shots as usize);
    assert!(
        rows.iter().all(|row| row.len() == 2 && row[0] == row[1]),
        "{rows:?}"
    );
    let ones = f64::from(count(rows.iter().filter(|row| row[0]))) / f64::from(shots);
    assert!(
        (ones - 0.5).abs() < 5.0 / f64::from(shots).sqrt(),
        "P(11) = {ones}"
    );
}

/// MPS14 and MPS19: Bell, GHZ, a phase, a sum with an identity, a
/// complex coefficient; each against its value by hand.
pub(in crate::tests) fn expectations_are_the_hand_computed_ones<E: MpsEngine>(
    stack: &MpsStack<E>,
    tolerance: f64,
) {
    let bell = Bytecode::new(2).h(0).cx(0, 1).build();
    let ghz = Bytecode::new(3).h(0).cx(0, 1).cx(1, 2).build();
    let theta: f64 = 0.7;
    let phase = Bytecode::new(1).h(0).rz(theta, 0).build();
    let strings = |labels: &[&str]| -> Vec<PauliSum> {
        labels
            .iter()
            .map(|labels| {
                let qubits: Vec<usize> = (0..labels.len()).collect();
                pauli(1.0, labels, &qubits)
            })
            .collect()
    };
    for (program, observables, want) in [
        (
            &bell,
            strings(&["ZZ", "XX", "YY", "ZI", "IX", "XZ"]),
            vec![1.0, 1.0, -1.0, 0.0, 0.0, 0.0],
        ),
        (
            &ghz,
            strings(&["XXX", "ZZI", "IZZ", "ZII", "XYY", "YXY", "XXY"]),
            vec![1.0, 1.0, 1.0, 0.0, -1.0, -1.0, 0.0],
        ),
        (
            &phase,
            strings(&["X", "Y", "Z"]),
            vec![theta.cos(), theta.sin(), 0.0],
        ),
    ] {
        close(
            &expectations(stack, program, &observables),
            &want,
            tolerance,
        );
    }
    // 0.25·I + 2·ZZ − XX + 3·Z₀ on the Bell pair is 0.25 + 2 − 1 + 0, with
    // the cost beside it.
    let mut sum = pauli(2.0, "ZZ", &[0, 1]);
    for (coefficient, labels) in [(-1.0, "XX"), (3.0, "ZI")] {
        sum.push_labels(Complex64::new(coefficient, 0.0), labels, &[0, 1])
            .expect("a well-formed term");
    }
    sum.push(Complex64::new(0.25, 0.0), [])
        .expect("the identity");
    match stack
        .evaluate(&bell, &[StateQuery::Expectation(sum), StateQuery::Cost])
        .expect("answered")[..]
    {
        [StateQueryValue::Expectation(e), StateQueryValue::Cost(cost)] => {
            assert!((e - 1.25).norm() <= tolerance, "{e}");
            assert_eq!(cost.max_bond_dimension, 2);
        }
        ref other => panic!("{other:?}"),
    }
    // Qubits in any order, and a subset.
    close(
        &expectations(stack, &ghz, &[pauli(1.0, "Z", &[2])]),
        &[0.0],
        tolerance,
    );
    close(
        &expectations(stack, &ghz, &[pauli(1.0, "YXY", &[2, 1, 0])]),
        &[-1.0],
        tolerance,
    );
    // MPS19: |+i⟩|1⟩ = (S·H ⊗ X)|00⟩ has ⟨X₀⟩ = 0 and ⟨Y₀⟩ = 1, so
    // 0.5·I + 0.5·X₀Z₁ + 0.5i·Y₀Z₁ is 0.5 − 0.5i.
    let plus_i = Bytecode::new(2).h(0).s(0).x(1).build();
    let mut sigma = pauli(0.5, "XZ", &[0, 1]);
    sigma
        .push_labels(Complex64::new(0.0, 0.5), "YZ", &[0, 1])
        .expect("a well-formed term");
    sigma
        .push(Complex64::new(0.5, 0.0), [])
        .expect("the identity");
    let [value] = expectations(stack, &plus_i, &[sigma])[..] else {
        unreachable!("one query");
    };
    assert!(
        (value - Complex64::new(0.5, -0.5)).norm() <= tolerance,
        "{value}"
    );
}

/// B1: X on a pattern of qubits of a `width`-qubit register, every qubit
/// measured: each shot reads the pattern, and the state is a product.
pub(in crate::tests) fn x_pattern_reads_back<E: MpsEngine>(stack: &MpsStack<E>, width: u32) {
    let ones: Vec<u64> = match width {
        2 => vec![0],
        3 => vec![0, 2],
        _ => vec![0, u64::from(width / 3), u64::from(width - 1)],
    };
    let program = ones
        .iter()
        .fold(Bytecode::new(width), |program, &q| program.x(q))
        .measure_all(&(0..u64::from(width)).collect::<Vec<_>>())
        .build();
    let expected: Vec<bool> = (0..u64::from(width)).map(|q| ones.contains(&q)).collect();
    let rows = bits(&stack.sample(&program, 4, Some(1)).expect("samples"));
    assert!(
        rows.iter().all(|row| *row == expected),
        "width {width}: {rows:?}"
    );
    match stack
        .evaluate(&program, &[StateQuery::Cost])
        .expect("answered")[..]
    {
        [StateQueryValue::Cost(cost)] => assert_eq!(cost.max_bond_dimension, 1, "width {width}"),
        ref other => panic!("{other:?}"),
    }
}

/// M0's `Circuit::trotter_domain_wall`: X on the upper half, then per
/// step a ZZ rotation on each neighbouring pair (CX, Rz, CX) and an Rx on
/// every qubit.
pub(in crate::tests) fn trotter_domain_wall(
    width: u32,
    steps: u32,
    angle: f64,
) -> PreparedAdaptiveProgram<u64> {
    let width = u64::from(width);
    let mut program = (width / 2..width).fold(
        Bytecode::new(u32::try_from(width).expect("small")),
        Bytecode::x,
    );
    for _ in 0..steps {
        for c in 0..width - 1 {
            program = program.cx(c, c + 1).rz(angle, c + 1).cx(c, c + 1);
        }
        for q in 0..width {
            program = program.rx(angle, q);
        }
    }
    program.build()
}

/// M0's `AdjacentZQuery`: Σᵢ ZᵢZᵢ₊₁.
pub(in crate::tests) fn adjacent_zz(width: u32) -> PauliSum {
    let mut sum = PauliSum::new();
    for i in 0..width as usize - 1 {
        sum.push_labels(Complex64::new(1.0, 0.0), "ZZ", &[i, i + 1])
            .expect("a well-formed term");
    }
    sum
}

/// B2–B4: ⟨Σ ZᵢZᵢ₊₁⟩ after the Trotter circuit, against M0's pinned value,
/// to a relative error of `relative`. The imaginary part must be
/// round-off, as M0 checked.
pub(in crate::tests) fn trotter_matches_m0<E: MpsEngine>(
    stack: &MpsStack<E>,
    width: u32,
    steps: u32,
    expected: f64,
    relative: f64,
) {
    let program = trotter_domain_wall(width, steps, 0.3);
    let [value] = expectations(stack, &program, &[adjacent_zz(width)])[..] else {
        unreachable!("one query");
    };
    let error = ((value.re - expected) / expected).abs();
    println!(
        "trotter width={width} steps={steps} chi={} value={:.15} expected={expected:.15} relative_error={error:.3e}",
        stack.max_bond(),
        value.re
    );
    assert!(value.im.abs() <= 1e-12, "width {width}: {value}");
    assert!(
        error <= relative,
        "width {width}: {} against {expected}, relative error {error:.3e}",
        value.re
    );
}

/// A circuit of 30 gates drawn from `seed` on `n` qubits (H, S, Rz, Y, CZ,
/// CX), every qubit measured, as the prototype's `scrambled`.
pub(in crate::tests) fn scrambled(n: u32, seed: u64) -> Bytecode {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut program = Bytecode::new(n);
    let n = u64::from(n);
    for _ in 0..30 {
        let q = rng.random_range(0..n);
        let t = (q + rng.random_range(1..n)) % n;
        program = match rng.random_range(0..6) {
            0 => program.h(q),
            1 => program.s(q),
            2 => program.rz(rng.random_range(-3.0..3.0), q),
            3 => program.gate(OPID_Y, q, 0, 0.0),
            4 => program.gate(OPID_CZ, q, t, 0.0),
            _ => program.cx(q, t),
        };
    }
    program
}

/// The Pauli strings every engine is compared on, for `n` = 4 qubits.
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code, reason = "only the A100 qualification compares two engines")
)]
pub(in crate::tests) const STRINGS: [&str; 8] = [
    "ZIII", "IXII", "IIYI", "IIIZ", "XXYZ", "ZIZI", "YYYY", "IXIZ",
];

/// B0: on scrambled circuits, each string's expectation on `stack` agrees
/// with `reference` to `tolerance`.
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code, reason = "only the A100 qualification compares two engines")
)]
pub(in crate::tests) fn expectations_agree<E: MpsEngine, R: MpsEngine>(
    stack: &MpsStack<E>,
    reference: &MpsStack<R>,
    seeds: std::ops::Range<u64>,
    tolerance: f64,
) {
    let observables: Vec<_> = STRINGS
        .iter()
        .map(|labels| pauli(1.0, labels, &[0, 1, 2, 3]))
        .collect();
    for seed in seeds {
        let program = scrambled(4, seed).measure_all(&[0, 1, 2, 3]).build();
        let actual = expectations(stack, &program, &observables);
        let expected = expectations(reference, &program, &observables);
        for ((a, e), labels) in actual.iter().zip(&expected).zip(STRINGS) {
            assert!(
                (a - e).norm() <= tolerance,
                "seed {seed}, ⟨{labels}⟩: {a} against {e}"
            );
        }
    }
}

/// The seed of the per-shot runs: shot `j` of a run draws from seeds fixed
/// by it and `j`.
const RUN_SEED: u64 = 1;

/// The per-shot strategy on Bell pairs, each shot through the walk: the
/// two records agree, both values occur, and each shot's ln p is ln ½.
pub(in crate::tests) fn per_shot_bell_pairs<E: MpsEngine>(stack: &MpsStack<E>, shots: u32) {
    let strategy: PerShot<TensorNetShots<E>> = stack.per_shot(2).expect("a chain");
    let program = bell();
    let mut ones = 0;
    for j in 1..=shots {
        // The walk drives one shot: each shot has its own session, opened
        // on that shot with the run's seed (see `SubmissionWrapper`).
        let session = open(&strategy, RUN_SEED, &ShotSet::one(j)).expect("opens");
        let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session, j));
        let mut records = Vec::new();
        adapter.run(&program, &mut records).expect("runs");
        let row: Vec<bool> = records.iter().map(bit).collect();
        assert_eq!(row.len(), 2);
        assert_eq!(row[0], row[1], "shot {j}");
        ones += usize::from(row[0]);
        let report = adapter.close();
        let ln_p = report.inner.log_p.get(j).expect("the shot's ln p");
        assert!((ln_p - 0.5f64.ln()).abs() < 1e-9, "shot {j}: ln p = {ln_p}");
    }
    assert!(ones > 0 && ones < shots as usize, "{ones} of {shots}");
}

/// The per-shot strategy on feedback and resets, each shot through the
/// walk: a measurement, then a gate after it, then a reset and a
/// `MResetZ`, which the batch strategy refuses and a shot's own state runs.
pub(in crate::tests) fn per_shot_feedback_and_resets<E: MpsEngine>(
    stack: &MpsStack<E>,
    shots: u32,
) {
    let program = Bytecode::new(2)
        .h(0)
        .mz(0, 0)
        .cx(0, 1)
        .mz(1, 1)
        .reset(0)
        .mz(0, 2)
        .x(1)
        .mresetz(1, 3)
        .mz(1, 4)
        .record(0)
        .record(1)
        .record(2)
        .record(3)
        .record(4)
        .build();
    let strategy: PerShot<TensorNetShots<E>> = stack.per_shot(2).expect("a chain");
    let mut seen = [false; 2];
    for j in 1..=shots {
        let session = open(&strategy, RUN_SEED, &ShotSet::one(j)).expect("opens");
        let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session, j));
        let mut records = Vec::new();
        adapter.run(&program, &mut records).expect("runs");
        let row = &bits(&[records])[0];
        // q1 copies q0; reset q0 reads 0; X flips the copy; MResetZ leaves 0.
        assert_eq!(row[1], row[0], "shot {j}");
        assert!(!row[2], "shot {j}");
        assert_eq!(row[3], !row[0], "shot {j}");
        assert!(!row[4], "shot {j}");
        seen[usize::from(row[0])] = true;
        let report = adapter.close().inner;
        assert_eq!(report.shots.resets, 1);
        assert_eq!(report.shots.measurements, 5);
        let ln_p = report.log_p.get(j).expect("known");
        // Only the first draw is random; the reset of a known qubit and the
        // others are certain.
        assert!((ln_p - 0.5f64.ln()).abs() < 1e-9, "shot {j}: {ln_p}");
    }
    assert_eq!(seen, [true, true]);
}

/// The bits and the phase of the phase estimation case: 2^4 · 0.3 = 4.8, so
/// the estimate is 5 with probability 0.876 and 4 with probability 0.055.
pub(in crate::tests) const IPE_BITS: u32 = 4;
pub(in crate::tests) const IPE_PHASE: f64 = 0.3;

/// The estimate a row of [`iterative_phase_estimation`] reads, most
/// significant bit first.
pub(in crate::tests) fn estimate(records: &[OutputRecord]) -> usize {
    records
        .iter()
        .fold(0, |x, record| 2 * x + usize::from(bit(record)))
}

/// The total variation distance between the frequencies of `histogram` and
/// the distribution `expected`.
pub(in crate::tests) fn total_variation(histogram: &[u32], expected: &[f64]) -> f64 {
    let shots = f64::from(histogram.iter().sum::<u32>());
    histogram
        .iter()
        .zip(expected)
        .map(|(&n, p)| (f64::from(n) / shots - p).abs())
        .sum::<f64>()
        / 2.0
}

/// The distance [`total_variation`] allows between `shots` draws and their
/// distribution: 2/√shots. Its mean is at most Σₓ √(pₓ)/(2√shots), and
/// Σₓ √pₓ is 2.04 for the phase estimation case's distribution, so this
/// bound is about twice the mean; the seeds are fixed, so a pass is
/// reproducible.
pub(in crate::tests) fn allowed_distance(shots: u32) -> f64 {
    2.0 / f64::from(shots).sqrt()
}

/// `shots` shots of [`iterative_phase_estimation`] (the case's bits and
/// phase), each in its own per-shot session through the walk, as H₂ runs:
/// each shot's ln p is that of its estimate in textbook phase estimation,
/// to `tolerance`, since the resets are of measured qubits; the estimates
/// follow that distribution and peak at round(2^bits·phase). Returns the
/// histogram of the estimates.
pub(in crate::tests) fn per_shot_phase_estimation<E: MpsEngine>(
    stack: &MpsStack<E>,
    shots: u32,
    tolerance: f64,
) -> Vec<u32> {
    let program = iterative_phase_estimation(IPE_BITS, IPE_PHASE);
    let expected = phase_estimation_distribution(IPE_BITS, IPE_PHASE);
    let strategy: PerShot<TensorNetShots<E>> = stack.per_shot(3).expect("a chain");
    let mut histogram = vec![0; expected.len()];
    for j in 1..=shots {
        let session = open(&strategy, RUN_SEED, &ShotSet::one(j)).expect("opens");
        let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session, j));
        let mut records = Vec::new();
        adapter.run(&program, &mut records).expect("runs");
        assert_eq!(records.len(), IPE_BITS as usize, "shot {j}");
        let x = estimate(&records);
        histogram[x] += 1;
        let report = adapter.close().inner;
        assert_eq!(report.shots.measurements, u64::from(IPE_BITS), "shot {j}");
        assert_eq!(report.shots.resets, u64::from(IPE_BITS / 2), "shot {j}");
        let ln_p = report.log_p.get(j).expect("known");
        assert!(
            (ln_p - expected[x].ln()).abs() < tolerance,
            "shot {j}: estimate {x}, ln p {ln_p} against {}",
            expected[x].ln()
        );
    }
    let distance = total_variation(&histogram, &expected);
    println!("phase estimation: {shots} shots, histogram {histogram:?}, distance {distance:.4}");
    assert!(
        distance < allowed_distance(shots),
        "{histogram:?} against {expected:?}: distance {distance}"
    );
    assert_eq!(peak(&histogram), 5, "{histogram:?}");
    histogram
}

/// The most frequent value of `histogram`.
pub(in crate::tests) fn peak(histogram: &[u32]) -> usize {
    (0..histogram.len())
        .max_by_key(|&x| histogram[x])
        .expect("a non-empty histogram")
}
