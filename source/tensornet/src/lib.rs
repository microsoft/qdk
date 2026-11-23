// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The contract between a simulation method that runs matrix product states
//! (MPS) and the library that holds them, in tensor terms only: the method
//! asks an engine ([`MpsEngine`]) for a state within a capacity
//! ([`MpsShape`]), applies unitaries to its sites, reads probabilities,
//! projects and forks it, samples it from a seed, and reads expectations of
//! Pauli sums ([`PauliSum`]); the engine answers, and refuses a call that
//! breaks the contract the same way on every engine ([`Refusal`]).
//!
//! Today the crate holds:
//!
//! - the engine: [`MpsEngine`], what a library that holds matrix product
//!   states does for a simulation method; [`MpsEngineError`], which an
//!   engine's error implements to tell a refusal from its own failure;
//!   [`checks`], which makes each refusal of the arguments once, for every
//!   engine; [`sampling`], an engine's `sample` from its conditional
//!   probabilities, written once for every engine; and
//!   `reference::ExactReference`, the exact engine every other one is
//!   checked against, for an engine's tests (feature `conformance`);
//! - the chain's shape: [`MpsShape`], the shape of a chain of site tensors
//!   by the extents of its sites alone (the capacity a caller asks for, and
//!   the chain an engine holds, which is what the run pays for), and
//!   [`MpsShapeError`];
//! - the observables: [`PauliSum`], of [`PauliTerm`]s of [`Pauli`] factors,
//!   on sites, and [`PauliSumError`];
//! - the refusals: [`Refusal`], why an engine refused a call: its arguments
//!   break the contract, or name an outcome of probability zero.
//!
//! A Bell pair on the reference (feature `conformance`), read three ways:
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use num_complex::Complex64;
//! use tensornet::reference::ExactReference;
//! use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};
//!
//! let (o, l, h) = (0.0, 1.0, std::f64::consts::FRAC_1_SQRT_2);
//! let c = |re| Complex64::new(re, 0.0);
//! let hadamard = [c(h), c(h), c(h), c(-h)];
//! // Row-major, `M[out][in]`, `sites[0]` the most significant bit.
//! let cx = [l, o, o, o, o, l, o, o, o, o, o, l, o, o, l, o].map(c);
//!
//! let engine = ExactReference::new();
//! let capacity = MpsShape::qubit_chain(2, 2).expect("two sites form a chain");
//! let mut state = engine.prepare(&capacity)?;
//! engine.apply(&mut state, &[0], &hadamard)?;
//! engine.apply(&mut state, &[0, 1], &cx)?;
//!
//! let p1 = engine.probability_of_one(&state, 1)?;
//! assert!((p1 - 0.5).abs() < 1e-12);
//!
//! let zz = PauliTerm::new(c(1.0), vec![(0, Pauli::Z), (1, Pauli::Z)])?;
//! let zz = PauliSum::new(c(0.0), vec![zz])?;
//! assert!((engine.expectation(&state, &zz)? - c(1.0)).norm() < 1e-12);
//!
//! let values = engine.sample(&state, &[0, 1], 100, 7)?;
//! assert!(values.chunks_exact(2).all(|shot| shot[0] == shot[1]));
//!
//! assert_eq!(engine.shape(&state)?.largest_bond(), 2);
//! # Ok(())
//! # }
//! ```
//!
//! It depends only on `thiserror` and `num-complex`, so it builds for every
//! target the QDK supports, WebAssembly included. Each item's documentation
//! states its contract; the crate's `README.md` gives an overview, and
//! `design.md` says why the crate is shaped as it is.

// When a module or a public type is added, moved or renamed, update the
// README's "Files" table with it.
mod engine;
mod mps_shape;
mod pauli_sum;
mod refusal;

pub use engine::checks;
#[cfg(any(test, feature = "conformance"))]
pub use engine::reference;
pub use engine::sampling;
pub use engine::{MpsEngine, MpsEngineError};
pub use mps_shape::{MpsShape, MpsShapeError};
pub use pauli_sum::{Pauli, PauliSum, PauliSumError, PauliTerm};
pub use refusal::Refusal;

#[cfg(test)]
mod tests;
