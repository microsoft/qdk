// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit's inputs, seeded so that a failure repeats:
//!
//! - pairs of sites, with graded singular values or with values known by
//!   construction;
//! - gates.
//!
//! Every input's seed derives from the kit's seed ([`derived`]). A failure
//! names its pair's seed. A scene's random gate is drawn from that seed
//! plus 5, so the seed repeats the whole scene.

use dense_linalg::{Matrix, Qr};
use num_complex::Complex64;
use std::fmt;
use tensornet::sampling::SplitMix64;

/// The pairs' shapes `(left_extent, bond, right_extent)`. They run from a
/// pair whose extents are all one (`θ` of 2 × 2) to a gated pair of 32 × 16.
/// They are square and not, and of full rank and not. For instance, under
/// the identity gate, `(16, 12, 8)` gives a `θ` of 32 × 16 with rank at most
/// 12.
pub(super) const SHAPES: [(usize, usize, usize); 6] = [
    (1, 1, 1),
    (2, 2, 4),
    (4, 3, 2),
    (8, 6, 4),
    (8, 8, 8),
    (16, 12, 8),
];

/// A pair whose `θ` is 64 × 64, where rounding errors add up the most. It
/// is moved, and split only without truncation. Each of its SVDs costs
/// about as much as a case's splits of all [`SHAPES`].
pub(super) const LARGE_SHAPE: (usize, usize, usize) = (32, 32, 32);

/// More shapes for the moves of the centre:
///
/// - bonds larger than the side of the site the centre leaves
///   (`2·left_extent` moving right, `2·right_extent` moving left);
/// - a bond smaller than both.
pub(super) const MOVE_SHAPES: [(usize, usize, usize); 3] = [(1, 4, 2), (2, 5, 1), (8, 3, 16)];

/// The splits' caps: from one value to none.
pub(super) const CAPS: [usize; 4] = [1, 2, 3, usize::MAX];

/// The splits' cutoffs:
///
/// - zero;
/// - a cutoff that drops a tail of most graded pairs;
/// - the largest double below one, which may drop all but the largest
///   value.
pub(super) const CUTOFFS: [f64; 3] = [0.0, 0.05, 1.0 - f64::EPSILON / 2.0];

/// The ratio of each bond slice of a graded pair's left site to the one
/// before it. So the pair's singular values decay, distinct, and a cutoff
/// drops a tail.
const GRADE: f64 = 0.5;

/// The seed of a case's input `offset` under the kit's seed `kit_seed`:
/// `1000·kit_seed + offset`. Every offset is below 1000. So a failure's
/// seed reads as the kit's seed and the input (seed 7305 is input 305 of
/// kit seed 7). Kit seed 0 draws the inputs of a fixed kit.
pub(super) fn derived(kit_seed: u64, offset: u64) -> u64 {
    kit_seed.wrapping_mul(1000).wrapping_add(offset)
}

/// Two neighbouring sites `A[a, p, j]` and `B[j, q, b]`, of extents
/// `[l, 2, m]` and `[m, 2, r]`, in the contract's layout.
pub(super) struct Pair {
    /// `A`, `2·l·m` elements.
    pub(super) left: Vec<Complex64>,
    /// `B`, `m·2·r` elements.
    pub(super) right: Vec<Complex64>,
    /// `A`'s left extent.
    pub(super) l: usize,
    /// The bond between them.
    pub(super) m: usize,
    /// `B`'s right extent.
    pub(super) r: usize,
    /// The seed it was drawn from.
    seed: u64,
}

impl Pair {
    /// A random pair of the shape `(l, m, r)` whose left site's bond
    /// slices `A[·, ·, j]` decay by [`GRADE`].
    pub(super) fn graded((l, m, r): (usize, usize, usize), seed: u64) -> Self {
        let mut left = random(2 * l, m, seed).into_elements();
        let mut scale = 1.0;
        for slice in left.chunks_exact_mut(2 * l) {
            for x in slice {
                *x *= scale;
            }
            scale *= GRADE;
        }
        let right = random(m, 2 * r, seed.wrapping_add(1)).into_elements();
        Self {
            left,
            right,
            l,
            m,
            r,
            seed,
        }
    }

    /// A pair of the outer extents `l` and `r` whose product `A·B`, read
    /// as `θ` under the identity gate, is `U·diag(values)·V†`. `U` and `V`
    /// are random unitaries. So its singular values are `values` by
    /// construction. The bond is `values.len()`, at most `2·l` and `2·r`.
    pub(super) fn with_values(values: &[f64], l: usize, r: usize, seed: u64) -> Self {
        let m = values.len();
        let u = unitary(2 * l, seed);
        let v_adjoint = unitary(2 * r, seed.wrapping_add(1)).view().adjoint();
        Self {
            left: leading_columns_scaled(&u, values),
            right: leading_rows(&v_adjoint, m),
            l,
            m,
            r,
            seed,
        }
    }
}

/// The first `values.len()` columns of `matrix`, column `j` scaled by
/// `values[j]`, column-major: `U·diag(values)` for the left site.
fn leading_columns_scaled(matrix: &Matrix, values: &[f64]) -> Vec<Complex64> {
    let rows = matrix.rows();
    (values.iter().enumerate())
        .flat_map(|(j, &value)| {
            matrix.elements()[rows * j..rows * (j + 1)]
                .iter()
                .map(move |x| x * value)
        })
        .collect()
}

/// The first `count` rows of `matrix`, column-major: `V†`'s for the right
/// site.
fn leading_rows(matrix: &Matrix, count: usize) -> Vec<Complex64> {
    let rows = matrix.rows();
    (0..matrix.cols())
        .flat_map(|col| {
            matrix.elements()[rows * col..rows * col + count]
                .iter()
                .copied()
        })
        .collect()
}

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the pair [{}, {}, {}] of seed {}",
            self.l, self.m, self.r, self.seed
        )
    }
}

/// A random two-site gate, row-major as the calls hold it. It is random,
/// not unitary, so a transposed or misplaced gate changes the result.
pub(super) fn two_site_gate(seed: u64) -> [Complex64; 16] {
    random(4, 4, seed)
        .elements()
        .try_into()
        .expect("a 4 × 4 matrix has 16 entries")
}

/// A random one-site gate, row-major as the calls hold it.
pub(super) fn one_site_gate(seed: u64) -> [Complex64; 4] {
    random(2, 2, seed)
        .elements()
        .try_into()
        .expect("a 2 × 2 matrix has 4 entries")
}

/// The two-site identity gate, row-major.
pub(crate) fn identity_gate() -> [Complex64; 16] {
    std::array::from_fn(|i| Complex64::new(if i % 5 == 0 { 1.0 } else { 0.0 }, 0.0))
}

/// `count` distinct values, for the storage cases.
pub(super) fn values(count: usize, seed: u64) -> Vec<Complex64> {
    random(count, 1, seed).into_elements()
}

/// A random `n × n` unitary. It is the Q of the QR decomposition of a
/// random matrix.
fn unitary(n: usize, seed: u64) -> Matrix {
    Qr::of(random(n, n, seed).view()).q
}

/// A `rows × cols` matrix of entries in `[-0.5, 0.5)`, real and imaginary
/// parts alike, from `tensornet`'s [`SplitMix64`] seeded by `seed`.
pub(crate) fn random(rows: usize, cols: usize, seed: u64) -> Matrix {
    let mut stream = SplitMix64::new(seed);
    let mut next = move || stream.next_uniform() - 0.5;
    Matrix::from_fn(rows, cols, |_, _| {
        let re = next();
        Complex64::new(re, next())
    })
}
