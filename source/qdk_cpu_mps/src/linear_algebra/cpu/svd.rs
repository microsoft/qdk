// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The SVD that splits a pair of sites: [`trimmed_thin_svd`] computes it, and
//! [`checks::checked`] takes or refuses its answer.
//!
//! Each split factors a contracted pair by SVD and truncates by the singular
//! values (`engine::split`): the truncation trusts the values, and the
//! canonical form, on which every read at the centre relies, trusts the
//! singular vectors to be orthonormal. The splits' matrices are rank-deficient
//! most of the time (they have fewer nonzero singular values than their smaller
//! side), with repeated values, and general SVDs have returned wrong factors on
//! such matrices without reporting a failure: faer 0.24's, on degenerate and
//! rank-deficient matrices, and t4a-faer's default method, divide and conquer,
//! on a 128 × 128 split of a polyene's chain (README.md, "Linear algebra").
//!
//! So the SVD guarantees, for `CpuLinearAlgebra::svd`:
//!
//! - **Checked factors.** It answers only factors that give back the matrix and
//!   singular vectors that are orthonormal, to the bounds of LAPACK's SVD tests
//!   ([`checks`]); otherwise, and when the rotations still turn after their
//!   last sweep, the split fails with a `FactorizationError`. So a split never
//!   spreads values that are not finite, nor factors whose product is not the
//!   matrix, through the state.
//! - **Only rounding left out.** It leaves out the singular values of rounding:
//!   those of the rows of the pivoted QR's `R` of smallest norm, as many as
//!   keep the dropped rows' Frobenius norm within `max(m, n) · ε` times the
//!   largest row's; rows that are exactly zero are always among them. It
//!   returns one value, 0, for the zero matrix.
//! - **Any scale.** It answers at any scale, down to subnormal entries, short
//!   of the matrices whose singular values are too small or too large to store,
//!   which the checks refuse.
//!
//! It runs these steps in order:
//!
//! 1. The zero matrix gets one value, 0, and the first unit vectors: it has no
//!    rank for the trim to keep and no largest entry to scale by.
//! 2. A wide matrix is factored through its adjoint, so that the rotated
//!    columns have length `min(m, n)`.
//! 3. A matrix whose largest entry is outside `[2⁻¹⁰⁰, 2¹⁰⁰]` is scaled by a
//!    power of two, so that the squared column norms neither overflow nor
//!    underflow, and its singular values are scaled back at the end
//!    ([`trimmed_thin_svd`] says when that rounds).
//! 4. The trim (Drmač's preconditioning): t4a-faer's column-pivoted QR, `A·P =
//!    Q·R`, whose rows of `R` of smallest norm are dropped, as many as keep the
//!    dropped rows' Frobenius norm within `max(m, n) · ε` times the largest
//!    row's norm. So the trim changes the matrix by no more than rounding does,
//!    whatever its size, and the factorization after it sees full rank. The
//!    rows are chosen by their norms ([`factor`] says why).
//! 5. The route: `W`, the adjoint of the kept rows of `R·Pᵀ`, goes to
//!    t4a-faer's QR algorithm ([`qr_algorithm`]) when the trim keeps at least
//!    [`QR_ALGORITHM_MIN_RANK`] rows and they have, on average, at least
//!    [`QR_ALGORITHM_MIN_OFF_DIAGONAL`] of their squared norm off the diagonal,
//!    the sign that `W` is far from orthogonal; every other `W` goes to the
//!    one-sided Jacobi rotations ([`jacobi`]), as does one the QR algorithm
//!    reports a failure on or answers factors that are not finite for. The
//!    route changes the cost, not what is computed: both give the SVD of `W`,
//!    checked alike.
//! 6. The left singular vectors are the kept columns of `Q` times `W`'s right
//!    ones, computed by applying the reflectors rather than by forming `Q`.
//!
//! The route's constants are performance choices (measured), not promises: the
//! tests pin that every route gives a checked SVD and that the fallback
//! answers, not where the route changes.

