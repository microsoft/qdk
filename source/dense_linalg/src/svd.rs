// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Svd`]: the thin singular value decomposition (SVD) of a dense complex
//! matrix, checked, and its truncation; [`SvdError`], why it failed. Thin:
//! only the `r ≤ min(rows, cols)` values and vectors that give back the
//! matrix (`docs/algorithms.md`'s Terms).
//!
//! A caller that truncates a matrix by its singular values trusts them. One
//! that keeps factors as an orthonormal basis trusts the singular vectors.
//! The matrices met when simulating matrix product states are
//! rank-deficient most of the time (they have fewer nonzero singular values
//! than their smaller side), with repeated values. General SVDs have
//! returned wrong factors on such matrices without reporting a failure:
//!
//! - nalgebra 0.35's: finite and orthonormal factors whose product missed
//!   a 56 × 72 matrix by about 330 times the checks' bound. The matrix
//!   came from an MPS simulation of H₂, a chemistry instance the QDK
//!   simulates;
//! - faer 0.24's, on degenerate and rank-deficient matrices;
//! - t4a-faer's default method, divide and conquer, on a 128 × 128 matrix
//!   from an MPS simulation of a polyene's chain, another such instance.
//!
//! So [`Svd::of`] checks every answer, and its doc states what it
//! guarantees. It runs these steps in order ([`trimmed_thin_svd`], then the
//! checks, [`checks`]):
//!
//! 1. The zero matrix gets one value, 0, and the first unit vectors
//!    ([`zero_matrix_svd`]). It has no rank for the trim to keep, and no
//!    largest entry to scale by.
//! 2. A wide matrix is factored through its adjoint, so that the rotated
//!    columns have length `min(m, n)` ([`tall_and_in_range`]).
//! 3. A matrix whose largest entry is far from 1 is scaled by a power of
//!    two ([`PowerOfTwoScaling`]). Then the largest squared column norms
//!    neither overflow nor underflow. A far smaller column's may
//!    underflow, far below rounding of the largest. Its singular values
//!    are scaled back at the end ([`trimmed_thin_svd`] says when that
//!    rounds).
//! 4. The trim ([`rows_kept_by_trim`]): a column-pivoted QR of the matrix
//!    `B` that steps 2 and 3 give, `B·P = Q·R`.
//!    It drops the rows of `R` of smallest norm, as many as keep the
//!    dropped rows' Frobenius norm within `max(m, n) · ε` times the largest
//!    row's norm. So the trim changes the matrix by no more than rounding
//!    does, whatever its size. The factorization after it sees full rank.
//!    The rows are chosen by their norms ([`rows_kept_by_trim`] says why).
//!    Preparing a matrix for the rotations by a pivoted QR is Drmač and
//!    Veselić's preconditioning (Further reading).
//! 5. The rotations: `W`, the adjoint of the kept rows of `R·Pᵀ`
//!    ([`adjoint_of_kept_rows`]), is factored by the one-sided Jacobi
//!    rotations ([`jacobi`]).
//! 6. The left singular vectors are the kept columns of `Q` times `W`'s
//!    right ones ([`Kernels::q_kept_times`]).
//!
//! The same steps as a pipeline, for an `m × n` matrix `A`:
//!
//! ```text
//!   A ── all zero? ──yes──▶ one value, 0, first unit vectors        step 1
//!   │ no
//!   ▼
//!   B = A, or A† when wide, times 2⁻ᵉ when far from 1             steps 2, 3
//!   │ column-pivoted QR
//!   ▼
//!   B·P = Q·R ── drop R's shortest rows, jointly within rounding:   step 4
//!   │            R_kept
//!   ▼
//!   W = (R_kept·Pᵀ)† = U_W·S·V_W†  (the rotations)                  step 5
//!   │
//!   ▼
//!   B = (Q_kept·V_W)·S·U_W†                                         step 6
//!   │ swap the factors back when wide, S times 2ᵉ
//!   ▼
//!   A = U·S·V†, then checked
//! ```
//!
//! The Terms of `docs/algorithms.md` define the terms: singular value,
//! orthonormal, adjoint, Frobenius norm, `ε`, subnormal, pivoted QR, trim,
//! rotation.
//!
//! # Further reading
//!
//! - G. H. Golub and C. F. Van Loan, *Matrix Computations*, 4th ed., Johns
//!   Hopkins University Press, 2013. §8.6, "Computing the SVD", says what
//!   the SVD is and the usual ways to compute it.
//! - Z. Drmač and K. Veselić, "New fast and accurate Jacobi SVD algorithm.
//!   I", SIAM J. Matrix Anal. Appl. 29(4):1322–1342, 2008. It puts a
//!   column-pivoted QR before the Jacobi rotations, as steps 4 and 5 do.

