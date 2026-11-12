// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CpuLinearAlgebra`]: the products, QR and SVD a
//! [`CpuMpsEngine`](crate::CpuMpsEngine) computes with.
//!
//! The engine hands it every matrix operation ([`LinearAlgebra`]): products to
//! apply gates and contract sites, a thin QR to move the orthogonality centre,
//! and a thin SVD to split a pair of sites after a gate.
//!
//! It computes in pure Rust, so the engine builds for every target the QDK
//! supports, WebAssembly included, and on the calling thread, since the caller
//! runs one state per core.
//!
//! The products and the QR are t4a-faer's, a fork of faer pinned to one
//! version. The SVD is the crate's own ([`svd`]), checked before a split takes
//! it; it uses t4a-faer's column-pivoted QR and, on large splits, its QR
//! algorithm as steps.

pub(crate) mod svd;

use self::svd::{checks::checked, qr_algorithm::tall_svd_by_qr_algorithm, trimmed_thin_svd};
use super::{FactorizationError, LinearAlgebra, Matrix, MatrixRef, Qr, Svd};
use num_complex::Complex64;
use t4a_faer::{Accum, MatMut, MatRef, Par, linalg::matmul::matmul};

/// [`LinearAlgebra`] on the CPU: t4a-faer's products and QR, sequential
/// (the crate builds t4a-faer without rayon), and the crate's checked SVD
/// (the [`svd`] module).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CpuLinearAlgebra;

fn to_faer(a: MatrixRef<'_>) -> MatRef<'_, Complex64> {
    MatRef::from_column_major_slice(a.elements(), a.rows(), a.cols())
}

fn from_faer(m: MatRef<'_, Complex64>) -> Matrix {
    Matrix::from_fn(m.nrows(), m.ncols(), |i, j| m[(i, j)])
}

impl LinearAlgebra for CpuLinearAlgebra {
    fn multiply(&self, a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
        let (rows, cols) = (a.rows(), b.cols());
        let mut elements = vec![Complex64::new(0.0, 0.0); rows * cols];
        matmul(
            MatMut::from_column_major_slice_mut(&mut elements, rows, cols),
            Accum::Replace,
            to_faer(a),
            to_faer(b),
            Complex64::new(1.0, 0.0),
            Par::Seq,
        );
        Matrix::new(rows, cols, elements)
    }

    fn qr(&self, a: MatrixRef<'_>) -> Result<Qr, FactorizationError> {
        let qr = to_faer(a).qr(Par::Seq);
        Ok(Qr {
            q: from_faer(qr.compute_thin_Q(Par::Seq).as_ref()),
            r: from_faer(qr.thin_R()),
        })
    }

    /// The thin SVD after a pivoted-QR trim, by the rotations or, on large
    /// splits far from orthogonal, by t4a-faer's QR algorithm, checked:
    /// see the [`svd`] module for what it guarantees and how.
    fn svd(&self, a: MatrixRef<'_>) -> Result<Svd, FactorizationError> {
        checked(a, trimmed_thin_svd(a, &tall_svd_by_qr_algorithm))
    }
}
