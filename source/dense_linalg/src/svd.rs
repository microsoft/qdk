// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Svd`]: the thin singular value decomposition (SVD) of a dense complex
//! matrix, checked, and [`SvdError`], why it failed.
//!
//! A caller that truncates a matrix by its singular values trusts them, and
//! one that keeps factors as an orthonormal basis trusts the singular
//! vectors. The matrices met when simulating matrix product states are
//! rank-deficient most of the time (they have fewer nonzero singular values
//! than their smaller side), with repeated values, and general SVDs have
//! returned wrong factors on such matrices without reporting a failure:
//! nalgebra 0.35's and faer 0.24's on degenerate and rank-deficient
//! matrices, and t4a-faer's default method, divide and conquer, on a
//! 128 × 128 matrix from an MPS simulation of a polyene's chain.
//!
//! So [`Svd::of`] guarantees:
//!
//! - **Checked factors.** It answers only factors that give back the matrix
//!   and singular vectors that are orthonormal, to the bounds of LAPACK's
//!   SVD tests ([`checks`]); otherwise, and when the rotations still turn
//!   after their last sweep, it fails with an [`SvdError`]. So it never
//!   answers values that are not finite, nor factors whose product is not
//!   the matrix. A matrix with an entry that is not finite (NaN or
//!   infinite) fails the same way, before any work.
//! - **Only rounding left out.** It leaves out the singular values of
//!   rounding: those of the rows of the pivoted QR's `R` of smallest norm,
//!   as many as keep the dropped rows' Frobenius norm within
//!   `max(m, n) · ε` times the largest row's; rows that are exactly zero
//!   are always among them. It returns one value, 0, for the zero matrix.
//! - **Any scale.** It answers at any scale, down to subnormal entries,
//!   short of the matrices whose singular values are too small or too large
//!   to store, which the checks refuse.
//!
//! It runs these steps in order:
//!
//! 1. The zero matrix gets one value, 0, and the first unit vectors: it has
//!    no rank for the trim to keep and no largest entry to scale by.
//! 2. A wide matrix is factored through its adjoint, so that the rotated
//!    columns have length `min(m, n)`.
//! 3. A matrix whose largest entry is outside `[2⁻¹⁰⁰, 2¹⁰⁰]` is scaled by
//!    a power of two, so that the squared column norms neither overflow nor
//!    underflow, and its singular values are scaled back at the end
//!    ([`trimmed_thin_svd`] says when that rounds).
//! 4. The trim (Drmač's preconditioning): a column-pivoted QR, `A·P = Q·R`,
//!    whose rows of `R` of smallest norm are dropped, as many as keep the
//!    dropped rows' Frobenius norm within `max(m, n) · ε` times the largest
//!    row's norm. So the trim changes the matrix by no more than rounding
//!    does, whatever its size, and the factorization after it sees full
//!    rank. The rows are chosen by their norms ([`factor`] says why).
//! 5. The route: `W`, the adjoint of the kept rows of `R·Pᵀ`, goes to the
//!    fast route ([`Kernels::tall_svd`]) when the trim keeps at least
//!    [`FAST_ROUTE_MIN_RANK`] rows and they have, on average, at least
//!    [`FAST_ROUTE_MIN_OFF_DIAGONAL`] of their squared norm off the
//!    diagonal, the sign that `W` is far from orthogonal; every other `W`
//!    goes to the one-sided Jacobi rotations ([`jacobi`]), as does one the
//!    fast route reports a failure on or answers factors that are not
//!    finite for. The route changes the cost, not what is computed: both
//!    give the SVD of `W`, checked alike.
//! 6. The left singular vectors are the kept columns of `Q` times `W`'s
//!    right ones.
//!
//! The route's constants are performance choices (measured), not promises:
//! the tests pin that every route gives a checked SVD and that the fallback
//! answers, not where the route changes.

pub(crate) mod checks;
pub(crate) mod jacobi;

use self::checks::checked;
use self::jacobi::{MAX_SWEEPS, one_sided};
use crate::kernels::{Kernels, Shipped};
use crate::matrix::{Matrix, MatrixRef};
use num_complex::Complex64;

