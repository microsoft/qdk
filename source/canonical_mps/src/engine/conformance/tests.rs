// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine on the CPU algebra runs the circuits a backend's tests
//! run (`engine::conformance`): it agrees with the exact reference on a
//! long circuit when nothing is truncated, with and without fusion, and
//! stays within its truncation bound when its splits drop weight.

use crate::engine::conformance;
use crate::{CpuAlgebra, CpuMpsEngine as Engine};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// The untruncated circuit, called as a backend's test calls it (the
/// engine fused, at a cutoff of 0), then on the same engine unfused; a
/// failure names the fusion setting it failed on.
#[test]
fn an_untruncated_circuit_agrees_with_the_exact_reference() {
    let runs: [(&str, &dyn Fn()); 2] = [
        ("fused", &|| {
            conformance::assert_untruncated_circuit_agrees_with_exact_reference(CpuAlgebra);
        }),
        ("unfused", &|| {
            let engine = Engine::new(CpuAlgebra).with_cutoff(0.0);
            conformance::assert_untruncated_circuit_agrees_on(&engine.with_fusion(false));
        }),
    ];
    for (name, run) in runs {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(run)) {
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
