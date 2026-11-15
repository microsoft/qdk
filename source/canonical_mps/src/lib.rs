// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A matrix product state (MPS) engine in canonical form, written once
//! over the tensor operations of a backend, so that each backend (the CPU,
//! a GPU library) writes only those operations, and every backend routes
//! and reads by the same rules, and truncates within the same bounds.
//!
//! - [`CanonicalMpsEngine`] is `tensornet`'s
//!   [`MpsEngine`](tensornet::MpsEngine) on any
//!   [`SiteAlgebra`](algebra::SiteAlgebra), with its state
//!   ([`CanonicalMpsState`]), its errors ([`CanonicalMpsError`]) and what
//!   its splits cost ([`SplitStatistics`]). It documents what a caller may
//!   rely on beyond the contract: how it routes and fuses, and the bounds
//!   its truncations stay within.
//! - [`algebra`] is the backend contract.
//!   [`SiteAlgebra`](algebra::SiteAlgebra) is what a backend implements: a
//!   site tensor's storage, a gate split, and a move of the centre.
//!   [`algebra`]'s documentation states the tensors' layout.
//! - [`CpuAlgebra`]: the reference algebra, the `SiteAlgebra` of the host
//!   CPU, whose answers a backend on another library is compared with.
//!
//! It depends on `tensornet` (the contract), `dense_linalg` (the reference
//! algebra's product, QR and SVD, and the expectation's products on the
//! host), `num-complex` and `thiserror`, all pure Rust, so it builds on
//! every platform the QDK supports (the `README.md`'s Platforms says which
//! are tested). Each item's documentation states its contract; the crate's
//! `README.md` gives an overview, `docs/engine.md` the backend calls and
//! the numerics of each method, and `docs/routing.md` compares the routings
//! considered.

pub mod algebra;
mod engine;

pub use engine::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, DEFAULT_CUTOFF, SplitStatistics,
};

pub use algebra::CpuAlgebra;

#[cfg(test)]
mod tests;
