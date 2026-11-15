// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A matrix product state (MPS) engine in canonical form, written once over
//! the tensor operations of a backend, so that each backend (the CPU, a GPU
//! library) writes only those operations, and every backend runs the same
//! rules: where the orthogonality centre moves, how a two-site gate is split
//! and truncated, and how the retained weight is kept.
//!
//! - [`algebra`]: the backend contract. [`SiteAlgebra`](algebra::SiteAlgebra)
//!   is what a backend implements: a site tensor's storage, a gate split, a
//!   move of the centre, and a host product and SVD. Its call and outcome
//!   types state the tensors' layout, and [`algebra::truncation`] gives host
//!   backends the rule for which singular values a split keeps.
//! - The engine, which implements `tensornet`'s `MpsEngine` over any
//!   `SiteAlgebra`, is planned: it is not in this crate yet.
//!
//! It depends only on `num-complex` and `dense_linalg`, so it builds for
//! every target the QDK supports, WebAssembly included. Each item's
//! documentation states its contract; the crate's `README.md` gives an
//! overview for backend writers.

pub mod algebra;

#[cfg(test)]
mod tests;
