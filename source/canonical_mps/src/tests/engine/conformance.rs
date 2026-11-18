// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine on the reference algebra runs the circuits a backend's tests
//! run (`engine::conformance`): it agrees with the exact reference on a
//! long circuit when nothing is truncated, with and without fusion, and
//! stays within the distance its truncations allow when they drop weight.

use super::*;
use crate::engine::conformance;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// The untruncated circuit on the engine with and without fusion; a
/// failure names the fusion setting it failed on.
#[test]
fn an_untruncated_circuit_agrees_with_the_exact_reference() {
    for (engine, name) in both_fusions(0.0) {
        let run = catch_unwind(AssertUnwindSafe(|| {
            conformance::assert_untruncated_circuit_agrees_on(&engine);
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

/// The truncated circuit, called as a backend's test calls it.
#[test]
fn a_truncated_circuit_stays_within_its_truncation_bound() {
    conformance::assert_truncated_circuit_stays_within_its_truncation_bound(CpuAlgebra);
}
