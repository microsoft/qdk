// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Rotation`]: the turn of two columns, in their own plane, that makes
//! them orthogonal, found from what it reads of them ([`ColumnPair`]).
//!
//! A rotation mixes the two columns `w_p` and `w_q` of its pair, `p < q`.
//! Every other column stays as it is. Here the pair is `(1, 3)`:
//!
//! ```text
//!  columns:  w₀   w₁   w₂   w₃
//!                 │         │
//!                 └────┬────┘
//!                      ▼
//!        w₁ ← c·w₁ − s·φ·w₃       c, s: the angle's cosine and sine
//!        w₃ ← s·w₁ + c·φ·w₃       φ: the phase on w₃ that makes
//!                                  w₁†(φ·w₃) real
//! ```
//!
//! [`Rotation::making_orthogonal`] finds the turn in three steps:
//!
//! 1. the phase `φ` on `w_q` that makes the inner product `w_p†(φ·w_q)`
//!    real ([`phase_making_product_real`]);
//! 2. the tangent `t` of the angle that turns the pair orthogonal
//!    ([`tangent_of_turn`]);
//! 3. the cosine and sine from `t`. The turn moves `t·|w_p† w_q|` of
//!    squared norm from `w_p` to `w_q`.
//!
//! For instance, on two real columns of length 2:
//!
//! ```text
//!  w_p = (1, 0), w_q = (1, 1)   ‖w_p‖² = 1, ‖w_q‖² = 2, w_p† w_q = 1
//!  1. φ = 1, the product already real
//!  2. ζ = (2 − 1) / (2·1) = 0.5, t = 1 / (0.5 + √1.25) ≈ 0.618
//!  3. c = 1 / √(1 + t²) ≈ 0.851, s = c·t ≈ 0.526
//!  w_p ← (0.325, −0.526)   ‖w_p‖² ≈ 0.382 = 1 − t·1
//!  w_q ← (1.376, 0.851)    ‖w_q‖² ≈ 2.618 = 2 + t·1
//!  w_p† w_q ≈ 0.447 − 0.447 = 0
//! ```

use num_complex::Complex64;

/// A plane rotation by the angle of tangent `sin / cos`, after the `phase`
/// on the second column.
pub(super) struct Rotation {
    cos: f64,
    sin: f64,
    phase: Complex64,
    /// The squared norm it moves from column `p` to column `q` of the pair
    /// it orthogonalizes: `t·|w_p† w_q|`, with `t` the tangent.
    pub(super) moved_norm_sqr: f64,
}

impl Rotation {
    /// The rotation that makes the columns `w_p` and `w_q` of `pair`
    /// orthogonal. Their squared norms are `α` and `β`. Their inner product
    /// is not 0.
    ///
    /// A phase `φ` on `w_q` first makes the product real, `|w_p† w_q|`. The
    /// pair is then a real problem in the plane of the two columns. Rotating
    /// it by an angle of tangent `t` makes it orthogonal when `t` solves
    /// `t² + 2ζt − 1 = 0`, `ζ = (β − α) / (2·|w_p† w_q|)`. That is the 2 × 2
    /// symmetric Schur decomposition of the pair's Gram matrix (Golub and
    /// Van Loan, §8.5).
    pub(super) fn making_orthogonal(pair: &ColumnPair) -> Self {
        let phase = phase_making_product_real(pair);
        let tangent = tangent_of_turn(pair);
        let cosine = 1.0 / tangent.hypot(1.0);
        Rotation {
            cos: cosine,
            sin: cosine * tangent,
            phase,
            moved_norm_sqr: tangent * pair.coupling,
        }
    }

    /// `[x_p, x_q] ← [cos·x_p − sin·φ·x_q, sin·x_p + cos·φ·x_q]` on the
    /// columns `p < q` of `columns` (`rows` long, `[re…, im…]`, column-major).
    ///
    /// Kept out of line: inlined into the sweep, its loop ran slower (an MPS
    /// simulation bench took 38% longer), and out of line it compiles on its
    /// own, as fast as before.
    #[inline(never)]
    pub(super) fn apply(&self, columns: &mut [f64], rows: usize, p: usize, q: usize) {
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

/// What a rotation reads of two columns `w_p` and `w_q`: their squared
/// norms and their inner product.
pub(super) struct ColumnPair {
    /// `‖w_p‖²`, also written `α`.
    pub(super) p_norm_sqr: f64,
    /// `‖w_q‖²`, also written `β`.
    pub(super) q_norm_sqr: f64,
    /// `w_p† w_q`, as `(re, im)`.
    pub(super) inner_product: (f64, f64),
    /// `|w_p† w_q|`: how far the pair is from orthogonal, before its norms
    /// are taken into account.
    pub(super) coupling: f64,
}

/// The phase `φ = (w_p† w_q)* / |w_p† w_q|`. On `w_q`, it makes the inner
/// product of the pair real and positive: `w_p† (φ·w_q) = |w_p† w_q|`.
fn phase_making_product_real(pair: &ColumnPair) -> Complex64 {
    let (re, im) = pair.inner_product;
    Complex64::new(re / pair.coupling, -im / pair.coupling)
}

/// The tangent of the angle that turns the pair orthogonal, once its
/// product is real: the smaller root of `t² + 2ζt − 1 = 0`,
/// `sign(ζ) / (|ζ| + √(1 + ζ²))`, with `ζ = (β − α) / (2·|w_p† w_q|)`. It
/// turns by at most 45°. This form of it does not cancel.
fn tangent_of_turn(pair: &ColumnPair) -> f64 {
    let zeta = (pair.q_norm_sqr - pair.p_norm_sqr) / (2.0 * pair.coupling);
    zeta.signum() / (zeta.abs() + zeta.hypot(1.0))
}
