// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    MeasurementResult, Simulator, cpu_full_state_simulator::FullStateSimulator,
    noise_config::CumulativeNoiseConfig, stabilizer_simulator::StabilizerSimulator,
    stream_key::StreamKey,
};
use std::sync::Arc;

fn check_forced_loss<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    let mut simulator = S::new(
        2,
        4,
        StreamKey::session(0),
        Arc::new(CumulativeNoiseConfig::default()),
    );
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

const COINS: usize = 64;

/// Measures `COINS` qubits in uniform superposition. Each measurement is preceded by
/// `noise_draws` zero-probability readout faults, which consume noise draws without
/// changing anything.
fn coins<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>(
    key: StreamKey,
    noise_draws: usize,
) -> Vec<MeasurementResult> {
    let mut simulator = S::new(1, COINS, key, Arc::new(CumulativeNoiseConfig::default()));
    for result in 0..COINS {
        for _ in 0..noise_draws {
            simulator.apply_readout_noise(0.0, 0.0, result);
        }
        simulator.h(0);
        simulator.mresetz(0, result);
    }
    simulator.take_measurements()
}

/// Flips `COINS` recorded `Zero` results with 50% readout faults. Each fault is preceded by
/// `measurement_draws` measurements of a qubit in uniform superposition, which consume
/// measurement draws without affecting the recorded results.
fn faults<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>(
    key: StreamKey,
    measurement_draws: usize,
) -> Vec<MeasurementResult> {
    let mut simulator = S::new(
        2,
        COINS + 1,
        key,
        Arc::new(CumulativeNoiseConfig::default()),
    );
    for result in 0..COINS {
        for _ in 0..measurement_draws {
            simulator.h(1);
            simulator.mresetz(1, COINS);
        }
        simulator.mz(0, result);
        simulator.apply_readout_noise(0.5, 0.5, result);
    }
    let mut measurements = simulator.take_measurements();
    measurements.truncate(COINS);
    measurements
}

fn assert_random(outcomes: &[MeasurementResult]) {
    // Guards the invariance checks below against passing vacuously.
    assert!(
        outcomes.contains(&MeasurementResult::Zero) && outcomes.contains(&MeasurementResult::One),
        "{outcomes:?}"
    );
}

fn check_streams_are_independent<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>() {
    // These hold for every key; a few are checked so no particular value is special.
    for shot in 0..4 {
        let key = StreamKey::shot(0, shot);

        let measured = coins::<S>(key, 0);
        assert_random(&measured);
        assert_eq!(
            measured,
            coins::<S>(key, 3),
            "noise draws changed measurements"
        );

        let faulted = faults::<S>(key, 0);
        assert_random(&faulted);
        assert_eq!(
            faulted,
            faults::<S>(key, 3),
            "measurements changed noise draws"
        );
    }
}

#[test]
fn measurement_and_noise_streams_are_independent() {
    check_streams_are_independent::<StabilizerSimulator>();
    check_streams_are_independent::<FullStateSimulator>();
}
