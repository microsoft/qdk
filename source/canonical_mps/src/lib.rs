// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A matrix product state (MPS) engine in canonical form, written once
//! over the tensor operations of a backend, so that each backend (the CPU,
//! a GPU library) writes only those operations, and every backend fuses,
//! applies and reads by the same rules, and truncates within the same
//! bounds.
//!
//! - [`CanonicalMpsEngine`] is `tensornet`'s
//!   [`MpsEngine`](tensornet::MpsEngine) on any
//!   [`SiteAlgebra`](algebra::SiteAlgebra), with its state
//!   ([`CanonicalMpsState`]), its errors ([`CanonicalMpsError`]) and what
//!   its splits cost ([`SplitStatistics`]). It documents what a caller may
//!   rely on beyond the contract: how it fuses and applies gates, and the bounds
//!   its truncations stay within.
//! - [`CpuMpsEngine`] is the engine on [`CpuAlgebra`], the reference
//!   algebra: the `SiteAlgebra` of the host CPU, whose answers a backend
//!   on another library can be compared with. It needs no device, so it
//!   runs wherever the QDK runs.
//! - [`algebra`] is the backend contract.
//!   [`SiteAlgebra`](algebra::SiteAlgebra) is what a backend implements: a
//!   site tensor's storage, a gate split, and a move of the centre.
//!   [`algebra`]'s documentation states the tensors' layout.
//!
//! The GHZ state on three qubits, its last qubit reached from the first by
//! a gate on sites that are not neighbours:
//!
//! ```
//! use canonical_mps::{CpuAlgebra, CpuMpsEngine};
//! use num_complex::Complex64;
//! use tensornet::{MpsEngine, MpsShape};
//!
//! let c = |re: f64| Complex64::new(re, 0.0);
//! let h = std::f64::consts::FRAC_1_SQRT_2;
//! let hadamard = [c(h), c(h), c(h), c(-h)];
//! let mut cx = [c(0.0); 16];
//! for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
//!     cx[4 * row + col] = c(1.0);
//! }
//!
//! let engine = CpuMpsEngine::new(CpuAlgebra);
//! let capacity = MpsShape::qubit_chain(3, 2).expect("three sites");
//! let mut ghz = engine.prepare(&capacity).expect("a chain of qubits");
//! engine.apply(&mut ghz, &[0], &hadamard).expect("a one-site gate");
//! engine.apply(&mut ghz, &[0, 1], &cx).expect("neighbours");
//! engine.apply(&mut ghz, &[0, 2], &cx).expect("sites further apart");
//!
//! let p = engine.probability_of_one(&ghz, 2).expect("a site in range");
//! assert!((p - 0.5).abs() < 1e-12);
//! let given = engine
//!     .conditional_probability_of_one(&ghz, 2, &[(0, true)])
//!     .expect("a possible condition");
//! assert!((given - 1.0).abs() < 1e-12);
//! ```
//!
//! It depends on `tensornet` (the contract), `dense_linalg` (the reference
//! algebra's product, QR and SVD, and the expectation's products on the
//! host), `num-complex` and `thiserror`, all pure Rust, so it builds on
//! every platform the QDK supports (the `README.md`'s Platforms says which
//! are tested). Each item's documentation states its contract; the crate's
//! `README.md` gives an overview, `docs/engine.md` the backend calls and
//! the numerics of each method, and `docs/distant_sites.md` how gates on
//! distant sites are applied, with the alternatives considered.

pub mod algebra;
mod engine;

pub use algebra::CpuAlgebra;
pub use engine::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, DEFAULT_CUTOFF, SplitStatistics,
};

/// The canonical MPS engine on the host CPU: [`CanonicalMpsEngine`] on the
/// reference algebra, [`CpuAlgebra`]. `CpuMpsEngine::new(CpuAlgebra)` makes
/// it, at the default cutoff ([`CanonicalMpsEngine::new`]). Its states are
/// [`CanonicalMpsState<CpuAlgebra>`], and its errors
/// [`CanonicalMpsError<SvdError>`](CanonicalMpsError), with `dense_linalg`'s
/// `SvdError`.
pub type CpuMpsEngine = CanonicalMpsEngine<CpuAlgebra>;

#[cfg(test)]
mod tests;
