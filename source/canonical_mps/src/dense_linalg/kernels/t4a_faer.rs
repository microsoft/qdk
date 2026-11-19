// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`T4aFaer`]: the [`Kernels`] on t4a-faer, a fork of faer pinned to one
//! version, sequential (the crate builds it without rayon).
//!
//! The [`Kernels`]' steps set the module's speed, which is why they come
//! from t4a-faer: with it, the SVD alone is 1.6 to 2.1 times as fast as with
//! nalgebra 0.35, the dense algebra the QDK's simulators already use, and
//! an MPS simulation 1.6 to 1.8 times as fast end to end (H₂'s phase
//! estimation shots at a bond of 64, on one core: 82 s against 129 s), with
//! the same results.
//!
//! Its fast route ([`T4aFaer::tall_svd`]) is t4a-faer's bidiagonalization
//! and QR algorithm: it reduces `W` by reflections to a bidiagonal matrix,
//! nonzero only on its diagonal and the one above, and finds that matrix's
//! singular values by iteration. It runs the QR algorithm at every size,
//! never t4a-faer's default, divide and conquer from size 128, which has
//! returned factors that the checks refuse where the QR algorithm's pass
//! ([`crate::dense_linalg::svd`]'s header gives the case).

use super::{ColumnMajorSvd, Kernels, PivotedQr};
use crate::dense_linalg::matrix::{Matrix, MatrixRef};
use crate::dense_linalg::qr::Qr;
use num_complex::Complex64;
use t4a_faer::{
    Accum, Auto, Conj, MatMut, MatRef, Par, ParShape,
    diag::Diag,
    dyn_stack::{MemBuffer, MemStack},
    linalg::{
        householder::{
            apply_block_householder_sequence_on_the_left_in_place_scratch,
            apply_block_householder_sequence_on_the_left_in_place_with_conj,
        },
        matmul::matmul,
        solvers::ColPivQr,
        svd::{ComputeSvdVectors, SvdParams, svd, svd_scratch},
    },
};

/// The [`Kernels`] on t4a-faer.
pub(crate) struct T4aFaer;

impl Kernels for T4aFaer {
    type Q = ColPivQr<Complex64>;

    fn product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
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

    fn qr(a: MatrixRef<'_>) -> Qr {
        let qr = to_faer(a).qr(Par::Seq);
        Qr {
            q: from_faer(qr.compute_thin_Q(Par::Seq).as_ref()),
            r: from_faer(qr.thin_R()),
        }
    }

    fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<Self::Q> {
        let qr = to_faer(a).col_piv_qr(Par::Seq);
        let r = from_faer(qr.thin_R());
        let forward = qr.P().arrays().0.to_vec();
        PivotedQr { r, forward, q: qr }
    }

    /// The reflectors applied to `x` scattered into the rows `kept` of a
    /// `rows × rank` zero matrix. The reflectors after the last kept row
    /// leave that matrix unchanged, so they are not applied.
    fn q_kept_times(
        qr: &PivotedQr<Self::Q>,
        kept: &[usize],
        x: &[Complex64],
        rank: usize,
    ) -> Vec<Complex64> {
        let rows = qr.q.Q_basis().nrows();
        let mut out = vec![Complex64::new(0.0, 0.0); rows * rank];
        for j in 0..rank {
            for (at, &i) in kept.iter().enumerate() {
                out[i + rows * j] = x[at + kept.len() * j];
            }
        }
        let reflectors = kept.last().map_or(0, |&i| i + 1);
        let basis = qr.q.Q_basis().subcols(0, reflectors);
        let coeff = qr.q.Q_coeff().subcols(0, reflectors);
        let mut buffer = MemBuffer::new(
            apply_block_householder_sequence_on_the_left_in_place_scratch::<Complex64>(
                rows,
                coeff.nrows(),
                rank,
            ),
        );
        apply_block_householder_sequence_on_the_left_in_place_with_conj(
            basis,
            coeff,
            Conj::No,
            MatMut::from_column_major_slice_mut(&mut out, rows, rank),
            Par::Seq,
            MemStack::new(&mut buffer),
        );
        out
    }

    /// By bidiagonalization and the QR algorithm; `None` when t4a-faer
    /// reports that it does not converge.
    fn tall_svd(rows: usize, cols: usize, w: &[Complex64]) -> Option<ColumnMajorSvd> {
        let mut params = <SvdParams as Auto<Complex64>>::auto();
        params.recursion_threshold = usize::MAX;
        let zero = Complex64::new(0.0, 0.0);
        let mut values = Diag::<Complex64>::zeros(cols);
        let mut u = vec![zero; rows * cols];
        let mut v = vec![zero; cols * cols];
        let mut buffer = MemBuffer::new(svd_scratch::<Complex64>(
            rows,
            cols,
            ComputeSvdVectors::Thin,
            ComputeSvdVectors::Thin,
            ParShape::Seq,
            params.into(),
        ));
        svd(
            MatRef::from_column_major_slice(w, rows, cols),
            values.as_mut(),
            Some(MatMut::from_column_major_slice_mut(&mut u, rows, cols)),
            Some(MatMut::from_column_major_slice_mut(&mut v, cols, cols)),
            Par::Seq,
            MemStack::new(&mut buffer),
            params.into(),
        )
        .ok()?;
        let values = values.column_vector();
        Some((u, (0..cols).map(|i| values[i].re).collect(), v))
    }
}

fn to_faer(a: MatrixRef<'_>) -> MatRef<'_, Complex64> {
    MatRef::from_column_major_slice(a.elements(), a.rows(), a.cols())
}

fn from_faer(m: MatRef<'_, Complex64>) -> Matrix {
    Matrix::from_fn(m.nrows(), m.ncols(), |i, j| m[(i, j)])
}
