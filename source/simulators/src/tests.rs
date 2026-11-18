// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    MeasurementResult, Simulator, cpu_full_state_simulator::FullStateSimulator,
    noise_config::CumulativeNoiseConfig, stabilizer_simulator::StabilizerSimulator,
};
use std::sync::Arc;

fn check_forced_loss<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    let mut simulator = S::new(2, 4, 0, Arc::new(CumulativeNoiseConfig::default()));
    simulator.x(0);
    simulator.x(1);
    simulator.apply_loss_noise(1.0, 1);
    simulator.apply_loss_noise(1.0, 1);
    simulator.x(1);
    simulator.peek_loss(1, 0);
    simulator.mz(1, 1);
    simulator.mz(1, 2);
    simulator.mz(0, 3);

    assert_eq!(
        simulator.measurements(),
        &[
            MeasurementResult::One,
            MeasurementResult::Loss,
            MeasurementResult::Zero,
            MeasurementResult::One,
        ]
    );
}

#[test]
fn forced_loss_resets_only_the_target_and_is_idempotent() {
    check_forced_loss::<StabilizerSimulator>();
    check_forced_loss::<FullStateSimulator>();
}

fn check_two_qubit_measurements<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    let mut simulator = S::new(2, 4, 0, Arc::new(CumulativeNoiseConfig::default()));
    simulator.x(0);
    simulator.x(1);
    // Test MZZ
    simulator.mzz(0, 1, 0);
    // Test MXX
    simulator.h(0);
    simulator.h(1);
    simulator.mxx(0, 1, 1);
    // Test MYY
    simulator.s(0);
    simulator.s(1);
    simulator.myy(0, 1, 2);
    // Test MZY
    simulator.s_adj(0);
    simulator.h(0);
    simulator.mzy(0, 1, 3);
    assert_eq!(
        simulator.measurements(),
        &[
            MeasurementResult::Zero,
            MeasurementResult::Zero,
            MeasurementResult::Zero,
            MeasurementResult::Zero,
        ]
    );
}

#[test]
fn test_noiseless_two_qubit_measurements() {
    check_two_qubit_measurements::<StabilizerSimulator>();
    check_two_qubit_measurements::<FullStateSimulator>();
}
