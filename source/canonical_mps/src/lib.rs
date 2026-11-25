// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A matrix product state (MPS) engine in canonical form, written once
//! over the tensor operations of a backend. Each backend (the CPU, a GPU
//! library) writes only those operations. Every backend then fuses,
//! applies and reads by the same rules, and truncates within the same
//! bounds.
//!
//! - [`CanonicalMpsEngine`] is `tensornet`'s
//!   [`MpsEngine`](tensornet::MpsEngine) on any
//!   [`SiteAlgebra`](site_algebra::SiteAlgebra). It comes with its state
//!   ([`CanonicalMpsState`]), its errors ([`CanonicalMpsError`]) and what
//!   a state's history cost ([`CanonicalMpsStatistics`]). It documents
//!   what a caller may rely on beyond the contract: the bounds its
//!   truncations stay within, how it defers gates, and how it applies a
//!   gate to sites far apart.
//! - [`CpuMpsEngine`] is the engine on [`CpuAlgebra`], the `SiteAlgebra`
//!   of the host CPU, which passes the contract's conformance kit.
//! - [`site_algebra`] is the backend contract.
//!   [`SiteAlgebra`](site_algebra::SiteAlgebra) is what a backend
//!   implements: a site tensor's storage, a gate split, and a move of the
//!   centre. [`site_algebra`]'s documentation states the tensors' layout.
//!   The `conformance` feature makes public two checks of a backend. The
//!   kit, `site_algebra::conformance`, checks it against the contract,
//!   call by call. `engine_conformance` runs the engine on it over two
//!   long circuits, checked against `tensornet`'s exact reference.
//! - [`SvdError`], re-exported from the `dense_linalg` crate, says why its
//!   checked singular value decomposition failed ([`CpuAlgebra`] says
//!   when). That crate holds the products, QR and LQ decompositions, and
//!   SVD the CPU algebra and the engine's host computations need.
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
//! Each item's documentation states its contract. The crate's documents
//! give the rest:
//!
//! - `README.md`: an overview, and in its Platforms, where the crate
//!   builds and which platforms are tested;
//! - `docs/engine.md`: how the engine implements each method, its backend
//!   calls, how it applies gates on distant sites (and why not by routing
//!   them together by SWAPs), and its numerics;
//! - `docs/site_algebra.md`: the backend contract's layout and formulas,
//!   and how the conformance kit checks them.

mod cpu_algebra;
mod engine;
pub mod site_algebra;

pub use cpu_algebra::CpuAlgebra;
pub use dense_linalg::SvdError;
#[cfg(feature = "conformance")]
pub use engine::conformance as engine_conformance;
pub use engine::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, CanonicalMpsStatistics,
    DEFAULT_CUTOFF,
};

/// The canonical MPS engine on the host CPU: [`CanonicalMpsEngine`] on the
/// CPU algebra, [`CpuAlgebra`]. `CpuMpsEngine::new(CpuAlgebra)` makes it,
/// at the default cutoff ([`CanonicalMpsEngine::new`]). Its states are
/// [`CanonicalMpsState<CpuAlgebra>`]. Its errors are
/// [`CanonicalMpsError<SvdError>`](CanonicalMpsError).
pub type CpuMpsEngine = CanonicalMpsEngine<CpuAlgebra>;
