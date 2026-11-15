// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A matrix product state (MPS) engine in canonical form, to be written
//! once over the tensor operations of a backend, so that each backend (the
//! CPU, a GPU library) writes only those operations, and every backend
//! runs the same rules: where the orthogonality centre moves, which pair a
//! gate splits, how the retained weight is kept, and the bounds every
//! truncation stays within.
//!
//! - [`algebra`]: the backend contract. [`SiteAlgebra`](algebra::SiteAlgebra)
//!   is what a backend implements: a site tensor's storage, a gate split,
//!   and a move of the centre. [`algebra`]'s documentation states the
//!   tensors' layout, and [`algebra::truncation`] holds a rule of
//!   truncation that a backend which computes the singular values itself
//!   may use.
//! - The engine, which will implement `tensornet`'s `MpsEngine` over any
//!   `SiteAlgebra`, is planned: it is not in this crate yet.
//!
//! It depends only on `num-complex`, so it builds on every platform the
//! QDK supports (the `README.md`'s Platforms says which are tested). Each
//! item's documentation states its contract; the crate's `README.md`
//! gives an overview for backend writers.

pub mod algebra;

#[cfg(test)]
mod tests;