mod checks;
mod jacobi;

use self::checks::checked;
use self::jacobi::{ColumnMajorSvd, MAX_SWEEPS, svd_by_rotations};
use crate::kernels::{DefaultKernels, Kernels};
use crate::matrix::{Matrix, MatrixRef};
use crate::scaling::PowerOfTwoScaling;
use num_complex::Complex64;

/// A thin singular value decomposition `a = u·diag(s)·v†`, from
/// [`Svd::of`]. Or its first `k` values and their vectors, from
/// [`Svd::truncated`], whose product only approximates `a`.
#[derive(Clone, Debug)]
pub struct Svd {
    /// The left singular vectors, as columns: `rows × r`, orthonormal.
    pub u: Matrix,
    /// The `r` singular values, nonincreasing.
    pub singular_values: Vec<f64>,
    /// The adjoint of the right singular vectors, the vectors as rows. It
    /// is `r × cols`, orthonormal.
    pub v_adjoint: Matrix,
}

impl Svd {
    /// The thin SVD of `a`, checked: `r` values in nonincreasing order,
    /// `1 ≤ r ≤ min(rows, cols)`. It guarantees:
    ///
    /// - **Checked factors.** Up to rounding that grows with the matrix's
    ///   size, `u·diag(s)·v†` gives back `a`, and `u` and `v` are
    ///   orthonormal: the bounds of LAPACK's SVD tests. Otherwise it fails
    ///   with an [`SvdError`]. So it never answers values that are not
    ///   finite, nor factors whose product is not the matrix.
    /// - **Only rounding left out.** It leaves out the singular values of
    ///   rounding. Those are the values of the rows of the pivoted QR's `R`
    ///   of smallest norm, as many as keep the dropped rows' Frobenius norm
    ///   within `max(m, n) · ε` times the largest row's. Rows that are
    ///   exactly zero are always among them. Take a matrix whose `R` has `r`
    ///   rows each above that bound, and the others jointly within it. It
    ///   gets exactly `r` values. For instance, a product of random factors
    ///   with `r` columns has rank `r`. The zero matrix gets one value, 0.
    /// - **Any scale.** It answers at any scale, down to subnormal entries.
    ///   The exception is matrices whose singular values are too small or
    ///   too large to store, which the checks refuse.
    ///
    /// The Terms of `docs/algorithms.md` define the terms: singular value,
    /// orthonormal, Frobenius norm, `ε`, subnormal, pivoted QR, trim.
    ///
    /// # Errors
    ///
    /// - [`SvdError::DidNotConverge`] when the rotations still turn after
    ///   their last sweep.
    /// - [`SvdError::FailedChecks`] when an entry of `a` is not finite
    ///   (refused before any work), or the factors are not finite, do not
    ///   give back `a`, or have singular vectors that are not orthonormal.
    ///
    /// # Panics
    ///
    /// When `a` is empty (no rows or no columns). It has no singular value.
    pub fn of(a: MatrixRef<'_>) -> Result<Self, SvdError> {
        let (rows, cols) = (a.rows(), a.cols());
        assert!(
            rows > 0 && cols > 0,
            "the SVD of an empty {rows} × {cols} matrix"
        );
        if !a.elements().iter().all(|x| x.is_finite()) {
            // No SVD gives it back to rounding, and the scaling (step 3 of
            // the module doc) reads only finite entries.
            return Err(SvdError::FailedChecks { rows, cols });
        }
        checked::<DefaultKernels>(a, trimmed_thin_svd::<DefaultKernels>(a))
    }

