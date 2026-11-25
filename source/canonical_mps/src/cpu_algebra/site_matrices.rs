// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SiteMatrices`]: one site tensor of the CPU algebra, read as the
//! matrix each factorisation needs, and a one-qubit gate applied to it.

use dense_linalg::MatrixRef;
use num_complex::Complex64;
use std::borrow::Cow;

/// One site `T[a, s, b]` of extents `[left, 2, right]`, in the
/// [site algebra's layout](crate::site_algebra). Its elements are borrowed
/// from the caller's tensor until a gate changes them.
///
/// The factorisations read its elements as one of the layout's two
/// matrices, without moving them:
/// [`SiteMatrices::qubit_in_rows_matrix`] and
/// [`SiteMatrices::qubit_in_columns_matrix`]. The layout's diagram shows
/// both.
pub(super) struct SiteMatrices<'e> {
    elements: Cow<'e, [Complex64]>,
    left_extent: usize,
    right_extent: usize,
}

impl<'e> SiteMatrices<'e> {
    /// The site of extents `[left_extent, 2, right_extent]` held by the
    /// first `left_extent·2·right_extent` elements of `tensor`. The rest,
    /// if any, is not the site's.
    pub(super) fn new(tensor: &'e [Complex64], left_extent: usize, right_extent: usize) -> Self {
        Self {
            elements: Cow::Borrowed(&tensor[..left_extent * 2 * right_extent]),
            left_extent,
            right_extent,
        }
    }

    /// The extent of the site's left bond.
    pub(super) fn left_extent(&self) -> usize {
        self.left_extent
    }

    /// The extent of the site's right bond.
    pub(super) fn right_extent(&self) -> usize {
        self.right_extent
    }

    /// The site as a `(2·left) × right` matrix: its qubit and left bond
    /// are the rows, its right bond the columns.
    pub(super) fn qubit_in_rows_matrix(&self) -> MatrixRef<'_> {
        MatrixRef::new(2 * self.left_extent, self.right_extent, &self.elements)
    }

    /// The site as a `left × (2·right)` matrix: its left bond is the rows,
    /// its qubit and right bond the columns.
    pub(super) fn qubit_in_columns_matrix(&self) -> MatrixRef<'_> {
        MatrixRef::new(self.left_extent, 2 * self.right_extent, &self.elements)
    }

    /// The site with the one-qubit `gate` (`G[s, p]` at `2s + p`) applied
    /// to its qubit, or unchanged when there is none. A gate makes the
    /// site's own copy, so the caller's tensor stays unchanged.
    pub(super) fn with_gate(mut self, gate: Option<&[Complex64; 4]>) -> Self {
        if let Some(gate) = gate {
            self.apply_one_qubit_gate(gate);
        }
        self
    }

    /// Applies the one-qubit `gate` to the site's qubit, in place:
    /// `T[a, s, b] ← Σ_p G[s, p]·T[a, p, b]`, for `a` the left bond, `s`
    /// the qubit and `b` the right bond. For each `(a, b)`, the pair of
    /// elements at `s = 0` and `s = 1` is multiplied by `G`.
    fn apply_one_qubit_gate(&mut self, gate: &[Complex64; 4]) {
        let (left, right) = (self.left_extent, self.right_extent);
        let site = self.elements.to_mut();
        for b in 0..right {
            for a in 0..left {
                let at = |s: usize| a + left * (s + 2 * b);
                let (zero, one) = (site[at(0)], site[at(1)]);
                site[at(0)] = gate[0] * zero + gate[1] * one;
                site[at(1)] = gate[2] * zero + gate[3] * one;
            }
        }
    }
}
