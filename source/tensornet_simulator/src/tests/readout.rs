// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The readout distribution ([`ReadoutDistribution::Exact`]): each run of
//! consecutive measurements gives the exact joint distribution of its
//! values, checked against a dense state vector the tests build from the
//! gate tables; the values drawn follow it; and a terminal that does not
//! read it draws what it drew before.

use std::rc::Rc;

use executor_protocol::{Close as _, PerShot, ShotSet, SubmissionWrapper, open};
use num_complex::Complex64;
use qdk_simulators::{
    QubitID,
    execution::{RegionInstruction, RuntimeAdapter, UnitaryOperation, unitary_matrix},
};
use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use tensornet::{MpsEngine as _, reference::ExactReference};

use super::{
    Bytecode,
    cases::{allowed_distance, bit, per_shot_feedback_and_resets, per_shot_phase_estimation},
    fusion::{random_adaptive_program, random_operation, walk},
    measure, measure_reset, region, run_batch,
};
use crate::{
    GateFusion, MAX_RUN, MeasurementRun, MpsStack, ReadoutDistribution, TensorNetShots,
    TensorNetTerminal, mps_capacity,
};

/// A dense state vector of `qubits` qubits, qubit `k` the bit `k` of the
/// index, built from the gate tables without the engine.
struct Dense {
    amplitudes: Vec<Complex64>,
}

impl Dense {
    fn new(qubits: usize) -> Self {
        let mut amplitudes = vec![Complex64::new(0.0, 0.0); 1 << qubits];
        amplitudes[0] = Complex64::new(1.0, 0.0);
        Self { amplitudes }
    }

    fn apply(&mut self, operation: UnitaryOperation) {
        let Some(matrix) = unitary_matrix(operation) else {
            return;
        };
        let matrix = matrix.row_major();
        let sites: Vec<QubitID> = match operation.qubits() {
            (first, None) => vec![first],
            (first, Some(second)) => vec![first, second],
        };
        let k = sites.len();
        let dimension = 1 << k;
        // The index offset of each value of the gate's own index, whose
        // first site is the most significant bit.
        let offsets: Vec<usize> = (0..dimension)
            .map(|c| {
                (0..k)
                    .filter(|&j| c >> (k - 1 - j) & 1 == 1)
                    .fold(0, |offset, j| offset | 1 << sites[j])
            })
            .collect();
        let mask = offsets[dimension - 1];
        for base in (0..self.amplitudes.len()).filter(|i| i & mask == 0) {
            let old: Vec<Complex64> = offsets.iter().map(|&o| self.amplitudes[base | o]).collect();
            for (row, &offset) in offsets.iter().enumerate() {
                self.amplitudes[base | offset] = (0..dimension)
                    .map(|c| matrix[row * dimension + c] * old[c])
                    .sum();
            }
        }
    }

    fn apply_all(&mut self, operations: &[UnitaryOperation]) {
        for &operation in operations {
            self.apply(operation);
        }
    }

    /// The joint distribution of `qubits`, row `r` giving `qubits[i]` the
    /// bit `len − 1 − i` of `r`.
    fn marginal(&self, qubits: &[QubitID]) -> Vec<f64> {
        let k = qubits.len();
        let mut distribution = vec![0.0; 1 << k];
        for (i, amplitude) in self.amplitudes.iter().enumerate() {
            let row = qubits.iter().fold(0, |row, &q| (row << 1) | (i >> q & 1));
            distribution[row] += amplitude.norm_sqr();
        }
        distribution
    }

    /// Projects `qubit` on `one` and renormalizes.
    fn project(&mut self, qubit: QubitID, one: bool) {
        let mut norm = 0.0;
        for (i, amplitude) in self.amplitudes.iter_mut().enumerate() {
            if (i >> qubit & 1 == 1) == one {
                norm += amplitude.norm_sqr();
            } else {
                *amplitude = Complex64::new(0.0, 0.0);
            }
        }
        let scale = 1.0 / norm.sqrt();
        for amplitude in &mut self.amplitudes {
            *amplitude *= scale;
        }
    }
}

