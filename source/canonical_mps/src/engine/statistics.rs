// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CanonicalMpsStatistics`]: what a state's history cost, which the chain
//! counts as its splits, moves of the centre and MPO applications happen.

/// What the splits and moves of a state's history cost: how many splits it
/// took, how many pending windows went as a matrix product operator, how
/// many times the centre moved by one site, and the largest bonds the
/// chain held, at rest and while such a window was applied.
///
/// Every split keeps at most the capacity's bond at its cut: with one `χ`
/// at every cut, a largest bond at rest below `χ` says the capacity cut
/// nothing, and one at `χ` that it may have, so a smaller `χ` would cut
/// more. A window applied as a matrix product operator grows the bonds of
/// its span, before its splits compress them, up to the capacity's times
/// the operator's bond (Sites far apart on
/// [`CanonicalMpsEngine`](super::CanonicalMpsEngine)), which
/// [`MpsEngine::shape`](tensornet::MpsEngine::shape) never shows: the
/// larger of the two figures is the largest bond the state held. The
/// splits and the moves of the centre count the backend's factorisations
/// (`gate_split`, `qr_move`), one call each.
///
/// With fusion, the window's gates count when the window is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CanonicalMpsStatistics {
    pub(super) splits: u64,
    pub(super) mpo_applications: u64,
    pub(super) centre_moves: u64,
    pub(super) largest_bond_at_rest: usize,
    pub(super) largest_transient_bond: usize,
}

impl CanonicalMpsStatistics {
    /// The splits of two neighbouring sites, each one
    /// [`gate_split`](crate::site_algebra::SiteAlgebra::gate_split): one per
    /// window on two neighbours, one per cut of the span of a window
    /// applied as a matrix product operator.
    #[must_use]
    pub fn splits(&self) -> u64 {
        self.splits
    }

    /// The pending windows applied as a matrix product operator: those on
    /// sites that are not neighbours, or on three sites.
    #[must_use]
    pub fn mpo_applications(&self) -> u64 {
        self.mpo_applications
    }

    /// The moves of the centre by one site, outside the splits: each one
    /// [`qr_move`](crate::site_algebra::SiteAlgebra::qr_move).
    #[must_use]
    pub fn centre_moves(&self) -> u64 {
        self.centre_moves
    }

    /// The largest bond a split kept, at most the capacity's largest bond;
    /// 1 for a state without splits.
    #[must_use]
    pub fn largest_bond_at_rest(&self) -> usize {
        self.largest_bond_at_rest.max(1)
    }

    /// The largest bond a window applied as a matrix product operator grew
    /// a cut of its span to, before its splits compressed it: at most the
    /// capacity's largest bond times the operator's (at most 4 for a window
    /// of two or three sites); 1 for a state without such windows.
    #[must_use]
    pub fn largest_transient_bond(&self) -> usize {
        self.largest_transient_bond.max(1)
    }
}
