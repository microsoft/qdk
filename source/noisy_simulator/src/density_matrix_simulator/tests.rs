// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::DensityMatrixSimulator;
use crate::tests::{noiseless_tests, noisy_tests};

#[test]
fn check_measuring_plus_state_yields_zero_with_50_percent_probability() {
    noiseless_tests::check_measuring_plus_state_in_mz_yields_zero_with_50_percent_probability::<
        DensityMatrixSimulator,
    >();
}

#[test]
fn check_measuring_plus_state_yields_one_with_50_percent_probability() {
    noiseless_tests::check_measuring_plus_state_in_mz_yields_one_with_50_percent_probability::<
        DensityMatrixSimulator,
    >();
}

#[test]
fn check_bell_pair_sampling_yields_same_outcome_for_both_qubits() {
    // We perform the test 100 times because of the probabilistic nature of the MZ measurement.
    for seed in 0..20 {
        noiseless_tests::check_bell_pair_sampling_yields_same_outcome_for_both_qubits::<
            DensityMatrixSimulator,
        >(seed, &noiseless_tests::Targets::MZ);
    }
}

#[test]
fn check_bell_pair_projection_on_mz0_yields_50_percent_probability_trace() {
    noiseless_tests::check_bell_pair_projection_on_mz0_yields_50_percent_probability_trace::<
        DensityMatrixSimulator,
    >();
}

#[test]
fn check_bell_pair_projection_on_mz1_yields_50_percent_probability_trace() {
    noiseless_tests::check_bell_pair_projection_on_mz1_yields_50_percent_probability_trace::<
        DensityMatrixSimulator,
    >();
}

#[test]
#[should_panic(expected = "operation should fail: ProbabilityZeroEvent")]
fn check_bell_pair_projection_on_oposite_directions_yields_an_error() {
    noiseless_tests::check_bell_pair_projection_on_oposite_directions_yields_an_error::<
        DensityMatrixSimulator,
    >();
}

#[test]
fn check_crx_gate_projection_on_mz0_yields_right_probabilities() {
    noiseless_tests::check_crx_gate_projection_on_mz0_yields_right_probabilities::<
        DensityMatrixSimulator,
    >();
}

#[test]
fn check_crx_gate_projection_on_mz1_yields_right_probabilities() {
    noiseless_tests::check_crx_gate_projection_on_mz1_yields_right_probabilities::<
        DensityMatrixSimulator,
    >();
}

#[test]
fn check_two_consecutive_mz_yield_same_outcome() {
    // We perform the test 100 times because of the probabilistic nature of the MZ measurement.
    for seed in 0..100 {
        noiseless_tests::check_two_consecutive_mz_yield_same_outcome::<DensityMatrixSimulator>(
            seed,
        );
    }
}

#[test]
fn check_alternating_mz_and_mx_yield_right_probabilities() {
    noiseless_tests::check_alternating_mz_and_mx_yield_right_probabilities::<DensityMatrixSimulator>(
    );
}

#[test]
fn check_noisy_identity_yields_same_qubit_with_right_probability() {
    noisy_tests::check_noisy_identity_yields_same_qubit_with_right_probability::<
        DensityMatrixSimulator,
    >();
}
