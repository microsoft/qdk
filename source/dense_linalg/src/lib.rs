// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dense complex matrices with a checked thin singular value decomposition
//! (SVD), with the product and thin QR decomposition MPS simulators also
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
//! Each computes on the calling thread, in pure Rust, on every target the
//! QDK supports, WebAssembly included.
//!
//! The SVD of a matrix of rank 1, whose one singular value is 5:
//!
//! ```
//! # fn main() -> Result<(), dense_linalg::SvdError> {
//! use dense_linalg::{Matrix, Svd};
//! use num_complex::Complex64;
//!
//! // (3, 4)ᵀ·(1, 0, 0): column-major, one column after the other.
//! let x = |re| Complex64::new(re, 0.0);
//! let a = Matrix::new(2, 3, vec![x(3.0), x(4.0), x(0.0), x(0.0), x(0.0), x(0.0)]);
//! let svd = Svd::of(a.view())?;
//! assert_eq!(svd.singular_values.len(), 1);
//! assert!((svd.singular_values[0] - 5.0).abs() < 1e-14);
//! # Ok(())
//! # }
//! ```

mod kernels;
mod matrix;
mod qr;
mod svd;

#[cfg(test)]
mod tests;

pub use self::matrix::{Matrix, MatrixRef};
pub use self::qr::Qr;
pub use self::svd::{Svd, SvdError};
