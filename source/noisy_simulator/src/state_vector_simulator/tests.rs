// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::StateVectorSimulator;
use crate::tests::{noiseless_tests, noisy_tests};

#[test]
fn check_measuring_plus_state_in_mz_yields_zero_with_50_percent_probability() {
    noiseless_tests::check_measuring_plus_state_in_mz_yields_zero_with_50_percent_probability::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_measuring_plus_state_in_mz_yields_one_with_50_percent_probability() {
    noiseless_tests::check_measuring_plus_state_in_mz_yields_one_with_50_percent_probability::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_measuring_zero_state_in_mx_yields_plus_with_50_percent_probability() {
    noiseless_tests::check_measuring_zero_state_in_mx_yields_plus_with_50_percent_probability::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_measuring_zero_state_in_mx_yields_minus_with_50_percent_probability() {
    noiseless_tests::check_measuring_zero_state_in_mx_yields_minus_with_50_percent_probability::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_measuring_zero_state_in_my_yields_plus_with_50_percent_probability() {
    noiseless_tests::check_measuring_zero_state_in_my_yields_plus_with_50_percent_probability::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_measuring_plus_state_in_my_yields_minus_with_50_percent_probability() {
    noiseless_tests::check_measuring_plus_state_in_my_yields_minus_with_50_percent_probability::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_bell_pair_sampling_in_mz_yields_same_outcome_for_both_qubits() {
    // We perform the test 100 times because of the probabilistic nature of the MZ measurement.
    for seed in 0..20 {
        noiseless_tests::check_bell_pair_sampling_yields_same_outcome_for_both_qubits::<
            StateVectorSimulator,
        >(seed, &noiseless_tests::Targets::MZ);
    }
}

#[test]
fn check_bell_pair_sampling_in_mx_yields_same_outcome_for_both_qubits() {
    for seed in 0..20 {
        noiseless_tests::check_bell_pair_sampling_yields_same_outcome_for_both_qubits::<
            StateVectorSimulator,
        >(seed, &noiseless_tests::Targets::MX);
    }
}

#[test]
fn check_bell_pair_sampling_in_my_yields_opposite_outcome_for_both_qubits() {
    for seed in 0..20 {
        noiseless_tests::check_bell_pair_sampling_yields_opposite_outcome_for_both_qubits::<
            StateVectorSimulator,
        >(seed, &noiseless_tests::Targets::MY);
    }
}

#[test]
fn check_bell_pair_projection_on_mz0_yields_50_percent_probability_trace() {
    noiseless_tests::check_bell_pair_projection_on_mz0_yields_50_percent_probability_trace::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_bell_pair_projection_on_mz1_yields_50_percent_probability_trace() {
    noiseless_tests::check_bell_pair_projection_on_mz1_yields_50_percent_probability_trace::<
        StateVectorSimulator,
    >();
}

#[test]
#[should_panic(expected = "operation should fail: ProbabilityZeroEvent")]
fn check_bell_pair_projection_on_oposite_directions_yields_an_error() {
    noiseless_tests::check_bell_pair_projection_on_oposite_directions_yields_an_error::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_crx_gate_projection_on_mz0_yields_right_probabilities() {
    noiseless_tests::check_crx_gate_projection_on_mz0_yields_right_probabilities::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_crx_gate_projection_on_mz1_yields_right_probabilities() {
    noiseless_tests::check_crx_gate_projection_on_mz1_yields_right_probabilities::<
        StateVectorSimulator,
    >();
}

#[test]
fn check_two_consecutive_mz_yield_same_outcome() {
    // We perform the test 100 times because of the probabilistic nature of the MZ measurement.
    for seed in 0..100 {
        noiseless_tests::check_two_consecutive_mz_yield_same_outcome::<StateVectorSimulator>(seed);
    }
}

#[test]
fn check_two_consecutive_mx_yield_same_outcome() {
    // We perform the test 100 times because of the probabilistic nature of the MX measurement.
    for seed in 0..100 {
        noiseless_tests::check_two_consecutive_mx_yield_same_outcome::<StateVectorSimulator>(seed);
    }
}

#[test]
fn check_two_consecutive_my_yield_same_outcome() {
    // We perform the test 100 times because of the probabilistic nature of the MY measurement.
    for seed in 0..100 {
        noiseless_tests::check_two_consecutive_my_yield_same_outcome::<StateVectorSimulator>(seed);
    }
}

#[test]
fn check_alternating_mz_and_mx_yield_right_probabilities() {
    noiseless_tests::check_alternating_mz_and_mx_yield_right_probabilities::<StateVectorSimulator>(
    );
}

#[test]
fn check_noisy_identity_yields_same_qubit_with_right_probability() {
    noisy_tests::check_noisy_identity_yields_same_qubit_with_right_probability::<
        StateVectorSimulator,
    >();
}
