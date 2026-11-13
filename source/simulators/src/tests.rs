// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    MeasurementResult, Simulator, cpu_full_state_simulator::FullStateSimulator,
    noise_config::CumulativeNoiseConfig, seed::share_draws,
    stabilizer_simulator::StabilizerSimulator,
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

#[test]
fn noise_and_measurement_streams_share_no_draws() {
    for seed in [0, 1, u64::MAX] {
        let noise = Arc::new(CumulativeNoiseConfig::default());
        let mut cpu = FullStateSimulator::new(1, 1, seed, noise.clone());
        assert!(!share_draws(cpu.rngs_mut()), "cpu, seed {seed}");
        let mut stabilizer = StabilizerSimulator::new(1, 1, seed, noise.clone());
        assert!(
            !share_draws(stabilizer.rngs_mut()),
            "stabilizer, seed {seed}"
        );
        stabilizer.set_seed(seed);
        assert!(
            !share_draws(stabilizer.rngs_mut()),
            "reseeded stabilizer, seed {seed}"
        );
    }
}