/// A thin singular value decomposition `a = u·diag(s)·v†`, from
/// [`Svd::of`].
#[derive(Clone, Debug)]
pub struct Svd {
    /// The left singular vectors, as columns: `rows × r`, orthonormal.
    pub u: Matrix,
    /// The `r` singular values, nonincreasing.
    pub singular_values: Vec<f64>,
    /// The adjoint of the right singular vectors, the vectors as rows:
    /// `r × cols`, orthonormal.
    pub v_adjoint: Matrix,
}

impl Svd {
    /// The thin SVD of `a`, to rounding, checked: `r` values in
    /// nonincreasing order, `1 ≤ r ≤ min(rows, cols)`, the values left out
    /// (`r < min(rows, cols)`) rounding's, and the zero matrix one value,
    /// 0. See the module doc for what it guarantees and how.
    ///
    /// # Errors
    ///
    /// [`SvdError::DidNotConverge`] when the rotations still turn after
    /// their last sweep, and [`SvdError::FailedChecks`] when an entry of
    /// `a` is not finite (refused before any work), or the factors are not
    /// finite, do not give back `a`, or have singular vectors that are not
    /// orthonormal.
    ///
    /// # Panics
    ///
    /// When `a` is empty (no rows or no columns): it has no singular value.
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
        checked::<Shipped>(a, trimmed_thin_svd::<Shipped>(a, &Shipped::tall_svd))
    }
}

/// Why [`Svd::of`] failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SvdError {
    /// The rotations still rotated after their last sweep.
    #[error("the SVD of a {rows} × {cols} matrix did not converge")]
    DidNotConverge {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
    /// The factors were not finite, did not give back the matrix, or had
    /// singular vectors that are not orthonormal; or the matrix had an
    /// entry that is not finite.
    #[error("the SVD of a {rows} × {cols} matrix failed its accuracy checks")]
    FailedChecks {
        /// The matrix's rows.
        rows: usize,
        /// The matrix's columns.
        cols: usize,
    },
}

/// The smallest kept rank whose `W` may go to the fast route: below it,
/// giving every `W` to t4a-faer's QR algorithm saved at most 3% of an SVD's
/// time, and cost 1.1 to 1.3 times the rotations' on small graded and
/// rank-deficient matrices (measured).
pub(crate) const FAST_ROUTE_MIN_RANK: usize = 32;

/// The smallest mean fraction `1 − |R_ii|²/‖R_i‖²` of the kept rows of `R`
/// whose `W` goes to the fast route: below it, as on the matrices of an MPS
/// simulation of H₂, the rotations end within five sweeps and are as fast
/// (measured).
pub(crate) const FAST_ROUTE_MIN_OFF_DIAGONAL: f64 = 0.1;

/// An SVD as column-major arrays, `(U, S, V)` with `W = U·diag(S)·V†`,
/// the values nonincreasing: `U` is `rows × r` and `V` is `cols × r`,
/// `W` being `rows × cols`. Both routes answer one.
pub(crate) type ColumnMajorSvd = (Vec<Complex64>, Vec<f64>, Vec<Complex64>);

/// The fast route, a thin SVD of a tall matrix, as [`Kernels::tall_svd`]
/// states it.
///
/// [`trimmed_thin_svd`] takes it as a parameter for the fallback's tests
/// alone: no matrix is known on which t4a-faer's QR algorithm fails or
/// answers values that are not finite, so the tests pass one that does.
pub(crate) type TallSvd<'f> = &'f dyn Fn(usize, usize, &[Complex64]) -> Option<ColumnMajorSvd>;