    /// The SVD's first `k` values and their vectors: `u`'s first `k`
    /// columns, the `k` largest values, and `v†`'s first `k` rows, each
    /// element as it was. To rounding, their product is a closest matrix of
    /// rank at most `k` to the decomposed one: it leaves out
    /// `√(s_k² + s_{k+1}² + ⋯)` in the Frobenius norm (`docs/algorithms.md`'s
    /// Terms, under SVD). The values are kept apart: a caller that wants
    /// them in a factor multiplies them in itself
    /// ([`Matrix::multiply_columns_by`], [`Matrix::multiply_rows_by`]).
    ///
    /// ```text
    ///   u (m × r) · diag(s) (r) · v† (r × n)
    ///      → u's columns 0..k, s[0..k], v†'s rows 0..k
    /// ```
    ///
    /// # Panics
    ///
    /// When `k` is 0 or more than the SVD's number of values: a truncation
    /// keeps at least one value, and has no more to keep.
    #[must_use]
    pub fn truncated(self, k: usize) -> Self {
        let r = self.singular_values.len();
        assert!(
            (1..=r).contains(&k),
            "truncating an SVD of {r} singular values to {k}"
        );
        let Svd {
            u,
            mut singular_values,
            v_adjoint,
        } = self;
        singular_values.truncate(k);
        Svd {
            u: first_columns(u, k),
            singular_values,
            v_adjoint: first_rows(&v_adjoint, k),
        }
    }
}

