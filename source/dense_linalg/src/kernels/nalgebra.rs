// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Nalgebra`]: the [`Kernels`] on nalgebra, a dependency of the QDK's
//! simulators already, with matrixmultiply's complex product.
//!
//! Its fast route ([`Nalgebra::tall_svd`]) is nalgebra's bidiagonalization
//! and our port of LAPACK's QR algorithm on the bidiagonal
//! ([`bidiagonal_qr`]).

pub(crate) mod bidiagonal_qr;

use self::bidiagonal_qr::bidiagonal_qr;
use super::{Kernels, PivotedQr};
use crate::matrix::{Matrix, MatrixRef};
use crate::qr::Qr;
use crate::svd::ColumnMajorSvd;
use matrixmultiply::CGemmOption;
use nalgebra::{Bidiagonal, DMatrix, DMatrixView};
use num_complex::Complex64;

/// The [`Kernels`] on nalgebra.
pub(crate) struct Nalgebra;

impl Kernels for Nalgebra {
    /// `Q` itself, thin: nalgebra applies only `Q†` in place.
    type Q = DMatrix<Complex64>;

    /// By matrixmultiply's complex kernel, which nalgebra uses for real
    /// matrices alone: nalgebra's own complex product is about 4 times
    /// slower on the checks' shapes (measured).
    fn product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
        let (rows, inner, cols) = (a.rows(), a.cols(), b.cols());
        let mut elements = vec![Complex64::new(0.0, 0.0); rows * cols];
        let stride = |n: usize| isize::try_from(n).expect("a matrix dimension fits isize");
        // SAFETY: `Complex64` is `#[repr(C)]` with fields `re` and `im`, so
        // it has the layout of `[f64; 2]`; `a`, `b` and `elements` hold
        // `rows × inner`, `inner × cols` and `rows × cols` elements,
        // column-major, as the strides say, and `elements` aliases neither.
        unsafe {
            matrixmultiply::zgemm(
                CGemmOption::Standard,
                CGemmOption::Standard,
                rows,
                inner,
                cols,
                [1.0, 0.0],
                a.elements().as_ptr().cast(),
                1,
                stride(rows),
                b.elements().as_ptr().cast(),
                1,
                stride(inner),
                [0.0, 0.0],
                elements.as_mut_ptr().cast(),
                1,
                stride(rows),
            );
        }
        Matrix::new(rows, cols, elements)
    }

    fn qr(a: MatrixRef<'_>) -> Qr {
        let qr = to_nalgebra(a).clone_owned().qr();
        Qr {
            q: from_nalgebra(qr.q()),
            r: from_nalgebra(qr.r()),
        }
    }

    fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<Self::Q> {
        let qr = to_nalgebra(a).clone_owned().col_piv_qr();
        let mut forward = DMatrix::from_fn(1, a.cols(), |_, j| j);
        qr.p().permute_columns(&mut forward);
        PivotedQr {
            r: from_nalgebra(qr.r()),
            forward: forward.as_slice().to_vec(),
            q: qr.q(),
        }
    }

    /// The product of `Q`'s columns `kept` with `x`.
    fn q_kept_times(
        qr: &PivotedQr<Self::Q>,
        kept: &[usize],
        x: &[Complex64],
        rank: usize,
    ) -> Vec<Complex64> {
        let q_kept = qr.q.select_columns(kept);
        let q_kept = MatrixRef::new(q_kept.nrows(), kept.len(), q_kept.as_slice());
        let x = MatrixRef::new(kept.len(), rank, x);
        Self::product(q_kept, x).into_elements()
    }

    /// nalgebra's bidiagonalization `W = U_B·B·V_B†`, with `B` real, then
    /// the QR algorithm on `B` ([`bidiagonal_qr`]); `None` when it does
    /// not converge in `dbdsqr`'s budget.
    fn tall_svd(rows: usize, cols: usize, w: &[Complex64]) -> Option<ColumnMajorSvd> {
        let bidiagonal = Bidiagonal::new(DMatrix::from_column_slice(rows, cols, w));
        let mut values = bidiagonal.diagonal().as_slice().to_vec();
        let mut above = bidiagonal.off_diagonal().as_slice().to_vec();
        let identity = || {
            let mut m = vec![0.0; cols * cols];
            for i in 0..cols {
                m[i + cols * i] = 1.0;
            }
            m
        };
        let (mut u_b, mut v_b) = (identity(), identity());
        if !bidiagonal_qr(&mut values, &mut above, &mut u_b, &mut v_b) {
            return None;
        }
        let complex = |m: Vec<f64>| {
            Matrix::new(
                cols,
                cols,
                m.into_iter().map(|x| Complex64::new(x, 0.0)).collect(),
            )
        };
        let u = Self::product(from_nalgebra(bidiagonal.u()).view(), complex(u_b).view());
        let v_bidiagonal = from_nalgebra(bidiagonal.v_t()).view().adjoint();
        let v = Self::product(v_bidiagonal.view(), complex(v_b).view());
        Some((u.into_elements(), values, v.into_elements()))
    }
}

fn to_nalgebra(a: MatrixRef<'_>) -> DMatrixView<'_, Complex64> {
    DMatrixView::from_slice(a.elements(), a.rows(), a.cols())
}

fn from_nalgebra(m: DMatrix<Complex64>) -> Matrix {
    let (rows, cols) = m.shape();
    Matrix::new(rows, cols, m.data.into())
}