fn max_difference(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

fn terminal(qubits: usize, seed: u64) -> TensorNetTerminal<ExactReference> {
    TensorNetTerminal::new(
        Rc::new(ExactReference::new()),
        mps_capacity(qubits, 1 << qubits).expect("a chain"),
        seed,
    )
    .expect("prepares")
    .with_readout_distribution(ReadoutDistribution::Exact)
}

/// Sends each instruction as its own batch, as the walk sends a
/// measurement: the values of the measurements, in order.
fn one_by_one(
    terminal: &mut TensorNetTerminal<ExactReference>,
    instructions: &[RegionInstruction],
) -> Vec<bool> {
    let mut values = Vec::new();
    for instruction in instructions {
        let (outcomes, end) = run_batch(terminal, std::slice::from_ref(instruction));
        end.expect("runs");
        values.extend(
            outcomes
                .into_iter()
                .map(|(_, value)| value == executor_protocol::Value::One),
        );
    }
    values
}

fn random_gates(rng: &mut StdRng, qubits: usize, count: usize) -> Vec<UnitaryOperation> {
    (0..count).map(|_| random_operation(rng, qubits)).collect()
}

/// `qubits` in a random order.
fn shuffled(rng: &mut StdRng, qubits: usize) -> Vec<QubitID> {
    let mut order: Vec<QubitID> = (0..qubits).collect();
    for i in (1..qubits).rev() {
        order.swap(i, rng.random_range(0..=i));
    }
    order
}

#[test]
fn off_is_the_default() {
    let shots = TensorNetShots::new(Rc::new(ExactReference::new()), 2, 4).expect("a chain");
    assert_eq!(shots.readout_distribution(), ReadoutDistribution::Off);
    let stack = MpsStack::new(ExactReference::new());
    assert_eq!(stack.readout_distribution(), ReadoutDistribution::Off);
    let per_shot = stack
        .with_readout_distribution(ReadoutDistribution::Exact)
        .per_shot(2)
        .expect("a chain");
    assert_eq!(
        per_shot.0.readout_distribution(),
        ReadoutDistribution::Exact
    );
}

/// (a) A readout of k qubits, measured one batch at a time in any order,
/// is one run whose distribution is the dense state's marginal.
#[test]
fn a_readout_is_one_run_with_the_exact_distribution() {
    let mut rng = StdRng::seed_from_u64(11);
    for case in 0..30 {
        let qubits = 2 + case % 4;
        let gates = random_gates(&mut rng, qubits, 25);
        let order = shuffled(&mut rng, qubits);
        let mut dense = Dense::new(qubits);
        dense.apply_all(&gates);
        let expected = dense.marginal(&order);

        let mut terminal = terminal(qubits, case as u64);
        let mut instructions = vec![region(gates)];
        instructions.extend(order.iter().enumerate().map(|(t, &q)| measure(q, 10 + t)));
        let values = one_by_one(&mut terminal, &instructions);

        let [run] = terminal.measurement_runs() else {
            panic!(
                "case {case}: one run, not {:?}",
                terminal.measurement_runs()
            );
        };
        assert_eq!(run.qubits(), order.as_slice());
        let tags: Vec<u32> = (10..10 + u32::try_from(qubits).expect("fits")).collect();
        assert_eq!(run.tags(), tags.as_slice());
        assert_eq!(run.values(), values.as_slice());
        let distance = max_difference(run.distribution(), &expected);
        assert!(distance < 1e-12, "case {case}: {distance}");
        assert!((run.distribution().iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Each value was drawn with P(One) given the run's earlier values.
        let mut prefix = Dense::new(qubits);
        gates_again(&mut prefix, &instructions);
        for (i, (&q, &one)) in order.iter().zip(&values).enumerate() {
            let p1 = prefix.marginal(&[q])[1];
            assert!(
                (run.probabilities_of_one()[i] - p1).abs() < 1e-12,
                "case {case}"
            );
            prefix.project(q, one);
        }
    }
}

/// Applies the regions of `instructions` to `dense`.
fn gates_again(dense: &mut Dense, instructions: &[RegionInstruction]) {
    for instruction in instructions {
        if let RegionInstruction::ExecuteRegion { region, .. } = instruction {
            dense.apply_all(region.operations());
        }
    }
}

/// (b) A run followed by gates and more measurements: each run's
/// distribution is that of its values given the values drawn before it;
/// a gate, a reset, or a second measurement of a run's qubit ends a run,
/// and a region of no operator does not.
#[test]
fn each_run_is_read_given_the_values_drawn_before_it() {
    let mut rng = StdRng::seed_from_u64(3);
    for case in 0..20u64 {
        let first = random_gates(&mut rng, 4, 20);
        let second = random_gates(&mut rng, 4, 20);
        let instructions = [
            region(first.clone()),
            measure(0, 0),
            region([]),
            measure(2, 1),
            region(second.clone()),
            measure(1, 2),
            measure(3, 3),
            measure(1, 4),
            RegionInstruction::Reset { qubit: 0 },
            measure(0, 5),
        ];
        let mut terminal = terminal(4, case);
        let values = one_by_one(&mut terminal, &instructions);
        let runs: Vec<&MeasurementRun> = terminal.measurement_runs().iter().collect();
        let qubits: Vec<&[QubitID]> = runs.iter().map(|run| run.qubits()).collect();
        assert_eq!(qubits, [&[0, 2][..], &[1, 3], &[1], &[0]]);

        let mut dense = Dense::new(4);
        dense.apply_all(&first);
        let first_run = dense.marginal(&[0, 2]);
        assert!(max_difference(runs[0].distribution(), &first_run) < 1e-12);
        dense.project(0, values[0]);
        dense.project(2, values[1]);
        dense.apply_all(&second);
        let second_run = dense.marginal(&[1, 3]);
        assert!(max_difference(runs[1].distribution(), &second_run) < 1e-12);
        // Qubit 1 again: certain, its value repeated.
        let repeated = if values[2] { [0.0, 1.0] } else { [1.0, 0.0] };
        assert!(max_difference(runs[2].distribution(), &repeated) < 1e-12);
        assert_eq!(values[4], values[2]);
        // After the reset, qubit 0 reads Zero.
        assert!(max_difference(runs[3].distribution(), &[1.0, 0.0]) < 1e-12);
        assert!(!values[5]);
    }
}

/// An `MResetZ` in a run flips its qubit back after the run's projections:
/// the qubit reads Zero afterwards, and the run's other values are those
/// of the state before it.
#[test]
fn a_reset_measurement_in_a_run_resets_its_qubit_when_the_run_ends() {
    for seed in 0..20 {
        let mut terminal = terminal(2, seed);
        let values = one_by_one(
            &mut terminal,
            &[
                region([super::h(0), super::cx(0, 1)]),
                measure_reset(0, 0),
                measure(1, 1),
                region([UnitaryOperation::Z { target: 1 }]),
                measure(0, 2),
            ],
        );
        assert_eq!(values[0], values[1], "seed {seed}");
        assert!(!values[2], "seed {seed}");
        let runs = terminal.measurement_runs();
        assert!(max_difference(runs[0].distribution(), &[0.5, 0.0, 0.0, 0.5]) < 1e-12);
    }
}

/// While a run is open, the terminal's state is the state before it (its
/// projections are still to apply), as `TensorNetTerminal::state` says; at
/// `close` it is projected.
#[test]
fn an_open_run_projects_at_close() {
    let mut terminal = terminal(1, 1);
    let values = one_by_one(&mut terminal, &[region([super::h(0)]), measure(0, 0)]);
    let engine = ExactReference::new();
    let state = terminal.state().expect("held");
    assert!((engine.probability_of_one(state, 0).expect("reads") - 0.5).abs() < 1e-12);
    terminal.close();
    let state = terminal.state().expect("kept");
    let p1 = engine.probability_of_one(state, 0).expect("reads");
    assert!((p1 - f64::from(u8::from(values[0]))).abs() < 1e-12);
}

/// The capacity is checked on the states a terminal that projects at once
/// checks: a Bell pair outgrows a bond of 1 only until its measurement's
/// projection, so neither terminal fails the shot, and both report bond 1;
/// a gate that entangles again after the run outgrows it in both.
#[test]
fn the_capacity_is_checked_once_a_run_is_projected() {
    for readout in [ReadoutDistribution::Off, ReadoutDistribution::Exact] {
        let mut terminal = TensorNetTerminal::new(
            Rc::new(ExactReference::new()),
            mps_capacity(2, 1).expect("a chain"),
            5,
        )
        .expect("prepares")
        .with_readout_distribution(readout);
        let (_, end) = run_batch(
            &mut terminal,
            &[region([super::h(0), super::cx(0, 1)]), measure(0, 0)],
        );
        end.unwrap_or_else(|e| panic!("{readout:?}: {e}"));
        let (_, end) = run_batch(&mut terminal, &[measure(1, 1)]);
        end.unwrap_or_else(|e| panic!("{readout:?}: {e}"));
        let (_, end) = run_batch(&mut terminal, &[region([super::h(0), super::cx(0, 1)])]);
        let reason = end.expect_err("a Bell pair outgrows a bond of 1");
        assert!(
            reason.contains("beyond its capacity"),
            "{readout:?}: {reason}"
        );
    }
    for readout in [ReadoutDistribution::Off, ReadoutDistribution::Exact] {
        let mut terminal = TensorNetTerminal::new(
            Rc::new(ExactReference::new()),
            mps_capacity(2, 1).expect("a chain"),
            5,
        )
        .expect("prepares")
        .with_readout_distribution(readout);
        let (_, end) = run_batch(
            &mut terminal,
            &[region([super::h(0), super::cx(0, 1)]), measure(0, 0)],
        );
        end.unwrap_or_else(|e| panic!("{readout:?}: {e}"));
        let report = terminal.close();
        assert_eq!(report.max_bond, 1, "{readout:?}");
        assert!(terminal.state().is_some(), "{readout:?}");
    }
}

/// A run ends at [`MAX_RUN`] measurements, and the next one starts.
#[test]
fn a_run_ends_at_its_largest_length() {
    let qubits = MAX_RUN + 2;
    let mut terminal = terminal(qubits, 4);
    let mut instructions = vec![region((0..qubits).map(super::h).collect::<Vec<_>>())];
    instructions.extend((0..qubits).map(|q| measure(q, q)));
    one_by_one(&mut terminal, &instructions);
    let lengths: Vec<usize> = terminal
        .measurement_runs()
        .iter()
        .map(MeasurementRun::len)
        .collect();
    assert_eq!(lengths, [MAX_RUN, 2]);
    let uniform = vec![1.0 / f64::from(1 << MAX_RUN); 1 << MAX_RUN];
    assert!(max_difference(terminal.measurement_runs()[0].distribution(), &uniform) < 1e-12);
}

/// The rows of probability zero (a GHZ state's) are not refined, so no
/// conditional is read on an impossible row; the distribution is exact.
#[test]
fn impossible_rows_are_zero_without_a_read() {
    for seed in 0..10 {
        let mut terminal = terminal(4, seed);
        let gates = vec![
            super::h(0),
            super::cx(0, 1),
            super::cx(1, 2),
            super::cx(2, 3),
        ];
        let mut instructions = vec![region(gates)];
        instructions.extend((0..4).map(|q| measure(q, q)));
        let values = one_by_one(&mut terminal, &instructions);
        assert!(values.iter().all(|&v| v == values[0]));
        let mut expected = vec![0.0; 16];
        expected[0] = 0.5;
        expected[15] = 0.5;
        let [run] = terminal.measurement_runs() else {
            panic!("one run");
        };
        assert!(max_difference(run.distribution(), &expected) < 1e-12);
    }
}

/// The program of the averaging case: qubit 0 is measured mid-circuit and
/// steers qubit 1 (CX copies it); after more gates, qubits 1 and 2 are
/// read. Each shot's readout distribution depends on the mid-circuit value.
fn mid_circuit_then_readout() -> (
    qdk_simulators::execution::PreparedAdaptiveProgram<u64>,
    Vec<f64>,
) {
    let (a, b, c) = (1.1, 0.7, 2.3);
    let program = Bytecode::new(3)
        .rx(a, 0)
        .cx(0, 1)
        .mz(0, 0)
        .rx(b, 1)
        .rx(c, 2)
        .cx(1, 2)
        .mz(1, 1)
        .mz(2, 2)
        .record(1)
        .record(2)
        .build();
    let ops = |a: f64, b: f64, c: f64| {
        [
            UnitaryOperation::Rx {
                angle: a,
                target: 0,
            },
            super::cx(0, 1),
        ]
        .into_iter()
        .chain([
            UnitaryOperation::Rx {
                angle: b,
                target: 1,
            },
            UnitaryOperation::Rx {
                angle: c,
                target: 2,
            },
            super::cx(1, 2),
        ])
        .collect::<Vec<_>>()
    };
    let gates = ops(a, b, c);
    let mut expected = vec![0.0; 4];
    for m in [false, true] {
        let mut dense = Dense::new(3);
        dense.apply_all(&gates[..2]);
        let p = dense.marginal(&[0])[usize::from(m)];
        dense.project(0, m);
        dense.apply_all(&gates[2..]);
        for (e, q) in expected.iter_mut().zip(dense.marginal(&[1, 2])) {
            *e += p * q;
        }
    }
    (program, expected)
}

/// (c) With a mid-circuit measurement before the readout, the mean of the
/// shots' readout distributions converges to the readout's exact
/// distribution, closer than the frequencies of the rows drawn
/// (Rao–Blackwell), which agree with it too.
#[test]
fn the_mean_over_shots_converges_to_the_exact_distribution() {
    let (program, expected) = mid_circuit_then_readout();
    let per_shot = MpsStack::new(ExactReference::new())
        .with_readout_distribution(ReadoutDistribution::Exact)
        .per_shot(3)
        .expect("a chain");
    let shots = 2000;
    let mut mean = vec![0.0; 4];
    let mut counts = vec![0u32; 4];
    for j in 1..=shots {
        let (records, runs) = walk_runs(&per_shot, &program, 17, j);
        let [mid, readout] = runs.as_slice() else {
            panic!("shot {j}: two runs, not {runs:?}");
        };
        assert_eq!(mid.qubits(), [0]);
        assert_eq!(readout.qubits(), [1, 2]);
        assert_eq!(readout.tags(), [1, 2]);
        let row = records
            .iter()
            .fold(0, |row, record| (row << 1) | usize::from(bit(record)));
        assert_eq!(row, readout.row());
        counts[row] += 1;
        for (m, p) in mean.iter_mut().zip(readout.distribution()) {
            *m += p / f64::from(shots);
        }
    }
    let tvd = |a: &[f64]| {
        a.iter()
            .zip(&expected)
            .map(|(x, y)| (x - y).abs())
            .sum::<f64>()
            / 2.0
    };
    let frequencies: Vec<f64> = counts
        .iter()
        .map(|&n| f64::from(n) / f64::from(shots))
        .collect();
    let (mean_distance, count_distance) = (tvd(&mean), tvd(&frequencies));
    println!("readout mean {mean_distance:.5}, counts {count_distance:.5}, {shots} shots");
    assert!(
        count_distance < allowed_distance(shots),
        "{counts:?} against {expected:?}"
    );
    assert!(
        mean_distance < allowed_distance(shots),
        "{mean:?} against {expected:?}"
    );
    assert!(
        mean_distance < count_distance,
        "{mean_distance} against {count_distance}"
    );
}

/// Shot `j` of `program` through the walk over `per_shot`, opened with
/// `seed`: its records and its measurement runs, read before the session
/// closes.
fn walk_runs(
    per_shot: &PerShot<TensorNetShots<ExactReference>>,
    program: &qdk_simulators::execution::PreparedAdaptiveProgram<u64>,
    seed: u64,
    j: u32,
) -> (Vec<qdk_simulators::OutputRecord>, Vec<MeasurementRun>) {
    let session = open(per_shot, seed, &ShotSet::one(j)).expect("opens");
    let mut adapter = RuntimeAdapter::new(SubmissionWrapper::new(session, j));
    let mut records = Vec::new();
    adapter.run(program, &mut records).expect("runs");
    let mut wrapper = adapter.into_inner();
    let runs = wrapper
        .inner()
        .shot(j)
        .expect("held")
        .measurement_runs()
        .to_vec();
    wrapper.close();
    (records, runs)
}

/// (e) The rows drawn follow the distribution: χ² of 4,000 shots of a
/// three-qubit readout over its eight rows, below 24.32, the 0.1% critical
/// value with seven degrees of freedom; the seeds are fixed, so a pass is
/// reproducible.
#[test]
fn the_rows_drawn_follow_the_distribution() {
    let gates = vec![
        UnitaryOperation::Ry {
            angle: 0.9,
            target: 0,
        },
        super::cx(0, 1),
        UnitaryOperation::Rx {
            angle: 1.7,
            target: 2,
        },
        UnitaryOperation::Ry {
            angle: 0.4,
            target: 1,
        },
        super::cx(2, 0),
    ];
    let mut dense = Dense::new(3);
    dense.apply_all(&gates);
    let expected = dense.marginal(&[2, 0, 1]);
    let shots = 4000u32;
    let mut counts = [0u32; 8];
    for seed in 0..u64::from(shots) {
        let mut terminal = terminal(3, seed);
        one_by_one(
            &mut terminal,
            &[
                region(gates.clone()),
                measure(2, 0),
                measure(0, 1),
                measure(1, 2),
            ],
        );
        let [run] = terminal.measurement_runs() else {
            panic!("one run");
        };
        assert!(max_difference(run.distribution(), &expected) < 1e-12);
        counts[run.row()] += 1;
    }
    let chi2: f64 = counts
        .iter()
        .zip(&expected)
        .map(|(&n, &p)| {
            let e = p * f64::from(shots);
            (f64::from(n) - e).powi(2) / e
        })
        .sum();
    println!("χ² {chi2:.2} over {counts:?}, expected {expected:?}");
    assert!(chi2 < 24.32, "χ² {chi2}: {counts:?} against {expected:?}");
}

/// The fingerprint of 40 random Adaptive programs (feedback, resets,
/// `MResetZ`), five shots each, unfused and in pairs, on the reference: an
/// FNV-1a hash over each record's bit and each shot's ln p, and the count
/// of One records.
fn fingerprint(readout: ReadoutDistribution) -> (u64, u32, Vec<(Vec<bool>, f64)>) {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut ones = 0u32;
    let mut rows = Vec::new();
    let mut mix = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    for seed in 0..40 {
        let qubits = 2 + (seed % 4) as u32;
        let program = random_adaptive_program(seed, qubits, 6);
        for fusion in [GateFusion::Unfused, GateFusion::Pairs] {
            let per_shot = MpsStack::new(ExactReference::new())
                .with_gate_fusion(fusion)
                .with_readout_distribution(readout)
                .per_shot(qubits as usize)
                .expect("a chain");
            for j in 1..=5 {
                let (records, log_p, _) = walk(&per_shot, &program, seed, j);
                let row: Vec<bool> = records.iter().map(bit).collect();
                for &one in &row {
                    ones += u32::from(one);
                    mix(u8::from(one));
                }
                for byte in log_p.to_bits().to_le_bytes() {
                    mix(byte);
                }
                rows.push((row, log_p));
            }
        }
    }
    (hash, ones, rows)
}

/// (d) Off, the terminal draws what it drew before the readout
/// distribution existed: the fingerprint is the one the terminal at
/// `218e5434a` gives for the same programs and seeds, bit for bit (ln p
/// included).
#[test]
fn off_draws_what_the_terminal_drew_before() {
    let (hash, ones, _) = fingerprint(ReadoutDistribution::Off);
    assert_eq!((hash, ones), (0x97dd_c5b0_6d65_4431, 968));
}

/// Exact draws the same values as Off for the same seed, with ln p equal to
/// rounding: the same random numbers, and P(One) equal to rounding (read
/// given the run's values rather than after projecting on them).
#[test]
fn exact_draws_the_same_values_as_off() {
    let (_, off_ones, off) = fingerprint(ReadoutDistribution::Off);
    let (_, exact_ones, exact) = fingerprint(ReadoutDistribution::Exact);
    assert_eq!(off_ones, exact_ones);
    for ((off_row, off_log_p), (row, log_p)) in off.iter().zip(&exact) {
        assert_eq!(off_row, row);
        assert!(
            (off_log_p - log_p).abs() < 1e-9,
            "{off_log_p} against {log_p}"
        );
    }
    assert_eq!(off.len(), exact.len());
}

/// The cases with feedback and resets, and phase estimation, with the
/// readout distribution read: their values and ln p are unchanged.
#[test]
fn feedback_resets_and_phase_estimation_with_the_readout_distribution() {
    let stack =
        MpsStack::new(ExactReference::new()).with_readout_distribution(ReadoutDistribution::Exact);
    per_shot_feedback_and_resets(&stack, 20);
    per_shot_phase_estimation(&stack.with_gate_fusion(GateFusion::Pairs), 400, 1e-9);
}
