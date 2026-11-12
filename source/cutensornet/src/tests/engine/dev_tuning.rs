// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the tuning bench's knobs keep the
//! engine's values and release what they hold.

use super::{engine, gate_and_read, ok, uniform_capacity};
use crate::engine::{DevTuning, RESTART_PERIOD};
use tensornet::MpsEngine;

/// The values a long walk reads with `tuning`, the buffers it allocated
/// and took from the pool, and whether every native object was released.
fn walk(tuning: DevTuning) -> (Vec<f64>, u64, u64, bool) {
    let (engine, libraries) = engine();
    let engine = engine.with_dev_tuning(tuning);
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    let mut read = Vec::new();
    for step in 0..3 * RESTART_PERIOD {
        read.push(gate_and_read(&engine, &mut state, step));
        read.push(ok(engine.retained_weight(&state)));
    }
    let fork = ok(engine.fork(&state));
    read.push(ok(engine.probability_of_one(&fork, 1)));
    drop((state, fork));
    let times = engine.dev_times();
    let session_objects = libraries.live_objects();
    drop(engine);
    let released = libraries.live_objects() == 0 && session_objects <= 2;
    (read, times.allocations, times.pool_hits, released)
}

/// Pooled buffers read the same values, reuse buffers instead of
/// allocating, and are all freed when the state is dropped.
#[test]
fn pooled_buffers_change_no_value_and_are_released() {
    let (plain, plain_allocations, plain_hits, plain_released) = walk(DevTuning::default());
    let (pooled, pooled_allocations, pooled_hits, pooled_released) = walk(DevTuning {
        pooled_buffers: true,
        ..DevTuning::default()
    });
    assert_eq!(pooled, plain);
    assert_eq!(plain_hits, 0);
    assert!(pooled_hits > 0, "the pool is used");
    assert!(pooled_allocations < plain_allocations, "fewer allocations");
    assert!(plain_released && pooled_released, "everything is released");
}

/// The hyper-samples of every read are the tuning's.
#[test]
fn reads_use_the_tunings_hyper_samples() {
    let (engine, libraries) = engine();
    let engine = engine.with_dev_tuning(DevTuning {
        expectation_hyper_samples: 1,
        ..DevTuning::default()
    });
    let mut state = ok(engine.prepare(&uniform_capacity(3, 2)));
    gate_and_read(&engine, &mut state, 0);
    assert_eq!(libraries.hyper_samples(), [1]);
}
