// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsShape`]: the shape of a matrix product state, the extents of its
//! sites, and [`MpsShapeError`], why a list of extents is not one.

/// The shape of a matrix product state: a chain of site tensors, described
/// by the extents of its sites alone.
///
/// It holds no amplitudes, hence a shape and not a state: a state lives in
/// an engine ([`MpsEngine::State`](crate::MpsEngine::State)). It plays two
/// roles:
///
/// - a capacity, how large a state may grow, given to
///   [`MpsEngine::prepare`](crate::MpsEngine::prepare);
/// - the bonds an engine holds now, read back with
///   [`MpsEngine::shape`](crate::MpsEngine::shape): what the run pays for.
///
/// On an engine that truncates, the bonds it holds stay within the capacity
/// ([`MpsShape::fits_within`] checks it). After a measurement, they need
/// not equal the state's exact Schmidt ranks.
///
/// It holds extents and never strides, which belong to a buffer; the
/// crate's `docs/design.md` says why.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MpsShape {
    sites: Vec<Vec<usize>>,
}

impl MpsShape {
    /// Describes a chain from the extents of each site, in chain order.
    ///
    /// The outer vector contains one inner vector per site. A site's extents
    /// are ordered `[bond_left, physical, bond_right]`, with absent boundary
    /// bonds omitted:
    ///
    /// - a lone site is `[physical]`;
    /// - the first site is `[physical, bond_right]`;
    /// - each middle site is `[bond_left, physical, bond_right]`; and
    /// - the last site is `[bond_left, physical]`.
    ///
    /// Each bond is therefore named twice: `bond_right` of one site must equal
    /// `bond_left` of the next. Any positive bond is accepted, even one
    /// larger than the largest Schmidt rank a state can have at its cut
    /// (the dimension of the smaller side, the product of the physical
    /// extents on that side), which no state needs.
    ///
    /// Nothing here is specific to qubits: a physical extent is just a
    /// number, so a chain of qutrits or of fermionic modes is describable
    /// as it is. An engine, which holds qubits, refuses any other site when
    /// it prepares a state
    /// ([`Refusal::NotTwoLevel`](crate::Refusal::NotTwoLevel)).
    ///
    /// # Errors
    ///
    /// The first site, in chain order, that breaks the chain: no sites at
    /// all, a rank that does not match the site's position, an extent of
    /// zero, or more elements than fit a machine word; then the first cut
    /// whose two sites disagree about their bond.
    pub fn new(sites: Vec<Vec<usize>>) -> Result<Self, MpsShapeError> {
        if sites.is_empty() {
            return Err(MpsShapeError::Empty);
        }
        let last = sites.len() - 1;
        for (site, extents) in sites.iter().enumerate() {
            let expected_rank = if sites.len() == 1 {
                1
            } else if site == 0 || site == last {
                2
            } else {
                3
            };
            if extents.len() != expected_rank {
                return Err(MpsShapeError::Rank {
                    site,
                    expected: expected_rank,
                    actual: extents.len(),
                });
            }
            if extents.contains(&0) {
                return Err(MpsShapeError::ZeroExtent { site });
            }
            extents
                .iter()
                .try_fold(1_usize, |elements, extent| elements.checked_mul(*extent))
                .ok_or(MpsShapeError::ElementCountOverflow { site })?;
        }
        for cut in 0..last {
            // The right-hand extent of one site and the left-hand extent of
            // the next are the same bond, named twice.
            let left = sites[cut][sites[cut].len() - 1];
            let right = sites[cut + 1][0];
            if left != right {
                return Err(MpsShapeError::BondMismatch { cut, left, right });
            }
        }
        Ok(Self { sites })
    }

    /// The shape of a chain of `sites` qubits whose every bond is the largest
    /// any state needs at its cut, capped at `max_bond_dimension` (χ).
    ///
    /// The bond at cut `k`, between sites `k` and `k + 1`, is the smaller of
    /// χ and the dimension of the cut's smaller side,
    /// `2^min(k + 1, sites − k − 1)`. For instance, 5 qubits give the bonds
    /// `[2, 2, 2, 2]` with χ = 2, and `[2, 4, 4, 2]` with χ unbounded. Given
    /// to [`MpsEngine::prepare`](crate::MpsEngine::prepare), it is the
    /// capacity of a method whose max bond dimension is χ;
    /// `qubit_chain(n, usize::MAX)` never truncates.
    ///
    /// # Errors
    ///
    /// As [`MpsShape::new`]: no sites; a `max_bond_dimension` of zero on two
    /// sites or more; or a bond so large that a site's element count
    /// overflows (a chain of more than 63 sites with an unbounded
    /// `max_bond_dimension`, on a 64-bit machine).
    pub fn qubit_chain(sites: usize, max_bond_dimension: usize) -> Result<Self, MpsShapeError> {
        let bond = |cut: usize| {
            let exponent = (cut + 1).min(sites - cut - 1);
            let exact = u32::try_from(exponent)
                .ok()
                .and_then(|exponent| 1_usize.checked_shl(exponent))
                .unwrap_or(usize::MAX);
            exact.min(max_bond_dimension)
        };
        Self::new(
            (0..sites)
                .map(|site| {
                    let left = site.checked_sub(1).map(bond);
                    let right = (site + 1 < sites).then(|| bond(site));
                    left.into_iter().chain([2]).chain(right).collect()
                })
                .collect(),
        )
    }

