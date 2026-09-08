// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod logical_stack_trace;

use super::gate_error;
use qdk_simulators::noise_config::NoiseConfig;

#[test]
fn gate_error_sums_mutually_exclusive_fault_probabilities() {
    let mut config = NoiseConfig::<f64, f64>::NOISELESS;
    config.h.probabilities = vec![0.1, 0.25];

    assert_eq!(gate_error(&config, "__quantum__qis__h__body"), Some(0.35));
}

#[test]
fn gate_error_uses_the_matching_intrinsic_table() {
    let mut config = NoiseConfig::<f64, f64>::NOISELESS;
    config.x.probabilities = vec![0.1];
    config.cx.probabilities = vec![0.3];

    assert_eq!(gate_error(&config, "__quantum__qis__x__body"), Some(0.1));
    assert_eq!(gate_error(&config, "__quantum__qis__cx__body"), Some(0.3));
}

#[test]
fn gate_error_omits_noiseless_and_unknown_gates() {
    let config = NoiseConfig::<f64, f64>::NOISELESS;

    assert_eq!(gate_error(&config, "__quantum__qis__h__body"), None);
    assert_eq!(gate_error(&config, "custom_intrinsic"), None);
}
