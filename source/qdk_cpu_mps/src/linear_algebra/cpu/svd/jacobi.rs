// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`one_sided`]: one-sided Jacobi rotations (Hestenes), the SVD's route for
//! every split but the large ones far from orthogonal.
//!
//! The SVD (the parent module) reduces each split to a matrix `W` of full rank,
//! the adjoint of the rows of `R` its trim keeps, and needs `W`'s SVD.
//! One-sided Jacobi finds it by rotating pairs of `W`'s columns until every two
//! are orthogonal: the column norms are then the singular values. Where `W` is
//! nearly orthogonal, as on H₂'s splits, a few sweeps over the pairs suffice.
//!
//! The rotations converge on any matrix of full rank. They report a failure
//! rather than answer columns that are not yet orthogonal: when the last sweep
//! allowed ([`MAX_SWEEPS`]) still rotates a pair, the SVD fails ("did not
//! converge"). They also factor the `W` that the QR algorithm fails on.
//!
//! A sweep visits every pair `(p, q)` in cyclic order and rotates it when
//! `|w_p† w_q| > √n · ε · ‖w_p‖ · ‖w_q‖`, with `n` the length of a column
//! (LAPACK `zgesvj`'s test); the iteration stops after the first sweep that
//! rotates no pair. A pair found below the threshold is not tested again until
//! one of its columns rotates, since its product has not changed. The squared
//! column norms are updated from each rotation's angle, recomputed when the
//! update loses half of one, and recomputed once at the end. The singular
//! values are the final column norms, the left singular vectors of `W` the
//! normalized columns, and the right ones the accumulated rotations.
//!
//! The columns are stored as `[re…, im…]` rather than as interleaved complex
//! numbers, so that the products and rotations are loops on `f64` that the
//! compiler vectorizes; they multiply and add without fused multiply-add (see
//! [`dot`]).

use super::ColumnMajorSvd;
use num_complex::Complex64;

/// The sweeps the rotations allow, after which they report a failure:
/// LAPACK `zgesvj`'s cap.
pub(crate) const MAX_SWEEPS: usize = 30;

/// Orthogonalizes the `cols` columns of `w` (`rows × cols`, columns stored
/// as `[re…, im…]`, `rows ≥ cols`). Returns the column norms,
/// nonincreasing, the normalized columns (`rows × cols`) and the
/// accumulated rotations' columns (`cols × cols`) in the same order,
/// column-major, so that `W = U·diag(S)·V†`; or `None` when the last of
/// `max_sweeps` sweeps still rotates a pair.
pub(crate) fn one_sided(
    rows: usize,
    cols: usize,
    mut w: Vec<f64>,
    max_sweeps: usize,
) -> Option<ColumnMajorSvd> {
    let mut rotations = vec![0.0; 2 * cols * cols];
    for j in 0..cols {
        rotations[2 * cols * j + j] = 1.0;
    }
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let tolerance = (rows as f64).sqrt() * f64::EPSILON;
    let mut norms: Vec<f64> = (0..cols).map(|j| norm_sqr(column(&w, rows, j))).collect();
    // `clock` counts the rotations; `rotated_at[j]` is the clock at column
    // j's last rotation and `below_at[p·cols + q]` the clock when the pair
    // was last found below the threshold (0: never).
    let mut clock = 1;
    let mut rotated_at = vec![0; cols];
    let mut below_at = vec![0; cols * cols];
    let mut sweeps = 0;
    loop {
        if sweeps == max_sweeps {
            return None;
        }
        sweeps += 1;
        let mut rotated = false;
        for p in 0..cols {
            for q in p + 1..cols {
                let below = below_at[p * cols + q];
                if below != 0 && rotated_at[p] <= below && rotated_at[q] <= below {
                    continue;
                }
                let (re, im) = dot(column(&w, rows, p), column(&w, rows, q));
                let coupling = re.hypot(im);
                let (alpha, beta) = (norms[p], norms[q]);
                // False when `coupling` is NaN too, so that the iteration
                // ends.
                if coupling > tolerance * alpha.sqrt() * beta.sqrt() {
                    rotated = true;
                    // A phase on w_q makes w_p† w_q real; then the rotation
                    // by the smaller root t of t² + 2ζt − 1 = 0 makes the
                    // pair orthogonal.
                    let zeta = (beta - alpha) / (2.0 * coupling);
                    let tan = zeta.signum() / (zeta.abs() + zeta.hypot(1.0));
                    let cosine = 1.0 / tan.hypot(1.0);
                    let rotation = Rotation {
                        cos: cosine,
                        sin: cosine * tan,
                        phase: Complex64::new(re / coupling, -im / coupling),
                    };
                    rotation.apply(&mut w, rows, p, q);
                    rotation.apply(&mut rotations, cols, p, q);
                    // The rotation moves t·|w_p† w_q| of squared norm from
                    // p to q; recompute a norm the update halves, whose
                    // update has lost accuracy.
                    let (to_p, to_q) = (alpha - tan * coupling, beta + tan * coupling);
                    norms[p] = if to_p >= 0.5 * alpha {
                        to_p
                    } else {
                        norm_sqr(column(&w, rows, p))
                    };
                    norms[q] = if to_q >= 0.5 * beta {
                        to_q
                    } else {
                        norm_sqr(column(&w, rows, q))
                    };
                    clock += 1;
                    rotated_at[p] = clock;
                    rotated_at[q] = clock;
                } else {
                    below_at[p * cols + q] = clock;
                }
            }
        }
        if !rotated {
            break;
        }
    }
    for (j, norm) in norms.iter_mut().enumerate() {
        *norm = norm_sqr(column(&w, rows, j));
    }
    let mut order: Vec<usize> = (0..cols).collect();
    order.sort_by(|&i, &j| norms[j].total_cmp(&norms[i]));
    let values: Vec<f64> = order.iter().map(|&j| norms[j].sqrt()).collect();
    let mut u = Vec::with_capacity(rows * order.len());
    let mut sorted_v = Vec::with_capacity(cols * order.len());
    for (&j, &value) in order.iter().zip(&values) {
        let (re, im) = column(&w, rows, j).split_at(rows);
        u.extend(
            re.iter()
                .zip(im)
                .map(|(x, y)| Complex64::new(x / value, y / value)),
        );
        let (re, im) = column(&rotations, cols, j).split_at(cols);
        sorted_v.extend(re.iter().zip(im).map(|(x, y)| Complex64::new(*x, *y)));
    }
    Some((u, values, sorted_v))
}

