// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    NoisySimulator,
    instrument::Instrument,
    operation::{Operation, operation},
};
use num_complex::Complex;

use super::assert_approx_eq;

pub enum Targets {
    MX,
    MY,
    MZ,
}

/// Returns an H gate.
fn noiseless_h() -> Operation {
    let f = 0.5_f64.sqrt();
    operation!([f,  f;
                f, -f;])
    .expect("operation should be valid")
}

/// Returns a CNOT gate.
fn noiseless_cnot() -> Operation {
    operation!([1., 0., 0., 0.;
                0., 1., 0., 0.;
                0., 0., 0., 1.;
                0., 0., 1., 0.;])
    .expect("operation should be valid")
}

/// Returns the 0-projection of an MZ measurement.
fn noiseless_mz0() -> Operation {
    operation!([1., 0.;
                0., 0.;])
    .expect("operation should be valid")
}

/// Returns the 1-projection of an MZ measurement.
fn noiseless_mz1() -> Operation {
    operation!([0., 0.;
                0., 1.;])
    .expect("operation should be valid")
}

/// Returns an MZ measurement.
pub(super) fn noiseless_mz() -> Instrument {
    Instrument::new(vec![noiseless_mz0(), noiseless_mz1()]).expect("instrument should be valid")
}

/// Returns the |+>-projection of an MX measurement.
fn noiseless_mx0() -> Operation {
    operation!([0.5, 0.5;
                0.5, 0.5;])
    .expect("operation should be valid")
}

/// Returns the |->-projection of an MX measurement.
fn noiseless_mx1() -> Operation {
    operation!([0.5, -0.5;
                -0.5, 0.5;])
    .expect("operation should be valid")
}

/// Returns an MX measurement.
pub(super) fn noiseless_mx() -> Instrument {
    Instrument::new(vec![noiseless_mx0(), noiseless_mx1()]).expect("instrument should be valid")
}

/// Returns the |i>-projection of an MY measurement.
fn noiseless_my0() -> Operation {
    operation!([0.5, -0.5 * Complex::i();
                0.5 * Complex::i(), 0.5;])
    .expect("operation should be valid")
}
/// Returns the |-i>-projection of an MY measurement.
fn noiseless_my1() -> Operation {
    operation!([0.5, 0.5 * Complex::i();
                -0.5 * Complex::i(), 0.5;])
    .expect("operation should be valid")
}

/// Returns an MY measurement.
pub(super) fn noiseless_my() -> Instrument {
    Instrument::new(vec![noiseless_my0(), noiseless_my1()]).expect("instrument should be valid")
}

pub(super) fn noiseless_instrument(target: &Targets) -> Instrument {
    match target {
        Targets::MX => noiseless_mx(),
        Targets::MY => noiseless_my(),
        Targets::MZ => noiseless_mz(),
    }
}

pub fn check_measuring_plus_state_in_mz_yields_zero_with_50_percent_probability<
    NS: NoisySimulator,
>() {
    let h = noiseless_h();
    let mz = noiseless_mz();
    let mut sim = NS::new(1);
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");

    // Random samples less than 0.5 should yield a 0-measurement.
    let measurement = sim
        .sample_instrument_with_distribution(&mz, &[0], 0.49999)
        .expect("measurement should succeed");
    assert_eq!(measurement, 0);
}

pub fn check_measuring_plus_state_in_mz_yields_one_with_50_percent_probability<
    NS: NoisySimulator,
>() {
    let h = noiseless_h();
    let mz = noiseless_mz();
    let mut sim = NS::new(1);
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");

    // Random samples greater than 0.5 should yield a 1-measurement.
    let measurement = sim
        .sample_instrument_with_distribution(&mz, &[0], 0.50001)
        .expect("measurement should succeed");
    assert_eq!(measurement, 1);
}

pub fn check_measuring_zero_state_in_mx_yields_plus_with_50_percent_probability<
    NS: NoisySimulator,
>() {
    let mx = noiseless_mx();
    let mut sim = NS::new(1);

    // Random samples less than 0.5 should yield a |+> measurement.
    let measurement = sim
        .sample_instrument_with_distribution(&mx, &[0], 0.49999)
        .expect("measurement should succeed");
    assert_eq!(measurement, 0); // 0 corresponds to the |+> outcome
}

