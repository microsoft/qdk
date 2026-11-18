// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit's inputs: seeded pairs of sites, with graded singular values or
//! with values known by construction, and seeded gates, so that a failure
//! repeats.

use crate::dense_linalg::{Matrix, Qr};
use num_complex::Complex64;
use std::fmt;

/// The pairs' shapes `(left_extent, bond, right_extent)`: from a pair whose
/// extents are all one (`θ` of 2 × 2) to a gated pair of 32 × 16, square and
/// not, and of full rank and not (`(16, 12, 8)`: `θ` of 32 × 16 has rank at
/// most 12 under the identity gate).
pub(super) const SHAPES: [(usize, usize, usize); 6] = [
    (1, 1, 1),
    (2, 2, 4),
    (4, 3, 2),
    (8, 6, 4),
    (8, 8, 8),
    (16, 12, 8),
];

/// A pair whose `θ` is 64 × 64, where rounding errors add up the most:
/// split only without truncation, as each of its SVDs costs about as much
/// as a case's splits of all [`SHAPES`], and moved.
pub(super) const LARGE_SHAPE: (usize, usize, usize) = (32, 32, 32);

/// More shapes for the moves of the centre: bonds larger than the side of
/// the site the centre leaves (`2·left_extent` moving right,
/// `2·right_extent` moving left), and a bond smaller than both.
pub(super) const MOVE_SHAPES: [(usize, usize, usize); 3] = [(1, 4, 2), (2, 5, 1), (8, 3, 16)];

/// The splits' caps: from one value to none.
pub(super) const CAPS: [usize; 4] = [1, 2, 3, usize::MAX];

/// The splits' cutoffs: zero, a cutoff that drops a tail of most graded
/// pairs, and the largest double below one, which may drop all but the
/// largest value.
pub(super) const CUTOFFS: [f64; 3] = [0.0, 0.05, 1.0 - f64::EPSILON / 2.0];

/// The ratio of each bond slice of a graded pair's left site to the one
/// before it, so that the pair's singular values decay, distinct, and a
/// cutoff drops a tail.
const GRADE: f64 = 0.5;

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
        let right = random(m, 2 * r, seed + 1).into_elements();
        Self {
            left,
            right,
            l,
            m,
            r,
        }
    }

    /// A pair of the outer extents `l` and `r` whose product `A·B`, read
    /// as `θ` under the identity gate, is `U·diag(values)·V†` with `U` and
    /// `V` random unitaries: its singular values are `values` by
    /// construction. The bond is `values.len()`, at most `2·l` and `2·r`.
    pub(super) fn with_values(values: &[f64], l: usize, r: usize, seed: u64) -> Self {
        let m = values.len();
        let u = unitary(2 * l, seed);
        let v_adjoint = unitary(2 * r, seed + 1).view().adjoint();
        let left = (0..m)
            .flat_map(|j| {
                u.elements()[2 * l * j..2 * l * (j + 1)]
                    .iter()
                    .map(move |x| x * values[j])
            })
            .collect();
        let right = (0..2 * r)
            .flat_map(|col| {
                v_adjoint.elements()[2 * r * col..2 * r * col + m]
                    .iter()
                    .copied()
            })
            .collect();
        Self {
            left,
            right,
            l,
            m,
            r,
        }
    }
}

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the pair [{}, {}, {}]", self.l, self.m, self.r)
    }
}

/// A random two-site gate, row-major as the calls hold it. Random, not
/// unitary: a transposed or misplaced gate then changes the result.
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

/// The two-site identity gate.
pub(super) fn identity_gate() -> [Complex64; 16] {
    std::array::from_fn(|i| Complex64::new(if i % 5 == 0 { 1.0 } else { 0.0 }, 0.0))
}

/// `count` distinct values, for the storage cases.
pub(super) fn values(count: usize, seed: u64) -> Vec<Complex64> {
    random(count, 1, seed).into_elements()
}

/// A random `n × n` unitary: the Q of the QR decomposition of a random
/// matrix.
fn unitary(n: usize, seed: u64) -> Matrix {
    Qr::of(random(n, n, seed).view()).q
}

/// A `rows × cols` matrix of entries in `[-0.5, 0.5)`, real and imaginary
/// parts alike, from a linear congruential generator seeded by `seed`.
fn random(rows: usize, cols: usize, seed: u64) -> Matrix {
    let mut state = seed;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The top 53 bits, exactly representable.
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit a double")]
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        unit - 0.5
    };
    Matrix::from_fn(rows, cols, |_, _| {
        let re = next();
        Complex64::new(re, next())
    })
}