pub(crate) mod checks;
pub(crate) mod jacobi;
pub(crate) mod qr_algorithm;

use self::jacobi::{MAX_SWEEPS, one_sided};
use super::to_faer;
use crate::linear_algebra::{Matrix, MatrixRef, Svd, adjoint};
use num_complex::Complex64;
use t4a_faer::{
    Conj, MatMut, MatRef, Par,
    dyn_stack::{MemBuffer, MemStack},
    linalg::householder::{
        apply_block_householder_sequence_on_the_left_in_place_scratch,
        apply_block_householder_sequence_on_the_left_in_place_with_conj,
    },
};

/// The smallest kept rank whose `W` may go to the QR algorithm: below it,
/// giving every `W` to the QR algorithm saved at most 3% of an SVD's time,
/// and cost 1.1 to 1.3 times the rotations' on small graded and
/// rank-deficient matrices (measured).
pub(crate) const QR_ALGORITHM_MIN_RANK: usize = 32;

/// The smallest mean fraction `1 − |R_ii|²/‖R_i‖²` of the kept rows of `R`
/// whose `W` goes to the QR algorithm: below it, as on H₂'s splits, the
/// rotations end within five sweeps and are as fast (measured).
pub(crate) const QR_ALGORITHM_MIN_OFF_DIAGONAL: f64 = 0.1;

/// An SVD as column-major arrays, `(U, S, V)` with `W = U·diag(S)·V†`,
/// the values nonincreasing: `U` is `rows × r` and `V` is `cols × r`,
/// `W` being `rows × cols`. Both routes answer one.
pub(crate) type ColumnMajorSvd = (Vec<Complex64>, Vec<f64>, Vec<Complex64>);

/// The route for the splits far from orthogonal: a thin SVD of a tall
/// matrix, `(rows, cols, w)` with `w` column-major and `rows ≥ cols`,
/// giving `r = cols` values; `None` when it reports a failure.
///
/// [`trimmed_thin_svd`] takes it as a parameter for the fallback's tests
/// alone: no matrix is known on which t4a-faer's QR algorithm fails or
/// answers values that are not finite, so the tests pass one that does.
pub(crate) type TallSvd<'f> = &'f dyn Fn(usize, usize, &[Complex64]) -> Option<ColumnMajorSvd>;

/// The thin SVD of `a` after a pivoted-QR trim, by `qr_algorithm` on the
/// splits the module doc routes to it and by the rotations otherwise, or
/// `None` when the rotations' last sweep allowed ([`MAX_SWEEPS`]) still
/// rotates a pair. `CpuLinearAlgebra` passes
/// [`tall_svd_by_qr_algorithm`](qr_algorithm::tall_svd_by_qr_algorithm).
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
pub(crate) fn trimmed_thin_svd(a: MatrixRef<'_>, qr_algorithm: TallSvd<'_>) -> Option<Svd> {
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
            v_adjoint: adjoint(first(cols).view()),
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
            adjoint(a)
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
    let (u_b, mut singular_values, v_b) = factor(b, qr_algorithm)?;
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
        v_adjoint: adjoint(MatrixRef::new(cols, rank, &v)),
    })
}

