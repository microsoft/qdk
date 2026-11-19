// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GateSplitCall`] and [`GateSplitOutcome`]: what the engine passes to
//! [`SiteAlgebra::gate_split`](super::SiteAlgebra::gate_split), and what
//! it gets back.

use super::CentreSide;
use num_complex::Complex64;

/// One gate split of the pair `A[a, p, j]`, `B[j, q, b]`: the gated pair
/// `θ[a, s, t, b] = Σ G[(s, t), (p, q)] · A[a, p, j] · B[j, q, b]`, read as
/// the column-major `(2·left_extent) × (2·right_extent)` matrix of rows
/// `a + left_extent·s` and columns `t + 2·b`, is split as `θ ≈ L·R`, with
/// `L` of extents `[left_extent, 2, k]` and `R` of extents
/// `[k, 2, right_extent]`, `k` the outcome's bond
/// ([`SiteAlgebra::gate_split`](super::SiteAlgebra::gate_split)).
///
/// `θ` is never zero: the caller's states have a norm of 1 to rounding (a
/// gate may drift from it within the unitarity check's tolerance).
#[derive(Debug)]
pub struct GateSplitCall<'t, T> {
    /// `A`, of extents `[left_extent, 2, bond]`.
    pub left: &'t T,
    /// `B`, of extents `[bond, 2, right_extent]`.
    pub right: &'t T,
    /// `A`'s left extent.
    pub left_extent: usize,
    /// The extent of the bond the pair shares before the split.
    pub bond: usize,
    /// `B`'s right extent.
    pub right_extent: usize,
    /// The gate `G`, row-major, the left site the most significant bit:
    /// `G[(s, t), (p, q)]` at `(2s + t)·4 + 2p + q`.
    pub gate: [Complex64; 16],
    /// The largest bond the split may keep, at least 1; `usize::MAX`: no
    /// cap.
    pub max_extent: usize,
    /// The share of the squared singular values the split may drop, in
    /// `[0, 1)`, and beyond it values of rounding's size, which an SVD may
    /// give where the exact values are zero: their squares may add up to
    /// 1e-20 of the squared norm (values up to 1e-10 of the norm), the share
    /// the conformance kit allows for rounding. So 0 drops the zeros, and
    /// may drop such values.
    pub cutoff: f64,
    /// The site that absorbs the singular values, and so holds the centre
    /// afterwards.
    pub centre: CentreSide,
}

/// What one gate split returned
/// ([`SiteAlgebra::gate_split`](super::SiteAlgebra::gate_split)).
#[derive(Debug)]
pub struct GateSplitOutcome<T> {
    /// The new left site, of extents `[left_extent, 2, bond]`, packed.
    pub left: T,
    /// The new right site, of extents `[bond, 2, right_extent]`, packed.
    pub right: T,
    /// The extent of the bond kept.
    pub bond: usize,
    /// The share of the squared singular values kept, in `(0, 1]`: the
    /// engine multiplies the state's retained weight by it.
    pub retained_fraction: f64,
}
