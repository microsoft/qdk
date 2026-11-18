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
//!   [`SiteAlgebra`](site_algebra::SiteAlgebra), with its state
//!   ([`CanonicalMpsState`]), its errors ([`CanonicalMpsError`]) and what
//!   a state's history cost ([`CanonicalMpsStatistics`]). It documents
//!   what a caller may rely on beyond the contract: how it fuses and
//!   applies gates, and the bounds its truncations stay within.
//! - [`CpuMpsEngine`] is the engine on [`CpuAlgebra`], the reference
//!   algebra: the `SiteAlgebra` of the host CPU, which passes the
//!   contract's conformance kit. It needs no device, so it runs wherever
//!   the QDK runs.
//! - [`site_algebra`] is the backend contract.
//!   [`SiteAlgebra`](site_algebra::SiteAlgebra) is what a backend implements: a
//!   site tensor's storage, a gate split, and a move of the centre.
//!   [`site_algebra`]'s documentation states the tensors' layout. Its
//!   conformance kit, `site_algebra::conformance`, which the `conformance`
//!   feature makes public, checks a backend against the contract, call by
//!   call; `engine_conformance`, under the same feature, runs the engine on
//!   the backend over two long circuits, checked against `tensornet`'s
//!   exact reference.
//! - [`SvdError`] is why the crate's checked singular value decomposition
//!   failed: `CpuAlgebra`'s error. The SVD, with the products and QR
//!   decompositions the reference algebra and the engine's host
//!   computations need, is the crate's private dense linear algebra,
//!   which the `conformance` feature makes public (`dense_linalg`).
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
//! The `README.md`'s Platforms says where it builds and which platforms
//! are tested. Each item's documentation states its contract; the crate's
//! `README.md` gives an overview, `docs/engine.md` the backend calls and
//! the numerics of each method, `docs/distant_sites.md` how gates on
//! distant sites are applied, with the alternatives considered, and
//! `docs/dense_linalg.md` the SVD's guarantees and steps.

#[cfg(feature = "conformance")]
pub mod dense_linalg;
#[cfg(not(feature = "conformance"))]
mod dense_linalg;
mod engine;
pub mod site_algebra;

pub use dense_linalg::SvdError;
#[cfg(feature = "conformance")]
pub use engine::conformance as engine_conformance;
pub use engine::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, CanonicalMpsStatistics,
    DEFAULT_CUTOFF,
};
pub use site_algebra::cpu_algebra::CpuAlgebra;

/// The canonical MPS engine on the host CPU: [`CanonicalMpsEngine`] on the
/// reference algebra, [`CpuAlgebra`]. `CpuMpsEngine::new(CpuAlgebra)` makes
/// it, at the default cutoff ([`CanonicalMpsEngine::new`]). Its states are
/// [`CanonicalMpsState<CpuAlgebra>`], and its errors
/// [`CanonicalMpsError<SvdError>`](CanonicalMpsError).
pub type CpuMpsEngine = CanonicalMpsEngine<CpuAlgebra>;

#[cfg(test)]
mod tests;
