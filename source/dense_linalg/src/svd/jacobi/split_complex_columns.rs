// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SplitComplexColumns`]: the columns the rotations turn, stored so that
//! their loops vectorize, with the inner product and the squared norm.
//!
//! Each column is stored as its real parts, then its imaginary parts
//! (*split complex*). Here, 2 rows and 3 columns, `x_ij` in row `i` and
//! column `j`:
//!
//! ```text
//!  entries = [ re x₀₀, re x₁₀, im x₀₀, im x₁₀,   re x₀₁, re x₁₁, … ]
//!              └──────────── column 0 ───────┘   └─── column 1 ───…
//! ```
//!
//! So the products and rotations are loops on `f64` that the compiler
//! vectorizes. They multiply and add without fused multiply-add (see
//! [`dot`]).

use super::rotation::Rotation;
use num_complex::Complex64;

/// The columns of a complex matrix, one column after the other. Each is
/// stored as its real parts then its imaginary parts, `[re…, im…]` (split
/// complex). Its loops (here, and [`Rotation::apply`]) vectorize that
/// layout.
pub(super) struct SplitComplexColumns {
    /// The length of a column.
    pub(super) rows: usize,
    /// The number of columns.
    pub(super) cols: usize,
    entries: Vec<f64>,
}

impl SplitComplexColumns {
    /// The `cols` columns of length `rows` held in `entries`.
    pub(super) fn new(rows: usize, cols: usize, entries: Vec<f64>) -> Self {
        Self {
            rows,
            cols,
            entries,
        }
    }

    /// The columns of the `n × n` identity.
    pub(super) fn identity(n: usize) -> Self {
        let mut entries = vec![0.0; 2 * n * n];
        for j in 0..n {
            entries[2 * n * j + j] = 1.0;
        }
        Self::new(n, n, entries)
    }

    /// Column `j`, as `[re…, im…]`.
    fn column(&self, j: usize) -> &[f64] {
        &self.entries[2 * self.rows * j..2 * self.rows * (j + 1)]
    }

    /// Column `j`, as complex numbers.
    fn complex_column(&self, j: usize) -> impl Iterator<Item = Complex64> + '_ {
        let (re, im) = self.column(j).split_at(self.rows);
        re.iter().zip(im).map(|(x, y)| Complex64::new(*x, *y))
    }

    /// The columns `order`, in that order, as complex numbers, column-major.
    pub(super) fn in_order(&self, order: &[usize]) -> Vec<Complex64> {
        let mut columns = Vec::with_capacity(self.rows * order.len());
        for &j in order {
            columns.extend(self.complex_column(j));
        }
        columns
    }

    /// The columns `order`, in that order, each divided by its `divisor`,
    /// as complex numbers, column-major.
    pub(super) fn divided_in_order(&self, order: &[usize], divisors: &[f64]) -> Vec<Complex64> {
        let mut columns = Vec::with_capacity(self.rows * order.len());
        for (&j, &divisor) in order.iter().zip(divisors) {
            columns.extend(self.complex_column(j).map(|x| x / divisor));
        }
        columns
    }

    /// The inner product `x_p† x_q`, as `(re, im)`.
    pub(super) fn inner_product(&self, p: usize, q: usize) -> (f64, f64) {
        dot(self.column(p), self.column(q))
    }

    /// `‖x_j‖²`.
    pub(super) fn norm_sqr(&self, j: usize) -> f64 {
        norm_sqr(self.column(j))
    }

    /// Every column's `‖x_j‖²`, measured.
    pub(super) fn norms_sqr(&self) -> Vec<f64> {
        (0..self.cols).map(|j| self.norm_sqr(j)).collect()
    }

    /// Applies `rotation` to the columns `p < q`.
    pub(super) fn rotate(&mut self, p: usize, q: usize, rotation: &Rotation) {
        rotation.apply(&mut self.entries, self.rows, p, q);
    }
}

/// `x† y` as `(re, im)`, with four accumulators of each part, so that the
/// additions do not wait on each other.
///
/// These loops (`dot`, [`norm_sqr`] and the rotations) multiply and add
/// without fused multiply-add, which some targets lack. So the rotations
/// round alike on every target. The products
/// ([`Kernels`](crate::kernels::Kernels)) may use it where
/// the target has it.
fn dot(x: &[f64], y: &[f64]) -> (f64, f64) {
    let len = x.len() / 2;
    let (x_re, x_im) = x.split_at(len);
    let (y_re, y_im) = y.split_at(len);
    let mut re = [0.0; 4];
    let mut im = [0.0; 4];
    let chunks = (x_re.as_chunks::<4>().0.iter())
        .zip(x_im.as_chunks::<4>().0)
        .zip(y_re.as_chunks::<4>().0.iter().zip(y_im.as_chunks::<4>().0));
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
    let (chunks, rest) = x.as_chunks::<8>();
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
