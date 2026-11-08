// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A matrix product state engine on the CPU: [`CpuMpsEngine`], an
//! implementation of `tensornet`'s [`MpsEngine`](tensornet::MpsEngine)
//! contract.
//!
//! A simulator holds the state of a program's qubits while it runs the program.
//! Held whole, the state of `n` qubits is `2ⁿ` complex amplitudes, so its
//! memory doubles with each qubit. A matrix product state (MPS) holds it
//! instead as a chain of small tensors, one per qubit, each joined to its
//! neighbours by a bond: the bond a cut of the chain needs grows with the
//! entanglement between the qubits on its two sides, so a state of little
//! entanglement is small. A simulator that runs programs this way is written
//! against `tensornet`'s contract, and runs on any engine that implements it.
//! This crate is the engine for the CPU, on every target the QDK supports,
//! WebAssembly included, since its linear algebra, t4a-faer, is pure Rust.
//!
//! - [`CpuMpsEngine`]: the engine. A caller prepares a state, applies one- and
//!   two-site gates, and reads probabilities, projections, expectations and
//!   samples. Beyond the contract, it states how it keeps the state small: how
//!   it truncates the state (to the largest bond the caller allows, `χ`, and to
//!   a cutoff, [`DEFAULT_CUTOFF`] unless the caller sets another), the weight
//!   truncation discards, and how it applies a gate to two qubits that are not
//!   neighbours in the chain.
//! - [`CanonicalMps`]: a state, one per shot, and [`SplitStatistics`], what
//!   keeping it small cost.
//! - [`CpuMpsError`]: a call the contract refuses, or one the engine failed;
//!   [`FactorizationError`] says why an SVD failed.
//!
//! A caller may rely on the contract, checked by its conformance kit against
//! the exact reference, and on the rules stated on [`CpuMpsEngine`]. A gate's
//! cost depends on its sites, the bonds there and how far the engine moves
//! along the chain to reach them, not on the chain's length. The engine takes
//! an SVD only after checking its factors, so a wrong one fails the call
//! ([`CpuMpsError::Factorization`]), and the caller drops the state, rather
//! than letting the run go on from a wrong state.
//!
//! The engine moves along the chain only as far as an operation needs, by one
//! QR decomposition per site it crosses, and splits two sites after a gate by
//! one SVD, plus one per SWAP when it brings two distant sites together (with
//! fusion, consecutive gates on one pair share one). Its products and QR are
//! t4a-faer's; its SVD is the crate's own. The crate's `README.md` explains the
//! choices.
//!
//! ```
//! use num_complex::Complex64;
//! use qdk_cpu_mps::CpuMpsEngine;
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
//! let engine = CpuMpsEngine::default();
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

mod chain;
mod engine;
mod error;
mod linear_algebra;

pub use chain::{CanonicalMps, SplitStatistics};
pub use engine::{CpuMpsEngine, DEFAULT_CUTOFF};
pub use error::CpuMpsError;
pub use linear_algebra::FactorizationError;

#[cfg(test)]
mod tests;
