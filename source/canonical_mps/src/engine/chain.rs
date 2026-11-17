// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Chain`]: what a state holds, its sites in canonical form and the work
//! still pending on them, and [`SplitStatistics`], what keeping it small
//! cost.
//!
//! The chain holds, besides its sites, what lets a gate cost no backend
//! call until a split or a move of the centre needs one: one pending
//! one-site gate per site, applied by the next call that touches the site
//! (a unitary on a site's qubit keeps the canonical form), and the pending
//! window ([`Window`]), the product of the gates on up to three sites,
//! applied at once ([`window`](super::window)).

use super::CanonicalMpsError;
use super::gates::multiply2;
use super::window::Window;
use crate::algebra::SiteAlgebra;
use num_complex::Complex64;
use tensornet::MpsShape;

/// The chain of a state: its sites, the centre, the capacity, the work
/// pending on the host, the retained weight and the statistics.
pub(super) struct Chain<T> {
    pub(super) sites: Vec<Site<T>>,
    /// The one-site gate (row-major) each site's tensor still owes, if any.
    pub(super) pending: Vec<Option<[Complex64; 4]>>,
    /// The orthogonality centre: the one site that is not an isometry.
    pub(super) centre: usize,
    /// The capacity's bond at each cut.
    pub(super) capacity_bonds: Vec<usize>,
    /// The gates on a few sites waiting to be applied at once, if any.
    pub(super) window: Option<Window>,
    /// The product of the shares of their squared values the splits kept.
    /// The chain itself is normalised at each split, so its squared norm
    /// stays about 1.
    pub(super) retained_weight: f64,
    pub(super) statistics: SplitStatistics,
}

/// A site tensor `T[a, s, b]`, of extents `[left, 2, right]`.
pub(super) struct Site<T> {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) tensor: T,
}

impl<T> Site<T> {
    /// The number of elements the site holds.
    pub(super) fn len(&self) -> usize {
        2 * self.left * self.right
    }
}

impl<T> Chain<T> {
    /// |0…0⟩ on `capacity_bonds.len() + 1` sites, the centre on site 0.
    pub(super) fn zero<A: SiteAlgebra<Tensor = T>>(
        algebra: &A,
        capacity_bonds: Vec<usize>,
    ) -> Result<Self, CanonicalMpsError<A::Error>> {
        let n = capacity_bonds.len() + 1;
        let zero = [Complex64::ONE, Complex64::ZERO];
        let sites = (0..n)
            .map(|_| {
                algebra.upload(&zero).map(|tensor| Site {
                    left: 1,
                    right: 1,
                    tensor,
                })
            })
            .collect::<Result<_, _>>()
            .map_err(CanonicalMpsError::Algebra)?;
        Ok(Self {
            sites,
            pending: vec![None; n],
            centre: 0,
            capacity_bonds,
            window: None,
            retained_weight: 1.0,
            statistics: SplitStatistics::default(),
        })
    }

    /// A copy whose sites are new tensors, duplicated by `algebra`.
    pub(super) fn duplicate<A: SiteAlgebra<Tensor = T>>(
        &self,
        algebra: &A,
    ) -> Result<Self, CanonicalMpsError<A::Error>> {
        let sites = self
            .sites
            .iter()
            .map(|site| {
                algebra
                    .duplicate(&site.tensor, site.len())
                    .map(|tensor| Site {
                        left: site.left,
                        right: site.right,
                        tensor,
                    })
            })
            .collect::<Result<_, _>>()
            .map_err(CanonicalMpsError::Algebra)?;
        Ok(Self {
            sites,
            pending: self.pending.clone(),
            centre: self.centre,
            capacity_bonds: self.capacity_bonds.clone(),
            window: self.window.clone(),
            retained_weight: self.retained_weight,
            statistics: self.statistics,
        })
    }

    /// Applies the one-site gate `m` (row-major, checked) to `site`: into
    /// the pending window's operator when `site` is one of its sites,
    /// otherwise into the site's pending gate.
    pub(super) fn apply_one(&mut self, site: usize, m: &[Complex64]) {
        match self.window.as_mut() {
            Some(window) if window.holds(site) => window.apply(m, &[site]),
            _ => {
                let m: [Complex64; 4] = std::array::from_fn(|at| m[at]);
                let pending = &mut self.pending[site];
                *pending = Some(match pending {
                    Some(earlier) => multiply2(&m, earlier),
                    None => m,
                });
            }
        }
    }

    /// The chain's shape: one qubit per site, with the bond each cut holds.
    /// The pending window, if any, has not changed its bonds yet.
    pub(super) fn shape(&self) -> MpsShape {
        let n = self.sites.len();
        let sites = self
            .sites
            .iter()
            .enumerate()
            .map(|(at, site)| {
                let left = (at > 0).then_some(site.left);
                let right = (at + 1 < n).then_some(site.right);
                left.into_iter().chain([2]).chain(right).collect()
            })
            .collect();
        MpsShape::new(sites).expect("a chain whose neighbours share each bond")
    }
}

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
pub struct SplitStatistics {
    pub(super) splits: u64,
    pub(super) mpo_applications: u64,
    pub(super) centre_moves: u64,
    pub(super) largest_bond_at_rest: usize,
    pub(super) largest_transient_bond: usize,
}

impl SplitStatistics {
    /// The splits of two neighbouring sites, each one
    /// [`gate_split`](crate::algebra::SiteAlgebra::gate_split): one per
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
    /// [`qr_move`](crate::algebra::SiteAlgebra::qr_move).
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
    /// capacity's largest bond times the operator's (4 for a window of two
    /// or three sites); 1 for a state without such windows.
    #[must_use]
    pub fn largest_transient_bond(&self) -> usize {
        self.largest_transient_bond.max(1)
    }
}
