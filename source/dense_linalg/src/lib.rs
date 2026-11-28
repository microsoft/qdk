// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Dense complex matrices with a checked thin singular value decomposition
//! (SVD), and the product, thin QR and LQ decompositions, adjoint and norms
//! that go with it.
//!
//! General SVDs have returned factors that do not give back the matrix,
//! without reporting a failure, on rank-deficient matrices with repeated
//! singular values. A caller that truncates a matrix by its singular values
//! then truncates it wrongly, silently. [`Svd::of`] either answers factors
//! checked against the matrix, to the bounds of LAPACK's SVD tests, or
//! reports a failure ([`SvdError`]). It leaves out only singular values of
//! rounding's size.
//!
//! - [`Matrix`], [`MatrixRef`]: a dense complex matrix, owned or borrowed,
//!   column-major, with its product ([`Matrix::product`]), adjoint
//!   ([`MatrixRef::adjoint`]), and columns or rows multiplied by values
//!   ([`Matrix::multiply_columns_by`], [`Matrix::multiply_rows_by`]).
//! - [`norm`], [`squared_norm`], [`norm_divided_by`]: the norm of a vector,
//!   or the Frobenius norm of a matrix, from its elements.
//! - [`Qr`] and [`Lq`]: the thin QR and LQ decompositions, [`Qr::of`] and
//!   [`Lq::of`].
//! - [`Svd`]: the checked thin SVD, [`Svd::of`], its truncation
//!   ([`Svd::truncated`]), and [`SvdError`], why it failed.
//!
//! Each computes on the calling thread, in pure Rust, so the crate builds
//! for WebAssembly too.
//!
//! The SVD of a matrix of rank 1, whose one singular value is 5:
//!
//! ```
//! use dense_linalg::{Matrix, Svd};
//! use num_complex::Complex64;
//!
//! // (3, 4)ᵀ·(1, 0, 0): column-major, one column after the other.
//! let x = |re| Complex64::new(re, 0.0);
//! let zero = x(0.0);
//! let a = Matrix::new(2, 3, vec![x(3.0), x(4.0), zero, zero, zero, zero]);
//! let svd = Svd::of(a.view()).expect("a rank-1 matrix passes the checks");
//! assert_eq!(svd.singular_values.len(), 1);
//! assert!((svd.singular_values[0] - 5.0).abs() < 1e-14);
//! ```
//!
//! The crate's `docs/algorithms.md` gives the SVD's steps and the kernels.
//! Its Terms explain, with diagrams and formulas, each word of numerical
//! linear algebra the crate's files use (column-major, adjoint, norm,
//! orthonormal, SVD, truncation, QR, LQ, pivot, reflector, rotation, Gram
//! matrix, rounding, subnormal, cancellation, conditioning), for a reader
//! new to it.
//!
//! The crate's parts:
//!
//! ```text
//! lib.rs               the crate's types, re-exported
//!   matrix.rs          Matrix, MatrixRef: a dense complex matrix, owned or
//!                      borrowed, column-major, its product and adjoint,
//!                      its columns or rows multiplied by values
//!   norms.rs           norm, squared_norm, norm_divided_by: the norm of
//!                      a vector or matrix from its elements
//!   qr.rs              Qr: the thin QR decomposition
//!   lq.rs              Lq: the thin LQ decomposition, through the QR
//!   svd.rs             Svd: the checked thin SVD, its steps, its
//!                      truncation; SvdError
//!     checks.rs        the checks every answer passes before it returns
//!     jacobi.rs        the one-sided Jacobi rotations: the sweeps, and the
//!                      order they visit the pairs of columns in
//!       rotation.rs    the turn that makes one pair of columns orthogonal,
//!                      with a worked example
//!       normed_columns.rs
//!                      the columns with their squared norms, and when a
//!                      pair counts as orthogonal
//!       split_complex_columns.rs
//!                      the columns' storage, real and imaginary parts
//!                      apart, so that their loops vectorize
//!       settled_pairs.rs
//!                      the pairs found orthogonal since either column
//!                      last turned, which a sweep skips
//!   scaling.rs         PowerOfTwoScaling: the power of two that brings a
//!                      matrix's largest entry near 1
//!   kernels.rs         Kernels: the product and the QRs, behind one trait
//!     matrixmultiply_householder.rs
//!                      the Kernels on matrixmultiply's product and the
//!                      crate's own QRs
//!       householder_reflectors.rs
//!                      a QR's Q as Householder reflectors, and the thin
//!                      and the column-pivoted QR that make it
//! ```

pub(crate) mod kernels;
mod lq;
mod matrix;
mod norms;
mod qr;
mod scaling;
pub(crate) mod svd;

pub use self::lq::Lq;
pub use self::matrix::{Matrix, MatrixRef};
pub use self::norms::{norm, norm_divided_by, squared_norm};
pub use self::qr::Qr;
pub use self::svd::{Svd, SvdError};

#[cfg(test)]
mod test_utils;
