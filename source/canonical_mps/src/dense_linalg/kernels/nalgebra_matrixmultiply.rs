// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NalgebraMatrixmultiply`]: the [`Kernels`] on the dense linear algebra
//! the QDK already depends on, nalgebra (its simulators' algebra) and
//! matrixmultiply (the product under ndarray's complex products and
//! nalgebra's real ones), with a column-pivoted QR of the module's own:
//!
//! - the product: matrixmultiply's complex `zgemm`;
//! - the thin QR: nalgebra's `QR`, by Householder reflections;
//! - the column-pivoted QR and the application of its `Q`: the module's
//!   own Householder QR ([`mod@householder_pivoted_qr`]), because nalgebra's
//!   `ColPivQR` does not keep the column-norm property that makes `R`
//!   reveal the rank the SVD's trim reads.
//!
//! Each step runs on the calling thread, in pure Rust, so the module builds
//! for WebAssembly too. The product is tuned for larger matrices than an
//! MPS simulation's, and the pivoted QR and its `Q` are unblocked. The SVD
//! checks its answer, so a wrong QR or `Q` cannot pass silently.

mod householder_pivoted_qr;

use self::householder_pivoted_qr::HouseholderReflectors;
use super::{Kernels, PivotedQr};
use crate::dense_linalg::matrix::{Matrix, MatrixRef};
use crate::dense_linalg::qr::Qr;
use matrixmultiply::CGemmOption;
use nalgebra::{DMatrix, DMatrixView};
use num_complex::Complex64;

/// The [`Kernels`] on nalgebra and matrixmultiply.
pub(crate) struct NalgebraMatrixmultiply;

impl Kernels for NalgebraMatrixmultiply {
    type Q = HouseholderReflectors;

    fn product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
        let (rows, inner, cols) = (a.rows(), a.cols(), b.cols());
        let mut elements = vec![Complex64::new(0.0, 0.0); rows * cols];
        if elements.is_empty() || inner == 0 {
            return Matrix::new(rows, cols, elements);
        }
        // Each dimension is at most an operand's element count, which a
        // `Vec` keeps within `isize::MAX`.
        let stride = |n: usize| isize::try_from(n).expect("a matrix dimension fits isize");
        // SAFETY: `Complex64` is `#[repr(C)]` with the fields `re` and `im`,
        // so it has the layout of `[f64; 2]`; `a`, `b` and `elements` hold
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
        let qr = DMatrixView::from_slice(a.elements(), a.rows(), a.cols())
            .clone_owned()
            .qr();
        Qr {
            q: from_nalgebra(qr.q()),
            r: from_nalgebra(qr.r()),
        }
    }

    fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<Self::Q> {
        HouseholderReflectors::pivoted_qr(a)
    }

    /// The reflectors applied to `x` scattered into the rows `kept` of a
    /// `rows × rank` zero matrix.
    fn q_kept_times(
        qr: &PivotedQr<Self::Q>,
        kept: &[usize],
        x: &[Complex64],
        rank: usize,
    ) -> Vec<Complex64> {
        qr.q.kept_times(kept, x, rank)
    }
}

fn from_nalgebra(m: DMatrix<Complex64>) -> Matrix {
    let (rows, cols) = m.shape();
    Matrix::new(rows, cols, m.data.into())
}