pub fn check_measuring_zero_state_in_mx_yields_minus_with_50_percent_probability<
    NS: NoisySimulator,
>() {
    let mx = noiseless_mx();
    let mut sim = NS::new(1);

    // Random samples greater than 0.5 should yield a |-> measurement.
    let measurement = sim
        .sample_instrument_with_distribution(&mx, &[0], 0.50001)
        .expect("measurement should succeed");
    assert_eq!(measurement, 1); // 1 corresponds to the |-> outcome
}

pub fn check_measuring_zero_state_in_my_yields_plus_with_50_percent_probability<
    NS: NoisySimulator,
>() {
    let my = noiseless_my();
    let mut sim = NS::new(1);

    // Random samples less than 0.5 should yield a |+i> measurement.
    let measurement = sim
        .sample_instrument_with_distribution(&my, &[0], 0.49999)
        .expect("measurement should succeed");
    assert_eq!(measurement, 0); // 0 corresponds to the |+i> outcome
}

pub fn check_measuring_plus_state_in_my_yields_minus_with_50_percent_probability<
    NS: NoisySimulator,
>() {
    let h = noiseless_h();
    let my = noiseless_my();
    let mut sim = NS::new(1);
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");

    // Random samples greater than 0.5 should yield a |-i> measurement.
    let measurement = sim
        .sample_instrument_with_distribution(&my, &[0], 0.50001)
        .expect("measurement should succeed");
    assert_eq!(measurement, 1); // 1 corresponds to the |-i> outcome
}

/// Check that both measurements in a Bell Pair yield the same result.
pub fn check_bell_pair_sampling_yields_same_outcome_for_both_qubits<NS: NoisySimulator>(
    seed: u64,
    target: Targets,
) {
    let (h, cnot, m) = (
        noiseless_h(),
        noiseless_cnot(),
        noiseless_instrument(&target),
    );
    let mut sim = NS::new_with_seed(2, seed);

    // Make a Bell Pair.
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&cnot, &[1, 0])
        .expect("operation should succeed");

    // Measure both qubits.
    let m1 = sim
        .sample_instrument(&m, &[0])
        .expect("measurement should succeed");
    let m2 = sim
        .sample_instrument(&m, &[1])
        .expect("measurement should succeed");

    // Check that both measurements yield the same result.
    assert_eq!(m1, m2);
}

/// Check that both measurements in a Bell Pair yield the same result.
pub fn check_bell_pair_sampling_yields_opposite_outcome_for_both_qubits<NS: NoisySimulator>(
    seed: u64,
    target: Targets,
) {
    let (h, cnot, m) = (
        noiseless_h(),
        noiseless_cnot(),
        noiseless_instrument(&target),
    );
    let mut sim = NS::new_with_seed(2, seed);

    // Make a Bell Pair.
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&cnot, &[1, 0])
        .expect("operation should succeed");

    // Measure both qubits.
    let m1 = sim
        .sample_instrument(&m, &[0])
        .expect("measurement should succeed");
    let m2 = sim
        .sample_instrument(&m, &[1])
        .expect("measurement should succeed");

    // Check that both measurements yield opposite results.
    assert_ne!(m1, m2);
}

/// Project both qubits of a Bell Pair on the mz0 direction.
/// The trace of the system (i.e. the probability of finding
/// the quantum system in this state) should be 0.5.
pub fn check_bell_pair_projection_on_mz0_yields_50_percent_probability_trace<NS: NoisySimulator>() {
    let (h, cnot, mz0) = (noiseless_h(), noiseless_cnot(), noiseless_mz0());
    let mut sim = NS::new(2);

    // Make a Bell Pair.
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&cnot, &[1, 0])
        .expect("operation should succeed");

    // Project both qubits on the mz0 direction.
    sim.apply_operation(&mz0, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&mz0, &[1])
        .expect("operation should succeed");
    assert_approx_eq(0.5, sim.trace_change().expect("state should be valid"));

    // Repeating the projection twice should yield the same result.
    sim.apply_operation(&mz0, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&mz0, &[1])
        .expect("operation should succeed");
    assert_approx_eq(0.5, sim.trace_change().expect("state should be valid"));
}

