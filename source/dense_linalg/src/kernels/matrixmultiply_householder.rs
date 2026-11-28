// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MatrixmultiplyHouseholder`]: the [`Kernels`] on matrixmultiply's
//! product and the crate's own Householder QRs.
//!
//! - The product: matrixmultiply's complex `zgemm`. matrixmultiply is
//!   already in the QDK's dependency tree, under ndarray's products.
//! - The thin QR, the column-pivoted QR and the application of its `Q`:
//!   the crate's own Householder reflections
//!   ([`mod@householder_reflectors`]). They are the crate's own for two
//!   rules: the reflector takes the phase of a subnormal entry without
//!   losing its modulus, and the pivoted QR keeps every pivot's `|R_kk|`
//!   at least the norm of every later column's remaining rows, so that `R`
//!   reveals the rank the SVD's trim reads.
//!
//! Each step is pure Rust, so the crate builds for WebAssembly too. The
//! product is tuned for larger matrices than, for instance, an MPS
//! simulation's. The QRs and the pivoted QR's `Q` are unblocked. The SVD
//! checks its answer, so a wrong pivoted QR or `Q` cannot pass silently.
//! The thin QR is unchecked.

mod householder_reflectors;

use self::householder_reflectors::HouseholderReflectors;
use super::{Kernels, PivotedQr};
use crate::matrix::{Matrix, MatrixRef};
use crate::qr::Qr;
use matrixmultiply::CGemmOption;
use num_complex::Complex64;

/// The [`Kernels`] on matrixmultiply's product and Householder
/// reflections.
pub(crate) struct MatrixmultiplyHouseholder;

impl Kernels for MatrixmultiplyHouseholder {
    type Q = HouseholderReflectors;

    fn product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
        let (rows, inner, cols) = (a.rows(), a.cols(), b.cols());
        let mut elements = vec![Complex64::new(0.0, 0.0); rows * cols];
        if elements.is_empty() || inner == 0 {
            return Matrix::new(rows, cols, elements);
        }
        multiply_into(a, b, &mut elements);
        Matrix::new(rows, cols, elements)
    }

    fn qr(a: MatrixRef<'_>) -> Qr {
        householder_reflectors::qr(a)
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

/// Writes `a·b` into `elements`, `a.rows() × b.cols()` column-major, with
/// matrixmultiply's `zgemm`.
///
/// # Panics
///
/// Unless `a` and `b` agree on the inner dimension, `elements` holds
/// `a.rows()·b.cols()` elements, and neither the inner dimension nor
/// `elements` is empty. [`MatrixmultiplyHouseholder::product`] answers the
/// empty products before calling it. The checks run once per product.
fn multiply_into(a: MatrixRef<'_>, b: MatrixRef<'_>, elements: &mut [Complex64]) {
    let (rows, inner, cols) = (a.rows(), a.cols(), b.cols());
    assert!(
        b.rows() == inner
            && rows.checked_mul(cols) == Some(elements.len())
            && inner > 0
            && !elements.is_empty(),
        "a {rows} × {inner} matrix times a {} × {cols} one into {} elements",
        b.rows(),
        elements.len()
    );
    // `a` holds `rows·inner` elements and `b` `inner·cols`, both nonzero
    // by the assert, so each dimension is at most an operand's element
    // count, which a slice keeps within `isize::MAX`.
    let stride = |n: usize| isize::try_from(n).expect("a matrix dimension fits isize");
    // SAFETY:
    // - `Complex64` is `#[repr(C)]` with the fields `re` and `im`, so it
    //   has the layout of `[f64; 2]`, which `zgemm` reads;
    // - `a` holds `rows × inner` elements and `b` `inner × cols`, by
    //   `MatrixRef`'s invariant (`MatrixRef::new` asserts it), and the
    //   assert above makes `b.rows()` the inner dimension;
    // - `elements` holds `rows × cols`, by the assert above;
    // - all three are column-major, as the strides say;
    // - `elements` aliases neither operand: it is a `&mut` borrow beside
    //   two shared ones.
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
}
