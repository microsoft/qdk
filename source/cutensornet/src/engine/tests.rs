// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the engine's GPU code, without a GPU, on fake libraries that
//! simulate the native API on the CPU
//! (`src/engine/native_calls/fake_libraries.rs`):
//!
//! - the CUDA algebra, with the device session it opens and its cleanups,
//!   through the algebra's methods (`cuda_algebra/tests.rs`, and a
//!   `tests.rs` beside each of its parts that is tested directly);
//! - the native calls' own code and the fake's rules
//!   (`native_calls/tests.rs`);
//! - the engine on the algebra, here: `canonical_mps`'s circuits run on
//!   the fake, so that CI checks the calls a real engine makes against the
//!   fake's rules. The engine itself is `canonical_mps`'s, tested there.
//!   The same circuits run on a GPU (`tests/cuda_mps_engine_on_the_gpu.rs`).

use crate::engine::{
    CudaAlgebra,
    native_calls::{NativeCalls, fake_libraries::FakeLibraries},
};
use canonical_mps::engine_conformance;
use std::sync::Arc;

/// An algebra on fake libraries, and the libraries, to check after a
/// circuit that it released every native object.
pub(super) fn algebra_on_the_fake() -> (CudaAlgebra, Arc<FakeLibraries>) {
    let libraries = Arc::new(FakeLibraries::new());
    (algebra_on(&libraries), libraries)
}

/// An algebra on `libraries`, which may hold other algebras.
pub(super) fn algebra_on(libraries: &Arc<FakeLibraries>) -> CudaAlgebra {
    let native_calls: Arc<dyn NativeCalls> = libraries.clone();
    CudaAlgebra::with_native_calls(native_calls)
        .unwrap_or_else(|error| panic!("the fake libraries open an algebra: {error}"))
}

/// `canonical_mps`'s circuits, untruncated and truncated, read on the
/// engine over the fake as the exact reference does, within their bounds:
/// every call the engine makes follows the fake's rules on streams,
/// copies, workspaces and prepared networks. Each circuit releases every
/// native object once its engine is dropped.
#[test]
fn the_engines_circuits_run_on_the_fake_libraries() {
    let circuits = [
        (
            "untruncated",
            engine_conformance::assert_untruncated_circuit_agrees_with_exact_reference
                as fn(CudaAlgebra),
        ),
        (
            "truncated",
            engine_conformance::assert_truncated_circuit_stays_within_its_truncation_bound,
        ),
    ];
    for (circuit, run) in circuits {
        let (algebra, libraries) = algebra_on_the_fake();
        run(algebra);
        assert_eq!(libraries.live_objects(), 0, "{circuit}: leaked");
    }
}