/// The thin SVD of `a` (not empty) after a pivoted-QR trim, by `tall_svd`
/// on the matrices the module doc routes to it and by the rotations
/// otherwise, or `None` when the rotations' last sweep allowed
/// ([`MAX_SWEEPS`]) still rotates a pair. [`Svd::of`] passes
/// [`Kernels::tall_svd`].
///
/// It returns `r` values, `1 ≤ r ≤ min(m, n)` for a finite matrix, one for
/// each row the trim keeps; the values left out are rounding's (the zero
/// matrix gives one value, 0). The caller checks the result
/// ([`checks::checked`]): this function does not.
///
/// The scaling by a power of two (step 3 of the module doc) is exact,
/// except on an entry it takes below the normal range (more than about
/// 2¹⁰²² times smaller than the largest), which it rounds, by far less than
/// the checks allow. A matrix whose singular values are too large to store
/// (above `f64::MAX`) gets infinite ones once they are scaled back, which
/// the checks refuse, as they refuse values too small to store to their
/// bound.
pub(crate) fn trimmed_thin_svd<K: Kernels>(a: MatrixRef<'_>, tall_svd: TallSvd<'_>) -> Option<Svd> {
    let (rows, cols) = (a.rows(), a.cols());
    let zero = Complex64::new(0.0, 0.0);
    if a.elements().iter().all(|x| *x == zero) {
        // The trim would keep no row (its tolerance is 0), and the scaling
        // below takes the logarithm of the largest entry: so the zero
        // matrix gets its one value here.
        let first = |n| {
            Matrix::from_fn(n, 1, |i, _| {
                Complex64::new(f64::from(u8::from(i == 0)), 0.0)
            })
        };
        return Some(Svd {
            u: first(rows),
            singular_values: vec![0.0],
            v_adjoint: first(cols).view().adjoint(),
        });
    }
    let wide = rows < cols;
    let largest = a
        .elements()
        .iter()
        .fold(0.0, |m: f64, x| m.max(x.re.abs()).max(x.im.abs()));
    let exponent = if largest.is_finite() && !(2f64.powi(-100)..=2f64.powi(100)).contains(&largest)
    {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a binary exponent, |e| < 1100"
        )]
        let exponent = largest.log2().round() as i32;
        exponent
    } else {
        0
    };
    // 2^-exponent as two factors, each finite whatever the exponent.
    let factors = [
        2f64.powi(-exponent / 2),
        2f64.powi(-(exponent - exponent / 2)),
    ];
    let prepared = (wide || exponent != 0).then(|| {
        let b = if wide {
            a.adjoint()
        } else {
            Matrix::new(rows, cols, a.elements().to_vec())
        };
        if exponent == 0 {
            b
        } else {
            let (r, c) = (b.rows(), b.cols());
            let scaled = b.into_elements().into_iter();
            Matrix::new(r, c, scaled.map(|x| x * factors[0] * factors[1]).collect())
        }
    });
    let b = prepared.as_ref().map_or(a, Matrix::view);
    let (u_b, mut singular_values, v_b) = factor::<K>(b, tall_svd)?;
    if exponent != 0 {
        for value in &mut singular_values {
            *value = *value / factors[0] / factors[1];
        }
    }
    let rank = singular_values.len();
    // B = U_B·S·V_B†, and A = B or B†.
    let (u, v) = if wide { (v_b, u_b) } else { (u_b, v_b) };
    Some(Svd {
        u: Matrix::new(rows, rank, u),
        singular_values,
        v_adjoint: MatrixRef::new(cols, rank, &v).adjoint(),
    })
}