/// Project both qubits of a Bell Pair on the mz1 direction.
/// The trace of the system (i.e. the probability of finding
/// the quantum system in this state) should be 0.5.
pub fn check_bell_pair_projection_on_mz1_yields_50_percent_probability_trace<NS: NoisySimulator>() {
    let (h, cnot, mz1) = (noiseless_h(), noiseless_cnot(), noiseless_mz1());
    let mut sim = NS::new(2);

    // Make a Bell Pair.
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&cnot, &[1, 0])
        .expect("operation should succeed");

    // Project both qubits on the mz1 direction.
    sim.apply_operation(&mz1, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&mz1, &[1])
        .expect("operation should succeed");
    assert_approx_eq(0.5, sim.trace_change().expect("state should be valid"));

    // Repeating the projection twice should yield the same result.
    sim.apply_operation(&mz1, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&mz1, &[1])
        .expect("operation should succeed");
    assert_approx_eq(0.5, sim.trace_change().expect("state should be valid"));
}

/// Project one qubit of a Bell Pair on the mz0 direction and the other on the mz1 direction.
/// This should yield a 0-probability error.
pub fn check_bell_pair_projection_on_oposite_directions_yields_an_error<NS: NoisySimulator>() {
    let (h, cnot, mz0, mz1) = (
        noiseless_h(),
        noiseless_cnot(),
        noiseless_mz0(),
        noiseless_mz1(),
    );
    let mut sim = NS::new(2);

    // Make a Bell Pair.
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");
    sim.apply_operation(&cnot, &[1, 0])
        .expect("operation should succeed");

    // Project first qubit on the mz0 direction.
    sim.apply_operation(&mz0, &[0])
        .expect("operation should succeed");

    // Project second qubit on the mz1 direction.
    // This should yield a 0-probability error.
    sim.apply_operation(&mz1, &[1])
        .expect("operation should fail");
}

/// Check that projecting the target qubit in a CRX gate on the mz0 direction yields the right probabilities.
pub fn check_crx_gate_projection_on_mz0_yields_right_probabilities<NS: NoisySimulator>() {
    let (h, mz0, mz1) = (noiseless_h(), noiseless_mz0(), noiseless_mz1());
    let probabilities: Vec<f64> = vec![0.05, 0.1, 0.3, 0.7, 0.8, 0.9, 0.99];

    // A CRX gate (Controlled Rotation around X axis).
    let crx = |t: f64| {
        let c = t.cos();
        let s = t.sin() * Complex::I;
        operation!([1., 0., 0., 0.;
                    0., 1., 0., 0.;
                    0., 0., c,  s;
                    0., 0., s,  c;])
        .expect("operation should be valid")
    };

    for p in &probabilities {
        let t = p.sqrt().acos();
        let mut sim = NS::new(2);

        // Apply CRX gate
        sim.apply_operation(&h, &[0])
            .expect("operation should succeed");
        sim.apply_operation(&crx(0.3 * t), &[1, 0])
            .expect("operation should succeed");
        sim.apply_operation(&crx(0.7 * t), &[1, 0])
            .expect("operation should succeed");

        sim.apply_operation(&mz1, &[0])
            .expect("operation should succeed");
        assert_approx_eq(0.5, sim.trace_change().expect("state should be valid"));

        // Project target qubit on mz0 and check the trace
        // (the probability of finding the system in that state).
        sim.apply_operation(&mz0, &[1])
            .expect("operation should succeed");
        assert_approx_eq(0.5 * *p, sim.trace_change().expect("state should be valid"));

        // Repeating a projection should yield the same result.
        sim.apply_operation(&mz0, &[1])
            .expect("operation should succeed");
        assert_approx_eq(0.5 * *p, sim.trace_change().expect("state should be valid"));
    }
}