    /// How many sites the chain has.
    #[must_use]
    pub fn site_count(&self) -> usize {
        self.sites.len()
    }

    /// How many levels one site carries, its physical dimension, or `None`
    /// if the chain has no such site.
    ///
    /// The physical axis is the first of a site that opens the chain and the
    /// second of every other, because only the first site has no bond to its
    /// left.
    #[must_use]
    pub fn physical_dimension(&self, site: usize) -> Option<usize> {
        let extents = self.sites.get(site)?;
        extents.get(usize::from(site != 0)).copied()
    }

    /// The dimension of the bond joining site `cut` to site `cut + 1`, or
    /// `None` past the last site.
    #[must_use]
    pub fn bond_dimension(&self, cut: usize) -> Option<usize> {
        if cut.checked_add(1)? >= self.sites.len() {
            return None;
        }
        self.sites.get(cut)?.last().copied()
    }

    /// The largest bond dimension in the chain: on a chain an engine
    /// reports, the bond its costliest gates pay for. A lone site has no
    /// bond, so the answer is one.
    ///
    /// It is a bond the chain holds, not a cap on it: the cap a method sets,
    /// its max bond dimension, is given to [`MpsShape::qubit_chain`].
    #[must_use]
    pub fn largest_bond_dimension(&self) -> usize {
        (0..self.sites.len().saturating_sub(1))
            .filter_map(|cut| self.bond_dimension(cut))
            .max()
            .unwrap_or(1)
    }

    /// How many elements each site's tensor holds, in chain order.
    ///
    /// No product overflows: a site too large to count is rejected when the
    /// chain is described.
    #[must_use]
    pub fn element_counts(&self) -> Vec<usize> {
        self.sites
            .iter()
            .map(|extents| extents.iter().product())
            .collect()
    }

    /// Whether this chain is one an engine may realize within `capacity`:
    /// the same sites, each with the same physical extent, and every bond at
    /// most the capacity's.
    ///
    /// Truncation shrinks bonds and nothing else, so a chain of another
    /// length, or with a site of another physical extent, is not a
    /// truncation of the capacity and does not fit it.
    #[must_use]
    pub fn fits_within(&self, capacity: &Self) -> bool {
        self.sites.len() == capacity.sites.len()
            && (0..self.sites.len()).all(|site| {
                self.physical_dimension(site) == capacity.physical_dimension(site)
                    && (site + 1 == self.sites.len()
                        || self.bond_dimension(site) <= capacity.bond_dimension(site))
            })
    }
}

/// Why a list of site extents is not the shape of a matrix product state.
///
/// Every variant describes the chain alone: a shape that contradicts the
/// chain structure, or a site with more elements than a machine word
/// counts.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MpsShapeError {
    /// A chain with no sites. A matrix product state factorizes something,
    /// and there is nothing to factorize here.
    #[error("the chain has no site")]
    Empty,

    /// A site with the wrong number of axes. The ends of a chain carry one
    /// bond and the interior carries two, so rank is fixed by position.
    #[error("site {site} has {actual} axes but its position in the chain gives it {expected}")]
    Rank {
        /// The site.
        site: usize,
        /// The rank its position gives it.
        expected: usize,
        /// The rank it was given.
        actual: usize,
    },

    /// An axis with no coordinates, which would leave the site, and so the
    /// whole state, with no elements.
    #[error("site {site} has an axis of extent zero")]
    ZeroExtent {
        /// The site.
        site: usize,
    },

    /// Two neighbours disagreeing about the bond they share. The right-hand
    /// axis of one site and the left-hand axis of the next are one bond named
    /// twice, so they cannot differ.
    #[error("the bond between sites {cut} and {} is {left} on one side and {right} on the other", cut + 1)]
    BondMismatch {
        /// The cut: the bond joins sites `cut` and `cut + 1`.
        cut: usize,
        /// The bond as site `cut` names it.
        left: usize,
        /// The bond as site `cut + 1` names it.
        right: usize,
    },

    /// A site whose extents multiply out past what this machine can count,
    /// rejected while describing the chain.
    #[error("site {site} has more elements than fit a machine word")]
    ElementCountOverflow {
        /// The site.
        site: usize,
    },
}

#[cfg(test)]
mod tests;
