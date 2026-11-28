// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`HouseholderReflectors`]: the `Q` of a QR decomposition, kept as the
//! product of Householder reflectors, `Q = H₀·H₁·…`, one per reduced
//! column, rather than as a matrix. The reflectors fit in the input's own
//! storage, and `Q` is formed only as far as a caller needs. The module
//! also holds the two QR decompositions that make it: the thin QR ([`qr`])
//! and the column-pivoted QR
//! ([`pivoted_qr`](HouseholderReflectors::pivoted_qr)). Both reduce each
//! column by one step
//! ([`reduce_column`](HouseholderReflectors::reduce_column)).
//!
//! Step `k` reduces column `k`. A reflector maps the column's rows `k..` to
//! `(β, 0, …, 0)`, which zeroes the column below the diagonal. The same
//! reflector is applied to every later column, which gives row `k` of `R`.
//! The reflector's vector is stored where the zeros would be. Here, step 1
//! of a 5 × 4 matrix:
//!
//! ```text
//!  r: an entry of R, final   v: a reflector's vector   x: not yet reduced
//!
//!   before step 1          after step 1
//!   r  r  r  r             r  r  r  r
//!   v  x  x  x             v  r  r  r    ← row 1 of R, final
//!   v  x  x  x     ──▶     v  v  x  x
//!   v  x  x  x             v  v  x  x
//!   v  x  x  x             v  v  x  x
//!      ↑                      ↑
//!      column 1               reflector 1's vector, below the diagonal
//! ```
//!
//! A reflector is `H = I − τ·v·v†`, of vector `v` (`v[0] = 1`) and scale
//! `τ`. It maps a column `x` to `(β, 0, …, 0)`, with `|β| = ‖x‖`
//! ([`turn_into_reflector`]). Their product is `Q`. It is never formed
//! beyond the columns asked for:
//! [`thin_q`](HouseholderReflectors::thin_q) and
//! [`kept_times`](HouseholderReflectors::kept_times) apply the reflectors,
//! last first, to the columns they answer.
//!
//! The SVD's trim drops the shortest rows of `R`, as many as stay jointly
//! within rounding. That leaves the rotations a matrix of full rank only
//! when `R` reveals the matrix's rank. So at each step, the pivot is the
//! remaining column of largest norm (Businger and Golub's rule, Further
//! reading). Then, to within the downdate's allowance (below), `|R_kk|` is
//! at least the norm of every later column of `R` from row `k` on.
//! The Terms of `docs/algorithms.md` define the words pivot, reflector,
//! norm, `ε` and cancellation.
//!
//! Each step `k` of [`pivoted_qr`](HouseholderReflectors::pivoted_qr):
//!
//! 1. picks the pivot, the remaining column of largest norm
//!    ([`ColumnNorms::largest_from`]), and swaps it into place `k`;
//! 2. reduces column `k`, giving row `k` of `R`;
//! 3. updates the later columns' norms for what row `k` took from them
//!    ([`ColumnNorms::leave_out_row`]), so that the next pivot is picked
//!    without measuring every column again.
//!
//! The thin QR takes step 2 alone.
//!
//! Both are unblocked: each step reflects every remaining column at once,
//! with plain loops. The pivoted QR's column norms are downdated after each
//! step. They are recomputed once cancellation has eaten into them. So no
//! other column's norm exceeds the pivot's by more than about `√ε/2`
//! (7e-9) of it. Their squared norms underflow for entries far below the
//! largest, which they may then round to 0. The SVD and `Qr::of` call them
//! on scaled input, where what is lost is far below rounding of the
//! largest entry. Each reflector's `τ` is computed from its stored vector,
//! so `Q` stays orthonormal there too.
//!
//! # Further reading
//!
//! - P. Businger and G. H. Golub, "Linear least squares solutions by
//!   Householder transformations", Numer. Math. 7:269–276, 1965. It gives
//!   the column pivoting by largest norm.
//! - G. H. Golub and C. F. Van Loan, *Matrix Computations*, 4th ed., Johns
//!   Hopkins University Press, 2013.
//!   - §5.1, "Householder and Givens Transformations", covers the
//!     reflections.
//!   - §5.4, "Other Orthogonal Factorizations", covers QR with column
//!     pivoting.

