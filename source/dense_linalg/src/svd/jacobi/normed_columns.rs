// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NormedColumns`]: the columns being made orthogonal, with their squared
//! norms, which each rotation updates rather than measures again. It tells
//! whether a pair needs a rotation, and reads the SVD off the columns once
//! all are orthogonal.

use super::ColumnMajorSvd;
use super::rotation::{ColumnPair, Rotation};
use super::split_complex_columns::SplitComplexColumns;

/// The columns `w_j` being orthogonalized, with their squared norms, which
/// each rotation updates rather than recomputes.
pub(super) struct NormedColumns {
    columns: SplitComplexColumns,
    norms_sqr: Vec<f64>,
    /// The threshold of [`is_coupled`], `√n · ε` for columns of length `n`.
    tolerance: f64,
}

impl NormedColumns {
    /// The columns, with their norms computed once.
    pub(super) fn new(columns: SplitComplexColumns) -> Self {
        let norms_sqr = columns.norms_sqr();
        #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
        let tolerance = (columns.rows as f64).sqrt() * f64::EPSILON;
        Self {
            columns,
            norms_sqr,
            tolerance,
        }
    }

    /// The number of columns.
    pub(super) fn count(&self) -> usize {
        self.columns.cols
    }

    /// The rotation that makes columns `p` and `q` orthogonal. `None` when
    /// they already are, to the threshold ([`is_coupled`]).
    pub(super) fn rotation_making_orthogonal(&self, p: usize, q: usize) -> Option<Rotation> {
        let pair = self.pair(p, q);
        is_coupled(
            pair.coupling,
            pair.p_norm_sqr,
            pair.q_norm_sqr,
            self.tolerance,
        )
        .then(|| Rotation::making_orthogonal(&pair))
    }

    /// The columns `p` and `q`, as a rotation reads them: their squared
    /// norms as kept, and their inner product.
    fn pair(&self, p: usize, q: usize) -> ColumnPair {
        let (re, im) = self.columns.inner_product(p, q);
        ColumnPair {
            p_norm_sqr: self.norms_sqr[p],
            q_norm_sqr: self.norms_sqr[q],
            inner_product: (re, im),
            coupling: re.hypot(im),
        }
    }

    /// Applies `rotation`, which orthogonalizes columns `p < q`, and updates
    /// their squared norms ([`updated_norm_sqr`]).
    pub(super) fn rotate(&mut self, p: usize, q: usize, rotation: &Rotation) {
        self.columns.rotate(p, q, rotation);
        let moved = rotation.moved_norm_sqr;
        let (p_before, q_before) = (self.norms_sqr[p], self.norms_sqr[q]);
        self.norms_sqr[p] =
            updated_norm_sqr(p_before, p_before - moved, || self.columns.norm_sqr(p));
        self.norms_sqr[q] =
            updated_norm_sqr(q_before, q_before + moved, || self.columns.norm_sqr(q));
    }

    /// The SVD the orthogonal columns give, `W = U·diag(S)·V†` with `V`
    /// the accumulated `rotations`:
    ///
    /// - the singular values are the column norms, in nonincreasing order,
    ///   recomputed once rather than read from their updates;
    /// - `U` holds the columns divided by their norms, in that order;
    /// - `V` holds the rotations' columns, in that order.
    pub(super) fn into_svd(self, rotations: &SplitComplexColumns) -> ColumnMajorSvd {
        let norms_sqr = self.columns.norms_sqr();
        let order = longest_first(&norms_sqr);
        let values: Vec<f64> = order.iter().map(|&j| norms_sqr[j].sqrt()).collect();
        ColumnMajorSvd {
            left_vectors: self.columns.divided_in_order(&order, &values),
            singular_values: values,
            right_vectors: rotations.in_order(&order),
        }
    }
}

/// The indices of the columns of these squared norms, longest first.
fn longest_first(norms_sqr: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..norms_sqr.len()).collect();
    order.sort_by(|&i, &j| norms_sqr[j].total_cmp(&norms_sqr[i]));
    order
}

/// Whether two columns are still too far from orthogonal and must be
/// rotated. Their squared norms are `alpha` and `beta`, and their product
/// has magnitude `coupling = |w_p† w_q|`. They must be rotated when the
/// cosine of their angle, `coupling / (‖w_p‖·‖w_q‖)`, exceeds `tolerance`.
/// That is `√n · ε`, with `n` the length of a column: about the rounding
/// error of the product of two unit columns of that length. Below it, the
/// pair is orthogonal to rounding. Rotating it would change nothing that
/// matters.
///
/// False when `coupling` is NaN too, so that the iteration ends (and the
/// checks of the parent module refuse the result).
fn is_coupled(coupling: f64, alpha: f64, beta: f64, tolerance: f64) -> bool {
    coupling > tolerance * alpha.sqrt() * beta.sqrt()
}

/// A column's squared norm after a rotation. It is `updated`, the norm
/// `before` plus or minus the squared norm the rotation moved, which saves a
/// pass over the column. Or it is `recomputed()` when the update lost half of
/// `before` (or is NaN). Such an update is the difference of two close
/// numbers, so it has lost accuracy. The next pairs' tests and angles would
/// read a wrong norm.
fn updated_norm_sqr(before: f64, updated: f64, recomputed: impl FnOnce() -> f64) -> f64 {
    if updated >= 0.5 * before {
        updated
    } else {
        recomputed()
    }
}

#[cfg(test)]
mod tests;
