// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`HouseholderReflectors::pivoted_qr`]: the column-pivoted QR
//! decomposition by Householder reflections, whose `Q` is the
//! [`HouseholderReflectors`].
//!
//! The SVD's trim keeps the rows of `R` above rounding's size, which
//! leaves the rotations a matrix of full rank only when `R` reveals the
//! matrix's rank: at each step, the pivot is the remaining column of
//! largest norm (Businger and Golub; Golub and Van Loan, *Matrix
//! Computations*, 4th ed., Algorithm 5.4.1), so that `|R_kk|` is at least
//! the norm of every later column of `R` from row `k` on, to within the
//! downdate's allowance, below.
//!
//! It is unblocked: each step reflects every remaining column at once,
//! with plain loops. The column norms are downdated after each step and
//! recomputed once cancellation has eaten into them, so that no other
//! column's norm exceeds the pivot's by more than about `√ε/2` (7e-9) of
//! it. Its squared norms underflow for entries far below the largest,
//! which it may then round to 0: the SVD calls it on scaled input, where
//! what is lost is far below the checks' bound. Each reflector's `τ` is
//! computed from its stored vector, so `Q` stays orthonormal there too.

use super::super::PivotedQr;
use crate::dense_linalg::matrix::{Matrix, MatrixRef};
use num_complex::Complex64;

/// The `Q` of a QR decomposition as its Householder reflectors,
/// `Q = H₀·H₁·…`, stored in place: `R` on and above the diagonal, each
/// reflector's vector `v` below it (`v[0] = 1` implied), and their `τ`s.
pub(crate) struct HouseholderReflectors {
    rows: usize,
    cols: usize,
    f: Vec<Complex64>,
    tau: Vec<f64>,
}

impl HouseholderReflectors {
    /// `a·P = Q·R`, thin, for an `a` with elements; the pivot of each step
    /// is the remaining column of largest norm (the module doc), the last
    /// of those of equal norm (LAPACK's `idamax` takes the first; either
    /// keeps the rule).
    ///
    /// The squared column norms are downdated after each step, and
    /// recomputed from the column once the downdated one has fallen to `√ε`
    /// times the one last computed (LAPACK `zlaqp2`'s rule): below it, the
    /// downdate has lost half its digits to cancellation.
    pub(crate) fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<Self> {
        let (rows, cols) = (a.rows(), a.cols());
        let mut f = a.elements().to_vec();
        let mut forward: Vec<usize> = (0..cols).collect();
        let mut norms: Vec<f64> = f.chunks_exact(rows).map(squared_norm).collect();
        let mut reference = norms.clone();
        let steps = rows.min(cols);
        let mut tau = vec![0.0; steps];
        let recompute_below = f64::EPSILON.sqrt();
        for k in 0..steps {
            let pivot = (k..cols)
                .max_by(|&i, &j| norms[i].total_cmp(&norms[j]))
                .expect("a column remains, since k < cols");
            if pivot != k {
                let (left, right) = f.split_at_mut(rows * pivot);
                left[rows * k..rows * (k + 1)].swap_with_slice(&mut right[..rows]);
                forward.swap(k, pivot);
                norms.swap(k, pivot);
                reference.swap(k, pivot);
            }
            tau[k] = reflector(&mut f[rows * k + k..rows * (k + 1)]);
            reflect_later_columns(&mut f, rows, k, tau[k]);
            for j in k + 1..cols {
                if norms[j] == 0.0 {
                    continue;
                }
                norms[j] -= f[k + rows * j].norm_sqr();
                if norms[j] <= recompute_below * reference[j] {
                    norms[j] = squared_norm(&f[rows * j + k + 1..rows * (j + 1)]);
                    reference[j] = norms[j];
                }
            }
        }
        let q = Self { rows, cols, f, tau };
        PivotedQr {
            r: q.r(),
            forward,
            q,
        }
    }

