// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Chain`]: what a state holds, its sites in canonical form and the work
//! still pending on them, and the statistics of its history
//! ([`statistics`](super::statistics)).
//!
//! The chain holds, besides its sites, what lets a gate cost no backend
//! call until a split or a move of the centre needs one: one pending
//! one-site gate per site, applied by the next call that touches the site
//! (a unitary on a site's qubit keeps the canonical form), and the pending
//! window ([`Window`]), the product of the gates on up to three sites,
//! applied at once ([`fusion`](super::fusion)).

use super::CanonicalMpsError;
use super::fusion::Window;
use super::gate_products::product_on_one_site;
use super::statistics::CanonicalMpsStatistics;
use crate::site_algebra::SiteAlgebra;
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

    /// Applies the one-site gate `m` (row-major, checked) to `site`: into
    /// the pending window's operator when `site` is one of its sites,
    /// otherwise into the site's pending gate.
    pub(super) fn apply_one(&mut self, site: usize, m: &[Complex64]) {
        match self.window.as_mut() {
            Some(window) if window.holds(site) => window.absorb(m, &[site]),
            _ => {
                let m: [Complex64; 4] = std::array::from_fn(|at| m[at]);
                let pending = &mut self.pending[site];
                *pending = Some(match pending {
                    Some(earlier) => product_on_one_site(&m, earlier),
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