/// Column `j` of `m` (`rows` long), as `[re…, im…]`.
fn column(m: &[f64], rows: usize, j: usize) -> &[f64] {
    &m[2 * rows * j..2 * rows * (j + 1)]
}

/// `x† y` as `(re, im)`, with four accumulators of each part, so that the
/// additions do not wait on each other.
///
/// The kernels multiply and add without fused multiply-add, which some
/// targets lack, so that the rotations round alike on every target.
/// t4a-faer's kernels, which compute the pivoted QR before them, the QR
/// algorithm and the engine's products, may use it where the target has
/// it.
fn dot(x: &[f64], y: &[f64]) -> (f64, f64) {
    let len = x.len() / 2;
    let (x_re, x_im) = x.split_at(len);
    let (y_re, y_im) = y.split_at(len);
    let mut re = [0.0; 4];
    let mut im = [0.0; 4];
    let chunks = (x_re.chunks_exact(4).zip(x_im.chunks_exact(4)))
        .zip(y_re.chunks_exact(4).zip(y_im.chunks_exact(4)));
    for ((a_re, a_im), (b_re, b_im)) in chunks {
        for k in 0..4 {
            re[k] = a_re[k] * b_re[k] + (a_im[k] * b_im[k] + re[k]);
            im[k] = a_re[k] * b_im[k] + (-a_im[k] * b_re[k] + im[k]);
        }
    }
    for i in len / 4 * 4..len {
        re[0] = x_re[i] * y_re[i] + (x_im[i] * y_im[i] + re[0]);
        im[0] = x_re[i] * y_im[i] + (-x_im[i] * y_re[i] + im[0]);
    }
    (
        (re[0] + re[1]) + (re[2] + re[3]),
        (im[0] + im[1]) + (im[2] + im[3]),
    )
}

/// `‖x‖²`, with eight accumulators.
fn norm_sqr(x: &[f64]) -> f64 {
    let mut sums = [0.0; 8];
    let chunks = x.chunks_exact(8);
    let rest = chunks.remainder();
    for a in chunks {
        for k in 0..8 {
            sums[k] += a[k] * a[k];
        }
    }
    for a in rest {
        sums[0] += a * a;
    }
    ((sums[0] + sums[1]) + (sums[2] + sums[3])) + ((sums[4] + sums[5]) + (sums[6] + sums[7]))
}

/// A plane rotation by the angle of tangent `sin / cos`, after the `phase`
/// on the second column.
struct Rotation {
    cos: f64,
    sin: f64,
    phase: Complex64,
}

impl Rotation {
    /// `[x_p, x_q] ← [cos·x_p − sin·φ·x_q, sin·x_p + cos·φ·x_q]` on the
    /// columns `p < q` of `columns` (`rows` long, `[re…, im…]`, column-major).
    fn apply(&self, columns: &mut [f64], rows: usize, p: usize, q: usize) {
        let (head, tail) = columns.split_at_mut(2 * rows * q);
        let (p_re, p_im) = head[2 * rows * p..2 * rows * (p + 1)].split_at_mut(rows);
        let (q_re, q_im) = tail[..2 * rows].split_at_mut(rows);
        let (cos, sin) = (self.cos, self.sin);
        let (s_re, s_im) = (sin * self.phase.re, sin * self.phase.im);
        let (c_re, c_im) = (cos * self.phase.re, cos * self.phase.im);
        let pairs =
            (p_re.iter_mut().zip(p_im.iter_mut())).zip(q_re.iter_mut().zip(q_im.iter_mut()));
        for ((x_re, x_im), (y_re, y_im)) in pairs {
            let (a_re, a_im, b_re, b_im) = (*x_re, *x_im, *y_re, *y_im);
            *x_re = cos * a_re + (-s_re * b_re + s_im * b_im);
            *x_im = cos * a_im + (-s_re * b_im - s_im * b_re);
            *y_re = sin * a_re + (c_re * b_re - c_im * b_im);
            *y_im = sin * a_im + (c_re * b_im + c_im * b_re);
        }
    }
}
