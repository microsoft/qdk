// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CPU algebra's gate split. It applies a two-site gate to two
//! neighbouring sites and cuts the result back into two sites. It is the
//! one call where the state's bonds are truncated, to keep the state small.
//!
//! Its steps: the gated pair (the two sites merged and gated), its SVD,
//! and the two new sites ([`SplitSites`]) cut from the values it keeps.
//!
//! The split, for a left site `A` of extents `[l, 2, m]` and a right site
//! `B` of extents `[m, 2, r]` (`n` the SVD's number of values, `k` the
//! number kept):
//!
//! ```text
//!  gated_pair:
//!    A (2l × m) · B (m × 2r)    the pair, (2l × 2r): rows (a, p),
//!                               columns (q, b)
//!    gate on (p, q) -> (s, t)   the gated pair θ, (2l × 2r): rows (a, s),
//!                               columns (t, b)
//!  Svd::of(θ):                  U (2l × n) · S (n values) · V† (n × 2r)
//!  Truncation::of(S):           keeps the k largest values, k ≤ n,
//!                               divided by their L2 norm
//!  SplitSites::keeping:         Svd::truncated(k):
//!                               left  = U's first k columns: [l, 2, k]
//!                               right = V†'s first k rows:   [k, 2, r]
//!  absorb_values_into_centre:   centre on the left:  left's column j
//!                                                    times value j
//!                               centre on the right: right's row j
//!                                                    times value j
//!                               (Matrix::multiply_columns_by or
//!                               multiply_rows_by)
//! ```
//!
//! No reshape is needed at the end: `U`'s columns, in column-major order,
//! are already a site in the layout, and so are `V†`'s rows once packed.
//!
//! Its part `truncation` ([`truncation::Truncation`]) is the rule that
//! decides which values the split keeps.

pub(super) mod truncation;

use super::site_matrices::SiteMatrices;
use crate::site_algebra::{CentreSide, GateSplitOutcome};
use dense_linalg::{Matrix, Svd};
use num_complex::Complex64;

/// The gated pair: the two sites merged over the bond they share, then
/// the two-qubit `gate` applied to their qubits. It is the `θ` of
/// [`GateSplitCall`](crate::site_algebra::GateSplitCall), a
/// `(2·l) × (2·r)` matrix, `l` the left site's left extent and `r` the
/// right site's right extent.
pub(super) fn gated_pair(
    left: &SiteMatrices<'_>,
    right: &SiteMatrices<'_>,
    gate: &[Complex64; 16],
) -> Matrix {
    let pair = Matrix::product(left.qubit_in_rows_matrix(), right.qubit_in_columns_matrix());
    apply_two_qubit_gate(&pair, gate, left.left_extent(), right.right_extent())
}

/// The two new sites of a split, before the values are absorbed, each a
/// matrix whose elements are the site in the layout.
pub(super) struct SplitSites {
    /// `[left_extent, 2, bond]`, the `(2·left_extent) × bond` matrix.
    left: Matrix,
    /// `[bond, 2, right_extent]`, the `bond × (2·right_extent)` matrix.
    right: Matrix,
}

impl SplitSites {
    /// The new sites made of the SVD's first `bond` singular vectors on
    /// each side: `U`'s first columns on the left, `V†`'s first rows on
    /// the right. The vectors of the dropped values go: that is the
    /// truncation.
    pub(super) fn keeping(svd: Svd, bond: usize) -> Self {
        let Svd { u, v_adjoint, .. } = svd.truncated(bond);
        Self {
            left: u,
            right: v_adjoint,
        }
    }

    /// Multiplies the kept `values` into the site the centre is on, one
    /// value per bond index: the left site's columns or the right site's
    /// rows. That site then carries the state's norm. The other stays an
    /// isometry, as the canonical form needs.
    pub(super) fn absorb_values_into_centre(&mut self, values: &[f64], centre: CentreSide) {
        match centre {
            CentreSide::Left => self.left.multiply_columns_by(values),
            CentreSide::Right => self.right.multiply_rows_by(values),
        }
    }

    /// The split's outcome, the sites as they are.
    pub(super) fn into_outcome(self, retained_fraction: f64) -> GateSplitOutcome<Vec<Complex64>> {
        GateSplitOutcome {
            bond: self.left.cols(),
            left: self.left.into_elements(),
            right: self.right.into_elements(),
            retained_fraction,
        }
    }
}

/// The two-qubit `gate` applied to the qubits of the merged `pair`,
/// giving the gated pair `θ`:
/// `θ[a, s, t, b] = Σ G[(s, t), (p, q)] · pair[a, p, q, b]`. The `pair`
/// and `θ` are both `(2·l) × (2·r)` matrices of rows `a + l·s` and
/// columns `t + 2·b`, for `l` the `left_extent` and `r` the
/// `right_extent`. The gated qubits are `(s, t)`, the pair's `(p, q)`,
/// `a` the left bond and `b` the right bond.
///
/// Each column of either matrix is two blocks of `l` values, one per
/// left qubit `s`. So for each right bond `b`, `θ`'s block `(s, t)` is a
/// sum of the pair's four blocks `(p, q)` at the same `b`, weighted by
/// the gate's entries:
///
/// ```text
///  θ or the pair, left = 2, right = 2 (4 × 4):
///
///              t=0  t=1  t=0  t=1
///              b=0  b=0  b=1  b=1
///   a=0, s=0 [  .    .    .    .  ]  blocks    θ's block (s, t) at b
///   a=1, s=0 [  .    .    .    .  ]  (0, t)    = Σ over (p, q) of
///   a=0, s=1 [  .    .    .    .  ]  blocks      G[(s, t), (p, q)]
///   a=1, s=1 [  .    .    .    .  ]  (1, t)      · the pair's block
///                                                  (p, q) at b
/// ```
///
/// The sums run in the order `(p, q)` = `00, 01, 10, 11`, from zero.
fn apply_two_qubit_gate(
    pair: &Matrix,
    gate: &[Complex64; 16],
    left_extent: usize,
    right_extent: usize,
) -> Matrix {
    let rows = 2 * left_extent;
    let pair = pair.elements();
    let mut gated = vec![Complex64::ZERO; rows * 2 * right_extent];
    for b in 0..right_extent {
        // Where the block of the qubits `(s, t)`, at right bond `b`, starts.
        let block = |s: usize, t: usize| rows * (t + 2 * b) + left_extent * s;
        for (s, t, p, q) in gate_entry_indices() {
            let entry = gate[(2 * s + t) * 4 + 2 * p + q];
            add_scaled(
                &mut gated[block(s, t)..][..left_extent],
                entry,
                &pair[block(p, q)..][..left_extent],
            );
        }
    }
    Matrix::new(rows, 2 * right_extent, gated)
}

/// Every gate entry's indices `(s, t, p, q)` in `{0, 1}⁴`, in
/// lexicographic order: `G[(s, t), (p, q)]` maps the qubits `(p, q)` to
/// `(s, t)`.
#[inline]
fn gate_entry_indices() -> impl Iterator<Item = (usize, usize, usize, usize)> {
    (0..16).map(|i| (i >> 3 & 1, i >> 2 & 1, i >> 1 & 1, i & 1))
}

/// `target += factor · source`, element by element.
#[inline]
fn add_scaled(target: &mut [Complex64], factor: Complex64, source: &[Complex64]) {
    for (sum, &value) in target.iter_mut().zip(source) {
        *sum += factor * value;
    }
}

#[cfg(test)]
mod tests;
