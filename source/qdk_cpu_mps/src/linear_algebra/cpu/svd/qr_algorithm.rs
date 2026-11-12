// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`tall_svd_by_qr_algorithm`]: the thin SVD of a tall matrix by t4a-faer's
//! bidiagonalization and QR algorithm, the SVD's route for the large splits far
//! from orthogonal.
//!
//! On the splits of shallow circuits, H₂'s among them, `W` (the adjoint of the
//! rows the trim keeps) is nearly orthogonal and the rotations finish it in a
//! few sweeps; on the large splits of deep circuits it is far from orthogonal,
//! and the rotations need many more. The QR algorithm is then faster
//! (README.md, "Linear algebra"): it reduces `W` by reflections to a bidiagonal
//! matrix, nonzero only on its diagonal and the one above, and finds that
//! matrix's singular values by iteration. So the SVD gives this function the
//! `W` of a split whose trim keeps at least [`QR_ALGORITHM_MIN_RANK`] rows far
//! from orthogonal ([`QR_ALGORITHM_MIN_OFF_DIAGONAL`]), and the rotations every
//! other.
//!
//! Its answer is not trusted: the SVD factors `W` by the rotations instead when
//! this function reports a failure or answers factors that are not finite, and
//! [`checked`] takes the result only when it gives back the matrix and its
//! singular vectors are orthonormal, as for the rotations' own.
//!
//! It runs the QR algorithm at every size, never t4a-faer's default, divide and
//! conquer from size 128: on a 128 × 128 split of a polyene's chain, divide and
//! conquer returned factors that the checks refuse, so the run failed, where
//! the QR algorithm's pass.
//!
//! [`QR_ALGORITHM_MIN_RANK`]: super::QR_ALGORITHM_MIN_RANK
//! [`QR_ALGORITHM_MIN_OFF_DIAGONAL`]: super::QR_ALGORITHM_MIN_OFF_DIAGONAL
//! [`checked`]: super::checks::checked

use super::ColumnMajorSvd;
use num_complex::Complex64;
use t4a_faer::{
    Auto, MatMut, MatRef, Par, ParShape,
    diag::Diag,
    dyn_stack::{MemBuffer, MemStack},
    linalg::svd::{ComputeSvdVectors, SvdParams, svd, svd_scratch},
};

/// `W = U·diag(S)·V†` of the `rows × cols` matrix `w` (column-major,
/// `rows ≥ cols ≥ 1`), by bidiagonalization and the QR algorithm: `U`
/// (`rows × cols`), the values nonincreasing, and `V` (`cols × cols`),
/// column-major; or `None` when t4a-faer reports that it does not
/// converge. It does not check its factors.
pub(crate) fn tall_svd_by_qr_algorithm(
    rows: usize,
    cols: usize,
    w: &[Complex64],
) -> Option<ColumnMajorSvd> {
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