use super::super::PivotedQr;
use crate::matrix::{Matrix, MatrixRef};
use crate::norms::squared_norm;
use crate::qr::Qr;
use num_complex::Complex64;

/// The `Q` of a QR decomposition as its Householder reflectors,
/// `Q = H₀·H₁·…`, stored in place:
///
/// - `R` on and above the diagonal;
/// - each reflector's vector `v` below it (`v[0] = 1` implied);
/// - their scales (`τ`).
pub(crate) struct HouseholderReflectors {
    rows: usize,
    cols: usize,
    /// The matrix, reduced in place. The columns not yet reduced hold what
    /// the earlier steps' reflectors left of them.
    in_place: Vec<Complex64>,
    /// Each reflector's scale, `τ` in `H = I − τ·v·v†` (Golub and Van
    /// Loan's notation), 0 for the identity.
    reflector_scales: Vec<f64>,
}

/// `a = Q·R`, thin, for an `a` with elements: the steps of the module
/// doc without the pivoting, so step 2 alone.
pub(crate) fn qr(a: MatrixRef<'_>) -> Qr {
    let mut reflectors = HouseholderReflectors::unreduced(a);
    for k in 0..reflectors.steps() {
        reflectors.reduce_column(k);
    }
    Qr {
        q: reflectors.thin_q(),
        r: reflectors.r(),
    }
}

impl HouseholderReflectors {
    /// `a·P = Q·R`, thin, for an `a` with elements, by the steps of the
    /// module doc. The pivot of each step is the remaining column of
    /// largest norm, the last of those of equal norm. Taking the first
    /// would keep the rule as well.
    pub(crate) fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<Self> {
        let mut reflectors = Self::unreduced(a);
        let mut forward: Vec<usize> = (0..a.cols()).collect();
        let mut norms = ColumnNorms::of(&reflectors.in_place, reflectors.rows);
        for k in 0..reflectors.steps() {
            let pivot = norms.largest_from(k);
            if pivot != k {
                reflectors.swap_columns(k, pivot);
                forward.swap(k, pivot);
                norms.swap(k, pivot);
            }
            reflectors.reduce_column(k);
            norms.leave_out_row(&reflectors.in_place, reflectors.rows, k);
        }
        PivotedQr {
            r: reflectors.r(),
            forward,
            q: reflectors,
        }
    }

    /// `a`, with no column reduced yet.
    fn unreduced(a: MatrixRef<'_>) -> Self {
        let (rows, cols) = (a.rows(), a.cols());
        Self {
            rows,
            cols,
            in_place: a.elements().to_vec(),
            reflector_scales: vec![0.0; rows.min(cols)],
        }
    }

    /// The number of steps, one per reflector: `min(rows, cols)`.
    fn steps(&self) -> usize {
        self.reflector_scales.len()
    }

    /// Swaps the columns `i < j`, neither reduced yet.
    fn swap_columns(&mut self, i: usize, j: usize) {
        let rows = self.rows;
        let (left, right) = self.in_place.split_at_mut(rows * j);
        left[rows * i..rows * (i + 1)].swap_with_slice(&mut right[..rows]);
    }

    /// Step `k` of the module doc: turns column `k` into reflector `k`,
    /// which zeroes it below the diagonal, and applies the reflector to
    /// every later column. Row `k` of `R` is then final.
    fn reduce_column(&mut self, k: usize) {
        let rows = self.rows;
        self.reflector_scales[k] =
            turn_into_reflector(&mut self.in_place[rows * k + k..rows * (k + 1)]);
        let (reduced, later) = self.in_place.split_at_mut(rows * (k + 1));
        let reflector = Reflector {
            first_row: k,
            vector: &reduced[rows * k + k..],
            scale: self.reflector_scales[k],
        };
        reflector.reflect_columns(later, rows);
    }