/// `B = U·S·V†`, or `None` when the rotations do not converge. The caller
/// passes `B` with `rows ≥ cols` for speed alone, so that the rotated
/// columns are the shorter side: a wide `B` gives the same factors at more
/// cost.
///
/// The trim drops the rows of `R` chosen by their norms, not by their
/// diagonal entries and not as a prefix, because the pivoting does not keep
/// `|R_ii|` nonincreasing: on a 4 × 4 matrix from an MPS simulation of H₂,
/// t4a-faer's puts a pivot of 1.9e-16 before one of 1.3e-9, as faer 0.24's
/// did.
fn factor<K: Kernels>(b: MatrixRef<'_>, tall_svd: TallSvd<'_>) -> Option<ColumnMajorSvd> {
    let cols = b.cols();
    let qr = K::pivoted_qr(b);
    let (r_rows, r) = (qr.r.rows(), qr.r.elements());
    let at = |i: usize, j: usize| r[i + r_rows * j];
    let row_norms: Vec<f64> = (0..r_rows)
        .map(|i| (i..cols).map(|j| at(i, j).norm_sqr()).sum::<f64>().sqrt())
        .collect();
    let largest = row_norms.iter().copied().fold(0.0, f64::max);
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let tolerance = b.rows().max(cols) as f64 * f64::EPSILON * largest;
    let mut smallest_first: Vec<usize> = (0..r_rows).collect();
    smallest_first.sort_by(|&i, &j| row_norms[i].total_cmp(&row_norms[j]));
    let mut dropped = 0.0;
    let dropping = smallest_first
        .iter()
        .take_while(|&&i| {
            dropped += row_norms[i] * row_norms[i];
            dropped <= tolerance * tolerance
        })
        .count();
    let mut kept = smallest_first[dropping..].to_vec();
    kept.sort_unstable();
    let forward = &qr.forward;
    // W = (R_kept·Pᵀ)†, `cols × kept`, from (R·Pᵀ)[i, forward[j]] = R[i, j];
    // each column stored as [re…, im…].
    let mut w = vec![0.0; 2 * cols * kept.len()];
    for (column_at, &i) in kept.iter().enumerate() {
        let column = &mut w[2 * cols * column_at..2 * cols * (column_at + 1)];
        for j in i..cols {
            let x = at(i, j);
            column[forward[j]] = x.re;
            column[cols + forward[j]] = -x.im;
        }
    }
    // W = U_W·S·V_W†, so R_kept·Pᵀ = V_W·S·U_W† and B = (Q_kept·V_W)·S·U_W†.
    let by_fast_route = goes_to_fast_route(&kept, &row_norms, |i| at(i, i).norm_sqr())
        .then(|| tall_svd(cols, kept.len(), &to_complex(&w, cols)))
        .flatten()
        .filter(is_finite);
    let (u_w, singular_values, v_w) = match by_fast_route {
        Some(factors) => factors,
        None => one_sided(cols, kept.len(), w, MAX_SWEEPS)?,
    };
    let u = K::q_kept_times(&qr, &kept, &v_w, singular_values.len());
    Some((u, singular_values, u_w))
}

/// Whether the kept rows `kept` of `R` (norms `row_norms`, squared
/// diagonal entries `diagonal_sqr`) go to the fast route: at least
/// [`FAST_ROUTE_MIN_RANK`] of them, with a mean fraction of their squared
/// norm off the diagonal of at least [`FAST_ROUTE_MIN_OFF_DIAGONAL`].
fn goes_to_fast_route(
    kept: &[usize],
    row_norms: &[f64],
    diagonal_sqr: impl Fn(usize) -> f64,
) -> bool {
    if kept.len() < FAST_ROUTE_MIN_RANK {
        return false;
    }
    let off_diagonal: f64 = kept
        .iter()
        .map(|&i| 1.0 - diagonal_sqr(i) / (row_norms[i] * row_norms[i]))
        .sum();
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let mean = off_diagonal / kept.len() as f64;
    mean >= FAST_ROUTE_MIN_OFF_DIAGONAL
}

/// The `rows`-row columns of `w`, each stored as `[re…, im…]`, as complex
/// numbers, column-major.
fn to_complex(w: &[f64], rows: usize) -> Vec<Complex64> {
    w.chunks_exact(2 * rows)
        .flat_map(|column| {
            let (re, im) = column.split_at(rows);
            re.iter().zip(im).map(|(&x, &y)| Complex64::new(x, y))
        })
        .collect()
}

/// Whether every entry of the factors is finite, in both parts.
fn is_finite((u, values, v): &ColumnMajorSvd) -> bool {
    let finite = |x: &Complex64| x.re.is_finite() && x.im.is_finite();
    values.iter().all(|s| s.is_finite()) && u.iter().all(finite) && v.iter().all(finite)
}
