// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsShape`]: the shape of a matrix product state, the extents of its
//! sites, and [`MpsShapeError`], why a list of extents is not one.
//!
//! It validates the chain's shape (ranks by position, bonds that agree, no
//! outer bonds) and compares shapes with [`MpsShape::fits_within`]; it holds
//! no tensor elements and no strides, and it does not build or truncate a
//! chain (an engine does that). Every error is raised while describing,
//! before any engine is involved, so no message carries vendor vocabulary.

/// The shape of a matrix product state: a chain of site tensors, described
/// by the extents of its sites alone.
///
/// It is named a shape, not a state, because it holds no amplitudes: a
/// state lives in an engine ([`MpsEngine::State`](crate::MpsEngine::State)),
/// and this type says how large it may grow (a capacity, given to
/// [`MpsEngine::prepare`](crate::MpsEngine::prepare)) or the bonds an engine
/// holds now (read back with [`MpsEngine::shape`](crate::MpsEngine::shape)):
/// what the run pays for, at most the capacity on an engine that truncates,
/// and not the state's exact Schmidt ranks after a measurement.
///
/// A capacity a caller asks for and the shape an engine realized are both
/// shapes: after truncation the realized shape is bounded by the requested
/// one, which is what [`MpsShape::fits_within`] checks. It holds extents and
/// never strides, which belong to a buffer; the crate's `design.md`
/// says why.
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
    /// Nothing here is specific to qubits. A physical extent is just a number,
    /// so a chain of qutrits or of fermionic modes is describable without
    /// change; an engine, which holds qubits, refuses any other site when it
    /// prepares a state
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

    /// The capacity of `sites` qubits whose bonds are at most `max_bond`.
    ///
    /// The bond at cut `k` (between sites `k` and `k + 1`) is the smaller of
    /// `max_bond` and the largest Schmidt rank a state of `sites` qubits can
    /// have there, `2^min(k + 1, sites − k − 1)`: no bond asks for more than
    /// any state needs. So `qubit_chain(n, usize::MAX)` never truncates, and
    /// `qubit_chain(n, χ)` is the usual cap of a method that truncates at
    /// bond χ.
    ///
    /// # Errors
    ///
    /// As [`MpsShape::new`]: no sites; a `max_bond` of zero on two sites or
    /// more; or a bond so large that a site's element count overflows (a
    /// chain of more than 63 sites with an unbounded `max_bond`, on a 64-bit
    /// machine).
    pub fn qubit_chain(sites: usize, max_bond: usize) -> Result<Self, MpsShapeError> {
        let bond = |cut: usize| {
            let exponent = (cut + 1).min(sites - cut - 1);
            let exact = u32::try_from(exponent)
                .ok()
                .and_then(|exponent| 1_usize.checked_shl(exponent))
                .unwrap_or(usize::MAX);
            exact.min(max_bond)
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

    /// How many levels one site carries, or `None` if the chain has no such
    /// site.
    ///
    /// The physical axis is the first of a site that opens the chain and the
    /// second of every other, because only the first site has no bond to its
    /// left.
    #[must_use]
    pub fn physical_dim(&self, site: usize) -> Option<usize> {
        let extents = self.sites.get(site)?;
        extents.get(usize::from(site != 0)).copied()
    }

    /// The bond joining site `cut` to site `cut + 1`, or `None` past the
    /// last site.
    #[must_use]
    pub fn bond_dim(&self, cut: usize) -> Option<usize> {
        if cut.checked_add(1)? >= self.sites.len() {
            return None;
        }
        self.sites.get(cut)?.last().copied()
    }

    /// The largest bond in the chain: on a chain an engine reports, the
    /// bond its costliest gates pay for. A lone site has no bond, so the
    /// answer is one.
    #[must_use]
    pub fn max_bond(&self) -> usize {
        (0..self.sites.len().saturating_sub(1))
            .filter_map(|cut| self.bond_dim(cut))
            .max()
            .unwrap_or(1)
    }

    /// How many elements each site's tensor holds, in chain order.
    ///
    /// Every product is known to be answerable: a site too large to count is
    /// rejected when the chain is described.
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
                self.physical_dim(site) == capacity.physical_dim(site)
                    && (site + 1 == self.sites.len()
                        || self.bond_dim(site) <= capacity.bond_dim(site))
            })
    }
}

/// Why a list of site extents is not the shape of a matrix product state.
///
/// Every variant describes a shape that contradicts the chain structure
/// itself, so none of them mention an engine, a buffer or a memory layout.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MpsShapeError {
    /// A chain with no sites. A matrix product state factorizes something,
    /// and there is nothing to factorize here.
    #[error("a matrix product state needs at least one site")]
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

    /// A site whose extents multiply out past what this machine can count.
    /// Rejected while describing the chain, so that every later element count
    /// is known to be answerable.
    #[error("site {site} has more elements than fit a machine word")]
    ElementCountOverflow {
        /// The site.
        site: usize,
    },
}
