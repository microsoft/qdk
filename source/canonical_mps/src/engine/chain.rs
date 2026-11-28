// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Chain`]: what a state holds. That is its sites in canonical form, the
//! work still pending on them, and the statistics of its history
//! ([`statistics`](super::statistics)).
//!
//! Besides its sites, the chain holds what lets a gate cost no backend call
//! until a split or a move of the centre needs one:
//!
//! - one pending one-site gate per site, applied by the next call that
//!   touches the site (a unitary on a site's qubit keeps the canonical
//!   form);
//! - the pending window ([`Window`]), the product of the gates on up to
//!   three sites, applied at once ([`fusion`](super::fusion)).
//!
//! A chain of four sites, its centre on site 2, a gate pending on sites 0
//! and 2, and a window open on sites 1 and 3:
//!
//! ```text
//!   site        0          1          2          3
//!             [iso]--b0--[iso]--b1--[ C ]--b2--[iso]
//!               |          |          |          |
//!   pending   gate         -         gate        -
//!   window                 *                     *
//! ```
//!
//! `C` is the centre, each `iso` site an isometry towards it. Each `bN` is
//! the bond at cut `N`, which its two neighbours share.

use super::CanonicalMpsError;
use super::fusion::Window;
use super::gate_products::product_on_one_site;
use super::statistics::CanonicalMpsStatistics;
use crate::site_algebra::{CentreSide, SiteAlgebra};
use num_complex::Complex64;
use tensornet::MpsShape;

/// The chain of a state. Between the engine's calls, neighbours share
/// each bond, and every site but the centre is an isometry towards it.
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
    /// The product of the splits' retained fractions.
    /// The chain itself is normalised at each split, so its squared norm
    /// stays about 1.
    pub(super) retained_weight: f64,
    pub(super) statistics: CanonicalMpsStatistics,
}

/// A site tensor `T[a, s, b]`, of extents `[left, 2, right]`.
pub(super) struct Site<T> {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) tensor: T,
}

impl<T> Site<T> {
    pub(super) fn len(&self) -> usize {
        2 * self.left * self.right
    }

    /// The site of `|0⟩`, its bonds of extent 1, uploaded by `algebra`.
    fn zero<A: SiteAlgebra<Tensor = T>>(algebra: &A) -> Result<Self, A::Error> {
        let zero = [Complex64::ONE, Complex64::ZERO];
        algebra.upload(&zero).map(|tensor| Site {
            left: 1,
            right: 1,
            tensor,
        })
    }

    /// A copy of this site, its tensor duplicated by `algebra`.
    fn duplicate<A: SiteAlgebra<Tensor = T>>(&self, algebra: &A) -> Result<Self, A::Error> {
        algebra
            .duplicate(&self.tensor, self.len())
            .map(|tensor| Site {
                left: self.left,
                right: self.right,
                tensor,
            })
    }
}

impl<T> Chain<T> {
    /// |0…0⟩ on `capacity_bonds.len() + 1` sites, the centre on site 0.
    pub(super) fn zero<A: SiteAlgebra<Tensor = T>>(
        algebra: &A,
        capacity_bonds: Vec<usize>,
    ) -> Result<Self, CanonicalMpsError<A::Error>> {
        let site_count = capacity_bonds.len() + 1;
        let sites = (0..site_count)
            .map(|_| Site::zero(algebra))
            .collect::<Result<_, _>>()
            .map_err(CanonicalMpsError::Algebra)?;
        Ok(Self {
            sites,
            pending: vec![None; site_count],
            centre: 0,
            capacity_bonds,
            window: None,
            retained_weight: 1.0,
            statistics: CanonicalMpsStatistics::default(),
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
            .map(|site| site.duplicate(algebra))
            .collect::<Result<_, _>>()
            .map_err(CanonicalMpsError::Algebra)?;
        debug_assert!(self.window.is_none(), "a fork applies the window first");
        Ok(Self {
            sites,
            pending: self.pending.clone(),
            centre: self.centre,
            capacity_bonds: self.capacity_bonds.clone(),
            window: None,
            retained_weight: self.retained_weight,
            statistics: self.statistics,
        })
    }

    /// Applies the one-site gate `gate` (row-major, checked) to `site`. It
    /// goes into the pending window's operator when `site` is one of its
    /// sites, otherwise into the site's pending gate.
    pub(super) fn apply_one(&mut self, site: usize, gate: &[Complex64]) {
        match self.window.as_mut() {
            Some(window) if window.holds(site) => window.absorb(gate, &[site]),
            _ => self.multiply_into_pending_gate(site, std::array::from_fn(|at| gate[at])),
        }
    }

    /// Makes `site`'s pending gate `gate` times the gate already pending,
    /// if any, so that `gate` acts after it.
    fn multiply_into_pending_gate(&mut self, site: usize, gate: [Complex64; 4]) {
        let pending = &mut self.pending[site];
        *pending = Some(match pending {
            Some(earlier) => product_on_one_site(&gate, earlier),
            None => gate,
        });
    }

    /// Replaces the sites `cut` and `cut + 1` by the two tensors a split or
    /// a move of the centre returned. They share a new `bond`, whose size
    /// may differ from the old one's, and keep their outer bonds. Puts the
    /// centre on the site `side` names.
    pub(super) fn replace_pair(
        &mut self,
        cut: usize,
        bond: usize,
        left: T,
        right: T,
        side: CentreSide,
    ) {
        let (left_extent, right_extent) = (self.sites[cut].left, self.sites[cut + 1].right);
        self.sites[cut] = Site {
            left: left_extent,
            right: bond,
            tensor: left,
        };
        self.sites[cut + 1] = Site {
            left: bond,
            right: right_extent,
            tensor: right,
        };
        self.centre = match side {
            CentreSide::Left => cut,
            CentreSide::Right => cut + 1,
        };
    }

    /// The chain's shape: one qubit per site, with the bond each cut holds.
    /// The pending window, if any, has not changed its bonds yet.
    pub(super) fn shape(&self) -> MpsShape {
        let site_count = self.sites.len();
        let sites = self
            .sites
            .iter()
            .enumerate()
            .map(|(at, site)| {
                // The first site has no left bond, the last no right one.
                let left = (at > 0).then_some(site.left);
                let right = (at + 1 < site_count).then_some(site.right);
                left.into_iter().chain([2]).chain(right).collect()
            })
            .collect();
        MpsShape::new(sites).expect("a chain whose neighbours share each bond")
    }
}
