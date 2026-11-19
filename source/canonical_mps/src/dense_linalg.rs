// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dense complex matrices with a checked thin singular value decomposition
//! (SVD), with the product, thin QR decomposition and adjoint that the
//! CPU algebra ([`CpuAlgebra`](crate::CpuAlgebra)) and the engine's
//! host computations (the MPO of a gate, the expectation's contractions)
//! need.
//!
//! General SVDs have returned factors that do not give back the matrix,
//! without reporting a failure, on rank-deficient matrices with repeated
//! singular values. [`Svd::of`] either answers factors checked against the
//! matrix, to the bounds of LAPACK's SVD tests, or reports a failure
//! ([`SvdError`]); it leaves out only singular values of rounding's size.
//!
//! - [`Matrix`], [`MatrixRef`]: a dense complex matrix, owned or borrowed,
//!   column-major, with its product ([`Matrix::product`]) and adjoint
//!   ([`MatrixRef::adjoint`]).
//! - [`Qr`]: the thin QR decomposition, [`Qr::of`].
//! - [`Svd`]: the checked thin SVD, [`Svd::of`], and [`SvdError`], why it
//!   failed.
//!
//! The module is private: only [`SvdError`] is public, at the crate's root,
//! because it is `CpuAlgebra`'s error and the source of an engine error.
//! The `conformance` feature makes the module public, for another crate's
//! test fakes that compute as a backend library would (a fake of a GPU
//! library's SVD and QR).
//!
//! Each computes on the calling thread. `docs/dense_linalg.md` gives the
//! SVD's guarantees and steps.

pub(crate) mod kernels;
mod matrix;
mod qr;
mod scaling;
pub(crate) mod svd;

pub use self::matrix::{Matrix, MatrixRef};
pub use self::qr::Qr;
pub use self::svd::{Svd, SvdError};