/// `B = U·S·V†`, or `None` when the rotations do not converge. The caller
/// passes `B` with `rows ≥ cols` for speed alone, so that the rotated
/// columns are the shorter side: a wide `B` gives the same factors at more
/// cost.
///
/// The trim drops the rows of `R` chosen by their norms, not by their
/// diagonal entries and not as a prefix, because the pivoting does not keep
/// `|R_ii|` nonincreasing: on a 4 × 4 matrix cut from a split of an H₂
/// shot, t4a-faer's puts a pivot of 1.9e-16 before one of 1.3e-9, as faer
/// 0.24's did.
fn factor(b: MatrixRef<'_>, qr_algorithm: TallSvd<'_>) -> Option<ColumnMajorSvd> {
    let (rows, cols) = (b.rows(), b.cols());
    let qr = to_faer(b).col_piv_qr(Par::Seq);
    let r = qr.thin_R();
    let row_norms: Vec<f64> = (0..r.nrows())
        .map(|i| (i..cols).map(|j| r[(i, j)].norm_sqr()).sum::<f64>().sqrt())
        .collect();
    let largest = row_norms.iter().copied().fold(0.0, f64::max);
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let tolerance = rows.max(cols) as f64 * f64::EPSILON * largest;
    let mut smallest_first: Vec<usize> = (0..r.nrows()).collect();
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
    let (forward, _) = qr.P().arrays();
    // W = (R_kept·Pᵀ)†, `cols × kept`, from (R·Pᵀ)[i, forward[j]] = R[i, j];
    // each column stored as [re…, im…].
    let mut w = vec![0.0; 2 * cols * kept.len()];
    for (at, &i) in kept.iter().enumerate() {
        let column = &mut w[2 * cols * at..2 * cols * (at + 1)];
        for j in i..cols {
            let x = r[(i, j)];
            column[forward[j]] = x.re;
            column[cols + forward[j]] = -x.im;
        }
    }
    // W = U_W·S·V_W†, so R_kept·Pᵀ = V_W·S·U_W† and B = (Q_kept·V_W)·S·U_W†.
    let by_qr_algorithm = goes_to_qr_algorithm(&kept, &row_norms, |i| r[(i, i)].norm_sqr())
        .then(|| qr_algorithm(cols, kept.len(), &to_complex(&w, cols)))
        .flatten()
        .filter(is_finite);
    let (u_w, singular_values, v_w) = match by_qr_algorithm {
        Some(factors) => factors,
        None => one_sided(cols, kept.len(), w, MAX_SWEEPS)?,
    };
    let u = q_kept_times(
        qr.Q_basis(),
        qr.Q_coeff(),
        &kept,
        &v_w,
        rows,
        singular_values.len(),
    );
    Some((u, singular_values, u_w))
}

/// Whether the kept rows `kept` of `R` (norms `row_norms`, squared
/// diagonal entries `diagonal_sqr`) go to the QR algorithm: at least
/// [`QR_ALGORITHM_MIN_RANK`] of them, with a mean fraction of their squared
/// norm off the diagonal of at least [`QR_ALGORITHM_MIN_OFF_DIAGONAL`].
fn goes_to_qr_algorithm(
    kept: &[usize],
    row_norms: &[f64],
    diagonal_sqr: impl Fn(usize) -> f64,
) -> bool {
    if kept.len() < QR_ALGORITHM_MIN_RANK {
        return false;
    }
    let off_diagonal: f64 = kept
        .iter()
        .map(|&i| 1.0 - diagonal_sqr(i) / (row_norms[i] * row_norms[i]))
        .sum();
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let mean = off_diagonal / kept.len() as f64;
    mean >= QR_ALGORITHM_MIN_OFF_DIAGONAL
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

/// `Q_kept·X`, with `Q` the Householder sequence `basis`, `coeff` (`rows`
/// rows), `Q_kept` its columns `kept` (increasing) and `X` `kept.len() ×
/// rank`, column-major: the reflectors applied to `X` scattered into the
/// rows `kept` of a `rows × rank` zero matrix. The reflectors after the
/// last kept row leave that matrix unchanged, so they are not applied.
fn q_kept_times(
    basis: MatRef<'_, Complex64>,
    coeff: MatRef<'_, Complex64>,
    kept: &[usize],
    x: &[Complex64],
    rows: usize,
    rank: usize,
) -> Vec<Complex64> {
    let mut out = vec![Complex64::new(0.0, 0.0); rows * rank];
    for j in 0..rank {
        for (at, &i) in kept.iter().enumerate() {
            out[i + rows * j] = x[at + kept.len() * j];
        }
    }
    let reflectors = kept.last().map_or(0, |&i| i + 1);
    let (basis, coeff) = (basis.subcols(0, reflectors), coeff.subcols(0, reflectors));
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
