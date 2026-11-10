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

#[test]
fn seeded_cpu_noise_and_measurements_are_independent() {
    let noise = Arc::new(CumulativeNoiseConfig::default());
    let mut counts = [0; 4];
    for seed in 0..512 {
        let mut sim = FullStateSimulator::new(1, 2, seed, Arc::clone(&noise));
        sim.h(0);
        sim.mz(0, 0);
        sim.mz(0, 1);
        // Readout faults consume noise draws without advancing the state RNG.
        sim.apply_readout_noise(0.5, 0.5, 0);
        sim.apply_readout_noise(0.5, 0.5, 1);
        let first = sim.measurements()[0] == MeasurementResult::One;
        let second = sim.measurements()[1] == MeasurementResult::One;
        counts[2 * usize::from(first) + usize::from(second)] += 1;
    }
    assert!(
        counts.iter().all(|count| (64..192).contains(count)),
        "{counts:?}"
    );
}