    /// Reflector `k`, once its column is reduced.
    fn reflector(&self, k: usize) -> Reflector<'_> {
        let rows = self.rows;
        Reflector {
            first_row: k,
            vector: &self.in_place[rows * k + k..rows * (k + 1)],
            scale: self.reflector_scales[k],
        }
    }

    /// The first `min(rows, cols)` columns of `Q`: the reflectors applied,
    /// last first, to those of the identity. Reflector `k` changes only rows
    /// `k..`, where the columns before `k` are still zero, so it is applied
    /// to the columns from `k` on.
    fn thin_q(&self) -> Matrix {
        let (rows, steps) = (self.rows, self.steps());
        let mut q = Matrix::from_fn(rows, steps, |i, j| {
            Complex64::new(if i == j { 1.0 } else { 0.0 }, 0.0)
        })
        .into_elements();
        for k in (0..steps).rev() {
            self.reflector(k).reflect_columns(&mut q[rows * k..], rows);
        }
        Matrix::new(rows, steps, q)
    }

    /// `Q_kept·x`. `x` (`kept.len() × rank`) is scattered into the rows
    /// `kept` of a `rows × rank` zero matrix, then `Q` is applied. The
    /// reflectors after the last kept row leave that matrix unchanged, so
    /// they are not applied.
    pub(crate) fn kept_times(
        &self,
        kept: &[usize],
        x: &[Complex64],
        rank: usize,
    ) -> Vec<Complex64> {
        let mut out = scattered_into_rows(self.rows, kept, x, rank);
        let reflectors_that_change_it = kept.last().map_or(0, |&i| i + 1);
        for k in (0..reflectors_that_change_it).rev() {
            self.reflector(k).reflect_columns(&mut out, self.rows);
        }
        out
    }

    /// `R`, `min(rows, cols) × cols`, upper trapezoidal.
    fn r(&self) -> Matrix {
        let (rows, cols) = (self.rows, self.cols);
        Matrix::from_fn(rows.min(cols), cols, |i, j| {
            if i <= j {
                self.in_place[i + rows * j]
            } else {
                Complex64::new(0.0, 0.0)
            }
        })
    }
}

/// `x` (`kept.len() × rank`, column-major) in the rows `kept` of a
/// `rows × rank` zero matrix.
fn scattered_into_rows(
    rows: usize,
    kept: &[usize],
    x: &[Complex64],
    rank: usize,
) -> Vec<Complex64> {
    let mut out = vec![Complex64::new(0.0, 0.0); rows * rank];
    for j in 0..rank {
        for (at, &i) in kept.iter().enumerate() {
            out[i + rows * j] = x[at + kept.len() * j];
        }
    }
    out
}

/// One reflector `H = I − τ·v·v†`, as stored. It changes the rows
/// `first_row..` of a column, and leaves the rows above.
struct Reflector<'a> {
    first_row: usize,
    /// `v`, as long as the rows it changes. `v[0] = 1` is implied: its
    /// place holds `R`'s diagonal entry.
    vector: &'a [Complex64],
    /// `τ`, the scale, 0 for the identity.
    scale: f64,
}

impl Reflector<'_> {
    /// Reflects each `rows`-long column of `columns`.
    fn reflect_columns(&self, columns: &mut [Complex64], rows: usize) {
        if self.scale == 0.0 {
            return;
        }
        for column in columns.chunks_exact_mut(rows) {
            self.reflect(&mut column[self.first_row..]);
        }
    }

    /// `a ← H·a = a − τ·v·(v†a)` for one column `a`, as long as `v`.
    fn reflect(&self, a: &mut [Complex64]) {
        let (v, tau) = (self.vector, self.scale);
        let mut re = a[0].re;
        let mut im = a[0].im;
        for (vi, ai) in v[1..].iter().zip(&a[1..]) {
            re += vi.re * ai.re + vi.im * ai.im;
            im += vi.re * ai.im - vi.im * ai.re;
        }
        // `τ·(v†a)`, the multiple of `v` taken from `a`.
        let correction = Complex64::new(tau * re, tau * im);
        a[0] -= correction;
        for (vi, ai) in v[1..].iter().zip(&mut a[1..]) {
            ai.re -= correction.re * vi.re - correction.im * vi.im;
            ai.im -= correction.re * vi.im + correction.im * vi.re;
        }
    }
}