    /// `Q_kept·x`: `x` (`kept.len() × rank`) scattered into the rows `kept`
    /// of a `rows × rank` zero matrix, then `Q` applied. The reflectors
    /// after the last kept row leave that matrix unchanged, so they are not
    /// applied.
    pub(crate) fn kept_times(
        &self,
        kept: &[usize],
        x: &[Complex64],
        rank: usize,
    ) -> Vec<Complex64> {
        let rows = self.rows;
        let mut out = vec![Complex64::new(0.0, 0.0); rows * rank];
        for j in 0..rank {
            for (at, &i) in kept.iter().enumerate() {
                out[i + rows * j] = x[at + kept.len() * j];
            }
        }
        let applied = kept.last().map_or(0, |&i| i + 1);
        for k in (0..applied).rev() {
            let tau = self.tau[k];
            if tau == 0.0 {
                continue;
            }
            let v = &self.f[rows * k + k..rows * (k + 1)];
            for column in out.chunks_exact_mut(rows) {
                reflect(v, tau, &mut column[k..]);
            }
        }
        out
    }

    /// `R`, `min(rows, cols) × cols`, upper trapezoidal.
    fn r(&self) -> Matrix {
        let (rows, cols) = (self.rows, self.cols);
        Matrix::from_fn(rows.min(cols), cols, |i, j| {
            if i <= j {
                self.f[i + rows * j]
            } else {
                Complex64::new(0.0, 0.0)
            }
        })
    }
}

/// Makes `x` the reflector that maps it to `β·e₀`: on return `x[0] = β`
/// and `x[1..]` holds `v[1..]` (`v[0] = 1`); returns `τ`, 0 when `x[1..]`
/// is already zero (then `H = I`). With `θ` the phase of `x[0]`,
/// `β = −e^{iθ}·‖x‖`, of the sign that adds `x[0]` and `‖x‖` in
/// `v[0] = x[0] − β` rather than subtract them, which would cancel.
fn reflector(x: &mut [Complex64]) -> f64 {
    let tail: f64 = x[1..].iter().map(Complex64::norm_sqr).sum();
    if tail == 0.0 {
        return 0.0;
    }
    let alpha = x[0];
    let norm = (alpha.norm_sqr() + tail).sqrt();
    let phase = if alpha == Complex64::new(0.0, 0.0) {
        Complex64::new(1.0, 0.0)
    } else {
        alpha / alpha.norm()
    };
    let v0 = phase * (alpha.norm() + norm);
    let inverse = v0.inv();
    for z in &mut x[1..] {
        *z *= inverse;
    }
    x[0] = -phase * norm;
    // From the stored `v`, so that `H` is unitary however `v` was rounded:
    // `tail / |v0|²` loses its digits once `|v0|²` is subnormal.
    2.0 / (1.0 + squared_norm(&x[1..]))
}

/// `a ← H·a = a − τ·v·(v†a)` for one column `a`, as long as `v`
/// (`v[0] = 1` implied).
fn reflect(v: &[Complex64], tau: f64, a: &mut [Complex64]) {
    let mut re = a[0].re;
    let mut im = a[0].im;
    for (vi, ai) in v[1..].iter().zip(&a[1..]) {
        re += vi.re * ai.re + vi.im * ai.im;
        im += vi.re * ai.im - vi.im * ai.re;
    }
    let w = Complex64::new(tau * re, tau * im);
    a[0] -= w;
    for (vi, ai) in v[1..].iter().zip(&mut a[1..]) {
        ai.re -= w.re * vi.re - w.im * vi.im;
        ai.im -= w.re * vi.im + w.im * vi.re;
    }
}

/// The columns after `k` of the `rows`-row `f`, rows `k..`, reflected by
/// the reflector stored in column `k`, rows `k..`.
fn reflect_later_columns(f: &mut [Complex64], rows: usize, k: usize, tau: f64) {
    if tau == 0.0 {
        return;
    }
    let (head, tail) = f.split_at_mut(rows * (k + 1));
    let v = &head[rows * k + k..];
    for column in tail.chunks_exact_mut(rows) {
        reflect(v, tau, &mut column[k..]);
    }
}

fn squared_norm(x: &[Complex64]) -> f64 {
    x.iter().map(Complex64::norm_sqr).sum()
}
