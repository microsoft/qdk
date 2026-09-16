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
    simulator.lose(1);
    simulator.lose(1);
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
