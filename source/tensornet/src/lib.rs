// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The contract between a simulation method that runs matrix product states
//! (MPS) and the library that holds them, in tensor terms only. The method
//! asks an engine ([`MpsEngine`]) for a state within a capacity
//! ([`MpsShape`]). It applies unitaries to the state's sites, reads
//! probabilities, projects and forks the state, samples it from a seed, and
//! reads expectations of Pauli sums ([`PauliSum`]). The engine answers. It
//! refuses a call that breaks the contract the same way on every engine
//! ([`Refusal`]).
//!
//! A Bell pair on the reference (feature `conformance`), read four ways:
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
//! assert_eq!(engine.shape(&state)?.largest_bond_dimension(), 2);
//! # Ok(())
//! # }
//! ```
//!
//! Each item's documentation states its contract. The crate's `README.md`
//! gives the overview; its `docs/design.md` says why the crate is shaped as
//! it is.

// When a module or a public type is added, moved or renamed, update the
// README's "Files" table with it.
mod engine;
mod mps_shape;
mod pauli_sum;

pub use engine::checks;
#[cfg(any(test, feature = "conformance"))]
pub use engine::reference;
pub use engine::sampling;
pub use engine::{MpsEngine, MpsEngineError, Refusal};
pub use mps_shape::{MpsShape, MpsShapeError};
pub use pauli_sum::{Pauli, PauliSum, PauliSumError, PauliTerm};

#[cfg(test)]
mod tests;