/// The squared norms of the columns not yet pivoted, from row `k` down at
/// step `k`. They pick each step's pivot.
///
/// After step `k`, each later column's squared norm loses the squared
/// magnitude of its entry in row `k` of `R` (the *downdate*). That spares
/// measuring the column again. A downdated value is a difference. Once it
/// has fallen to `√ε` times the value last measured from the column,
/// cancellation has cost it about half its digits. It is then measured
/// again from the column.
struct ColumnNorms {
    /// The current squared norms, downdated.
    squared: Vec<f64>,
    /// Each column's squared norm when it was last measured from the
    /// column.
    last_measured: Vec<f64>,
}

impl ColumnNorms {
    /// The squared norms of the `rows`-long columns of `columns`.
    fn of(columns: &[Complex64], rows: usize) -> Self {
        let squared: Vec<f64> = columns.chunks_exact(rows).map(squared_norm).collect();
        let last_measured = squared.clone();
        Self {
            squared,
            last_measured,
        }
    }

    /// The column of largest norm from column `k` on, the last of those of
    /// equal norm.
    fn largest_from(&self, k: usize) -> usize {
        (k..self.squared.len())
            .max_by(|&i, &j| self.squared[i].total_cmp(&self.squared[j]))
            .expect("a column remains, since k < cols")
    }

    /// Follows a swap of the columns `i` and `j`.
    fn swap(&mut self, i: usize, j: usize) {
        self.squared.swap(i, j);
        self.last_measured.swap(i, j);
    }

    /// The norms of the columns after `k` of `columns`, `rows` long, once step
    /// `k` has reflected them. Each leaves out its row `k`, now in `R` (the
    /// downdate). It is measured again from rows `k + 1..` when the
    /// downdate has lost too many digits.
    fn leave_out_row(&mut self, columns: &[Complex64], rows: usize, k: usize) {
        let measure_again_below = f64::EPSILON.sqrt();
        for j in k + 1..self.squared.len() {
            if self.squared[j] == 0.0 {
                continue;
            }
            self.squared[j] -= columns[k + rows * j].norm_sqr();
            if self.squared[j] <= measure_again_below * self.last_measured[j] {
                self.squared[j] = squared_norm(&columns[rows * j + k + 1..rows * (j + 1)]);
                self.last_measured[j] = self.squared[j];
            }
        }
    }
}

/// Makes `x` the reflector that maps it to `β·e₀`. On return, `x[0] = β`
/// and `x[1..]` holds `v[1..]` (`v[0] = 1`). Returns `τ`, 0 when `x[1..]`
/// is already zero (then `H = I`). Let `θ` be the phase of `x[0]`. Then
/// `β = −e^{iθ}·‖x‖`. Its sign adds `x[0]` and `‖x‖` in `v[0] = x[0] − β`,
/// rather than subtract them, which would cancel.
fn turn_into_reflector(x: &mut [Complex64]) -> f64 {
    let tail = squared_norm(&x[1..]);
    if tail == 0.0 {
        return 0.0;
    }
    let first = x[0];
    let norm = (first.norm_sqr() + tail).sqrt();
    let phase = phase_of(first);
    // `v0 = e^{iθ}·(|x[0]| + ‖x‖)`, inverted through its phase, which keeps
    // `v`'s digits: `1 / v0` as `v0* / |v0|²` loses them once `|v0|²` is
    // subnormal.
    let inverse = phase.conj() / (first.norm() + norm);
    for z in &mut x[1..] {
        *z *= inverse;
    }
    x[0] = -phase * norm;
    // From the stored `v`, so that `H` is unitary however `v` was rounded:
    // `tail / |v0|²` loses its digits once `|v0|²` is subnormal.
    2.0 / (1.0 + squared_norm(&x[1..]))
}

/// `e^{iθ} = z / |z|`, of modulus 1 up to rounding, or 1 when `z` is
/// zero. `z` is first divided by its larger part's magnitude: `|z|` of a
/// subnormal `z` keeps few digits, so `z / |z|` may be far from modulus 1
/// (`1 + i` for `z = t + it`, `t` the smallest subnormal).
fn phase_of(z: Complex64) -> Complex64 {
    let larger = z.re.abs().max(z.im.abs());
    if larger == 0.0 {
        return Complex64::new(1.0, 0.0);
    }
    let scaled = z / larger;
    scaled / scaled.norm()
}

#[cfg(test)]
mod tests;