/// Why the crate's checked singular value decomposition (SVD),
/// [`Svd::of`], failed. `docs/algorithms.md` gives its checks.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SvdError {
    /// The SVD's one-sided Jacobi rotations still rotated after their last
    /// sweep.
    #[error("the SVD of a {rows} × {cols} matrix did not converge")]
    DidNotConverge {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
    /// The factors were not finite, did not give back the matrix, or had
    /// singular vectors that are not orthonormal. Or the matrix had an
    /// entry that is not finite.
    #[error("the SVD of a {rows} × {cols} matrix failed its accuracy checks")]
    FailedChecks {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
}

/// The thin SVD of `a` (not empty) after a pivoted-QR trim, by the
/// rotations. `None` when their last sweep allowed ([`MAX_SWEEPS`]) still
/// rotates a pair.
///
/// It returns `r` values, `1 ≤ r ≤ min(m, n)` for a finite matrix, one for
/// each row the trim keeps. The values left out are rounding's (the zero
/// matrix gives one value, 0). This function does not check the result;
/// the caller does ([`checks::checked`]).
///
/// The scaling ([`PowerOfTwoScaling`], step 3 of the module doc) rounds
/// only the entries it takes below the normal range. It rounds them by far
/// less than the checks allow. A matrix whose singular values are too
/// large to store (above `f64::MAX`) gets infinite ones once they are
/// scaled back. The checks refuse them, as they refuse values too small to
/// store to their bound.
pub(crate) fn trimmed_thin_svd<K: Kernels>(a: MatrixRef<'_>) -> Option<Svd> {
    let (rows, cols) = (a.rows(), a.cols());
    if is_zero_matrix(a) {
        return Some(zero_matrix_svd(rows, cols));
    }
    let wide = rows < cols;
    let scaling = PowerOfTwoScaling::of(a.elements());
    let prepared = tall_and_in_range(a, wide, scaling.as_ref());
    let tall = prepared.as_ref().map_or(a, Matrix::view);
    let ColumnMajorSvd {
        left_vectors,
        mut singular_values,
        right_vectors,
    } = factor::<K>(tall)?;
    if let Some(scaling) = &scaling {
        scale_back(&mut singular_values, scaling);
    }
    let rank = singular_values.len();
    // A is `tall`, or its adjoint when wide, which swaps the factors.
    let (u, v) = if wide {
        (right_vectors, left_vectors)
    } else {
        (left_vectors, right_vectors)
    };
    Some(Svd {
        u: Matrix::new(rows, rank, u),
        singular_values,
        v_adjoint: MatrixRef::new(cols, rank, &v).adjoint(),
    })
}

/// Whether every entry of `a` is zero (step 1 of the module doc).
fn is_zero_matrix(a: MatrixRef<'_>) -> bool {
    let zero = Complex64::new(0.0, 0.0);
    a.elements().iter().all(|x| *x == zero)
}

/// Undoes step 3 of the module doc on the singular values of the scaled
/// matrix.
fn scale_back(singular_values: &mut [f64], scaling: &PowerOfTwoScaling) {
    for value in singular_values {
        *value = scaling.scaled_back(*value);
    }
}

/// The SVD of the `rows × cols` zero matrix: one value, 0, and the first
/// unit vectors. It is answered apart. The trim would keep no row (its
/// tolerance is 0), and the matrix has no largest entry to scale by.
fn zero_matrix_svd(rows: usize, cols: usize) -> Svd {
    let first = |n| {
        Matrix::from_fn(n, 1, |i, _| {
            Complex64::new(f64::from(u8::from(i == 0)), 0.0)
        })
    };
    Svd {
        u: first(rows),
        singular_values: vec![0.0],
        v_adjoint: first(cols).view().adjoint(),
    }
}

/// The matrix [`factor`] reads in place of `a` (steps 2 and 3 of the
/// module doc):
///
/// - its adjoint when `a` is `wide`, so that the rotated columns are the
///   shorter side;
/// - multiplied by `scaling` when there is one, so that its largest
///   squared norms neither overflow nor underflow.
///
/// `None` when `a` needs neither, and is read as it is.
fn tall_and_in_range(
    a: MatrixRef<'_>,
    wide: bool,
    scaling: Option<&PowerOfTwoScaling>,
) -> Option<Matrix> {
    (wide || scaling.is_some()).then(|| {
        let tall = if wide {
            a.adjoint()
        } else {
            Matrix::new(a.rows(), a.cols(), a.elements().to_vec())
        };
        match scaling {
            Some(scaling) => scaling.scaled(tall),
            None => tall,
        }
    })
}

/// The SVD of `tall`, the module doc's `B`: `B = U·S·V†`, or `None` when
/// the rotations do not converge. Its steps:
///
/// 1. the column-pivoted QR `B·P = Q·R`;
/// 2. the trim of `R`'s shortest rows, jointly within rounding
///    ([`rows_kept_by_trim`]);
/// 3. the rotations on the adjoint of the rows kept
///    ([`adjoint_of_kept_rows`], [`svd_by_rotations`]);
/// 4. `U` from the columns of `Q` kept.
///
/// The caller passes `B` with `rows ≥ cols` for speed alone, so that the
/// rotated columns are the shorter side. A wide `B` gives the same factors
/// at more cost.
fn factor<K: Kernels>(tall: MatrixRef<'_>) -> Option<ColumnMajorSvd> {
    let cols = tall.cols();
    let qr = K::pivoted_qr(tall);
    let kept = rows_kept_by_trim(&qr.r, tall.rows().max(cols));
    let columns_to_rotate = adjoint_of_kept_rows(&qr.r, &qr.forward, &kept);
    // W = U_W·S·V_W†, so R_kept·Pᵀ = V_W·S·U_W† and B = (Q_kept·V_W)·S·U_W†:
    // W's left vectors are B's right ones, and W's right vectors, mapped
    // through the kept columns of Q, are B's left ones.
    let ColumnMajorSvd {
        left_vectors: right_vectors,
        singular_values,
        right_vectors: rotations,
    } = svd_by_rotations(cols, kept.len(), columns_to_rotate, MAX_SWEEPS)?;
    let left_vectors = K::q_kept_times(&qr, &kept, &rotations, singular_values.len());
    Some(ColumnMajorSvd {
        left_vectors,
        singular_values,
        right_vectors,
    })
}

/// The trim (step 4 of the module doc): the rows of `r` it keeps, in
/// increasing order. It drops the rows of smallest norm, as many as keep
/// the dropped rows' Frobenius norm within `larger_side · ε` times the
/// largest row's norm. So what it drops is no more than rounding could have
/// put there. `larger_side` is the larger dimension of the matrix `r`
/// comes from.
///
/// The rows are chosen by their norms, not by their diagonal entries and
/// not as a prefix, for these reasons:
///
/// - The choice then does not rest on `|R_ii|` being nonincreasing.
///   [`Kernels::pivoted_qr`]'s pivoting keeps it so only to within about
///   `√ε/2` of the pivot.
/// - A blocked pivoting need not keep it at all. On a 4 × 4 matrix from an
///   MPS simulation of H₂, t4a-faer's put a pivot of 1.9e-16 before one of
///   1.3e-9, as faer 0.24's did.
/// - A row's norm may exceed its diagonal entry.
///
/// For instance, rows of norms 3.1, 0.8, 3e-16 and 1e-16, with
/// `larger_side` 4, give a tolerance of `4 · ε · 3.1 ≈ 2.8e-15`:
///
/// ```text
///   shortest first   row 3    row 2    row 1    row 0
///   norm             1e-16    3e-16    0.8      3.1
///   dropped so far   1e-16    3.2e-16  0.8
///   within 2.8e-15?  yes      yes      no: stop
/// ```
///
/// It drops rows 3 and 2, and keeps rows 0 and 1.
fn rows_kept_by_trim(r: &Matrix, larger_side: usize) -> Vec<usize> {
    let row_norms = row_norms(r);
    let tolerance = trim_tolerance(&row_norms, larger_side);
    let shortest_first = shortest_first(&row_norms);
    let dropped_count = count_within_tolerance(&shortest_first, &row_norms, tolerance);
    let mut kept = shortest_first[dropped_count..].to_vec();
    kept.sort_unstable();
    kept
}

/// The norm of each row of the upper trapezoidal `r`, from its diagonal
/// on: the measure by which the trim chooses rows, not the diagonal entry
/// alone.
fn row_norms(r: &Matrix) -> Vec<f64> {
    let (r_rows, cols, r) = (r.rows(), r.cols(), r.elements());
    let at = |i: usize, j: usize| r[i + r_rows * j];
    (0..r_rows)
        .map(|i| (i..cols).map(|j| at(i, j).norm_sqr()).sum::<f64>().sqrt())
        .collect()
}

/// The trim's tolerance, how much the dropped rows' Frobenius norm may
/// reach: `larger_side · ε` times the largest row's norm.
#[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
fn trim_tolerance(row_norms: &[f64], larger_side: usize) -> f64 {
    let largest = row_norms.iter().copied().fold(0.0, f64::max);
    larger_side as f64 * f64::EPSILON * largest
}

/// The rows, by increasing norm.
fn shortest_first(row_norms: &[f64]) -> Vec<usize> {
    let mut rows: Vec<usize> = (0..row_norms.len()).collect();
    rows.sort_by(|&i, &j| row_norms[i].total_cmp(&row_norms[j]));
    rows
}

/// How many of the `shortest_first` rows fit, together, within
/// `tolerance`: the length of the longest prefix whose Frobenius norm is at
/// most `tolerance`. The trim drops that many.
fn count_within_tolerance(shortest_first: &[usize], row_norms: &[f64], tolerance: f64) -> usize {
    let mut dropped_norm_sqr = 0.0;
    shortest_first
        .iter()
        .take_while(|&&i| {
            dropped_norm_sqr += row_norms[i] * row_norms[i];
            dropped_norm_sqr <= tolerance * tolerance
        })
        .count()
}

/// `W = (R_kept·Pᵀ)†`, `cols × kept.len()`, the matrix the rotations
/// factor (step 5 of the module doc). It is the rows `kept` of `r`, put back
/// in the matrix's own column order (`(R·Pᵀ)[i, forward[j]] = R[i, j]`), and
/// conjugated and transposed. Each column is stored as `[re…, im…]`, as the
/// rotations read it.
fn adjoint_of_kept_rows(r: &Matrix, forward: &[usize], kept: &[usize]) -> Vec<f64> {
    let (r_rows, cols, r) = (r.rows(), r.cols(), r.elements());
    let at = |i: usize, j: usize| r[i + r_rows * j];
    let mut w = vec![0.0; 2 * cols * kept.len()];
    for (column_at, &i) in kept.iter().enumerate() {
        let column = &mut w[2 * cols * column_at..2 * cols * (column_at + 1)];
        for j in i..cols {
            let x = at(i, j);
            column[forward[j]] = x.re;
            column[cols + forward[j]] = -x.im;
        }
    }
    w
}

/// The first `count` columns of `matrix`. Column-major, they are its first
/// `rows·count` elements, kept in place.
fn first_columns(matrix: Matrix, count: usize) -> Matrix {
    let rows = matrix.rows();
    let mut elements = matrix.into_elements();
    elements.truncate(rows * count);
    Matrix::new(rows, count, elements)
}

/// The first `count` rows of `matrix`, gathered column by column.
fn first_rows(matrix: &Matrix, count: usize) -> Matrix {
    let (rows, cols) = (matrix.rows(), matrix.cols());
    let elements = matrix.elements();
    let kept = (0..cols)
        .flat_map(|j| (0..count).map(move |i| elements[i + rows * j]))
        .collect();
    Matrix::new(count, cols, kept)
}

#[cfg(test)]
mod tests;