/// Check that projecting the target qubit in a CRX gate on the mz1 direction yields the right probabilities.
pub fn check_crx_gate_projection_on_mz1_yields_right_probabilities<NS: NoisySimulator>() {
    let (h, mz1) = (noiseless_h(), noiseless_mz1());
    let probabilities: Vec<f64> = vec![0.05, 0.1, 0.3, 0.7, 0.8, 0.9, 0.99];

    // A CRX gate (Controlled Rotation around X axis).
    let crx = |t: f64| {
        let c = t.cos();
        let s = t.sin() * Complex::I;
        operation!([1., 0., 0., 0.;
                    0., 1., 0., 0.;
                    0., 0., c,  s;
                    0., 0., s,  c;])
        .expect("operation should be valid")
    };

    for p in &probabilities {
        let t = p.sqrt().acos();
        let mut sim = NS::new(2);

        // Apply CRX gate
        sim.apply_operation(&h, &[0])
            .expect("operation should succeed");
        sim.apply_operation(&crx(0.3 * t), &[1, 0])
            .expect("operation should succeed");
        sim.apply_operation(&crx(0.7 * t), &[1, 0])
            .expect("operation should succeed");

        sim.apply_operation(&mz1, &[0])
            .expect("operation should succeed");
        assert_approx_eq(0.5, sim.trace_change().expect("state should be valid"));

        // Project target qubit on mz1 and check the trace
        // (the probability of finding the system in that state).
        sim.apply_operation(&mz1, &[1])
            .expect("operation should succeed");
        assert_approx_eq(
            0.5 * (1. - *p),
            sim.trace_change().expect("state should be valid"),
        );

        // Repeating a projection should yield the same result.
        sim.apply_operation(&mz1, &[1])
            .expect("operation should succeed");
        assert_approx_eq(
            0.5 * (1. - *p),
            sim.trace_change().expect("state should be valid"),
        );
    }
}

/// Check that two consecutive MZ on the same qubit yield the same outcome.
pub fn check_two_consecutive_mz_yield_same_outcome<NS: NoisySimulator>(seed: u64) {
    let h = noiseless_h();
    let mz = noiseless_mz();
    let mut sim = NS::new_with_seed(1, seed);

    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");
    let outcome_0 = sim
        .sample_instrument(&mz, &[0])
        .expect("measurement should succeed");
    let outcome_1 = sim
        .sample_instrument(&mz, &[0])
        .expect("measurement should succeed");
    assert_eq!(outcome_0, outcome_1);
}

/// Check that two consecutive MX on the same qubit yield the same outcome.
pub fn check_two_consecutive_mx_yield_same_outcome<NS: NoisySimulator>(seed: u64) {
    let mx = noiseless_mx();
    let mut sim = NS::new_with_seed(1, seed);

    let outcome_0 = sim
        .sample_instrument(&mx, &[0])
        .expect("measurement should succeed");
    let outcome_1 = sim
        .sample_instrument(&mx, &[0])
        .expect("measurement should succeed");
    assert_eq!(outcome_0, outcome_1);
}

/// Check that two consecutive MY on the same qubit yield the same outcome.
pub fn check_two_consecutive_my_yield_same_outcome<NS: NoisySimulator>(seed: u64) {
    let my = noiseless_my();
    let mut sim = NS::new_with_seed(1, seed);

    let outcome_0 = sim
        .sample_instrument(&my, &[0])
        .expect("measurement should succeed");
    let outcome_1 = sim
        .sample_instrument(&my, &[0])
        .expect("measurement should succeed");
    assert_eq!(outcome_0, outcome_1);
}

pub fn check_alternating_mz_and_mx_yield_right_probabilities<NS: NoisySimulator>() {
    let h = noiseless_h();
    let mz = noiseless_mz();
    let mx = noiseless_mx();

    let mut sim = NS::new(1);
    sim.apply_operation(&h, &[0])
        .expect("operation should succeed");
    let mut prob = 1.0;

    // Alternate MZ and MX 5 times.
    for _ in 0..5 {
        sim.sample_instrument(&mz, &[0])
            .expect("measurement should succeed");
        prob *= 0.5;
        assert_approx_eq(prob, sim.trace_change().expect("state should be valid"));
        sim.sample_instrument(&mx, &[0])
            .expect("measurement should succeed");
        prob *= 0.5;
        assert_approx_eq(prob, sim.trace_change().expect("state should be valid"));
    }
}
