// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine on the reference algebra runs the circuits a backend's tests
//! run (`engine_circuits`): it agrees with the exact reference on a long
//! circuit when nothing is truncated, with and without fusion, and stays
//! within the distance its truncations allow when they drop weight.

use super::*;
use crate::engine_circuits;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// The long circuit on the engine with and without fusion; a failure
/// names the fusion setting it failed on.
#[test]
fn a_long_circuit_agrees_with_the_reference_without_truncation() {
    for (engine, name) in both_fusions(0.0) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            engine_circuits::a_long_circuit_agrees_with_the_reference_without_truncation(&engine);
        }));
        if let Err(payload) = run {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("a panic without a message");
            panic!("{name}: {message}");
        }
    }
}

#[test]
fn a_truncated_circuit_stays_within_the_distance_its_splits_allow() {
    let engine = Engine::new(CpuAlgebra).with_cutoff(0.0);
    engine_circuits::a_truncated_circuit_stays_within_the_distance_its_splits_allow(&engine);
}
