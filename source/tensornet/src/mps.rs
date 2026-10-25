//! [`MpsShape`]: the shape of a matrix product state, the extents of its
//! sites.
//!
//! It validates the chain's shape (ranks, bonds that agree, no outer bonds)
//! and compares shapes with `fits_within`; it holds no tensor elements and no
//! strides, and it does not build or truncate a chain (a method does that).

use crate::MpsShapeError;

/// The shape of a matrix product state: a chain of site tensors, described
/// by the extents of its sites alone.
///
/// It is named a shape, not a state, because it holds no amplitudes: a
/// state lives in an engine ([`MpsEngine::State`](crate::MpsEngine::State)),
/// and this type says how large it may grow (a capacity, given to
/// [`MpsEngine::prepare`](crate::MpsEngine::prepare)) or how large it grew
/// (read back with [`MpsEngine::shape`](crate::MpsEngine::shape)).
///
/// A general tensor network can express the chain's topology, but not what
/// makes it a matrix product state: that the sites form a line, that each
/// neighbouring pair agrees on the bond they share, and that the two ends
/// carry no outer bond. Those are invariants, and a general network has
/// nowhere to put them; this type holds them. In the other direction a chain
/// cannot express an arbitrary topology, so a general network, when one is
/// described, is a peer of this type rather than its supertype.
///
/// # Extents, but never strides
///
/// A site here is a list of *extents*, and that is deliberately all. Strides
/// — the offsets used to walk a tensor laid out in a flat buffer — are a
/// property of a particular buffer, not of the state. An `MpsShape` carries no
/// tensor elements, so it has no buffer, and a layout without a buffer to lay
/// out is meaningless. Strides therefore travel with the data they describe,
/// in whatever code owns that data.
///
/// The distinction is not pedantic. A backend that truncates bonds writes a
/// tensor smaller than the allocation it was given, and reports the strides it
/// actually used; those strides belong to that one readout. The *shape* of the
/// result is what callers reason about — how large the bonds grew, whether the
/// result fits a requested capacity — and that is what lives here.
///
/// # Roles
///
/// A capacity a caller asks for and the shape a backend produced are both
/// shapes. They are different values, not different types: after
/// truncation the realized shape is bounded by the requested one, which is
/// exactly what [`MpsShape::fits_within`] checks.
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
    /// `bond_left` of the next.
    ///
    /// Nothing here is specific to qubits. A physical extent is just a number,
    /// so a chain of qutrits or of fermionic modes is describable without
    /// change; a caller that requires two-level sites checks for that itself.
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
            // Verify that the site's tensor element count fits in `usize`.
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

    /// The extents of every site, in chain order.
    #[must_use]
    pub fn sites(&self) -> &[Vec<usize>] {
        &self.sites
    }

    /// How many sites the chain has.
    #[must_use]
    pub fn site_count(&self) -> usize {
        self.sites.len()
    }

    /// The extents of one site, or `None` if the chain has no such site.
    #[must_use]
    pub fn site(&self, site: usize) -> Option<&[usize]> {
        self.sites.get(site).map(Vec::as_slice)
    }

    /// How many levels one site carries.
    ///
    /// The physical axis is the first of a site that opens the chain and the
    /// second of every other, because only the first site has no bond to its
    /// left.
    #[must_use]
    pub fn physical_dim(&self, site: usize) -> Option<usize> {
        let extents = self.sites.get(site)?;
        extents.get(usize::from(site != 0)).copied()
    }

    /// The bond joining site `cut` to site `cut + 1`.
    #[must_use]
    pub fn bond_dim(&self, cut: usize) -> Option<usize> {
        if cut + 1 >= self.sites.len() {
            return None;
        }
        self.sites[cut].last().copied()
    }

    /// The largest bond in the chain, and the usual measure of how entangled
    /// a state grew. A lone site has no bond, so its chain is a product state
    /// and the answer is one.
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

    /// Whether this chain fits inside one describing a capacity.
    ///
    /// A backend that truncates returns a chain bounded by the one it was
    /// asked for, site for site and axis for axis. A chain of a different
    /// length, or with a site of a different rank, is not a truncation of this
    /// one and does not fit.
    #[must_use]
    pub fn fits_within(&self, capacity: &Self) -> bool {
        self.sites.len() == capacity.sites.len()
            && self
                .sites
                .iter()
                .zip(&capacity.sites)
                .all(|(realized, allowed)| {
                    realized.len() == allowed.len()
                        && realized
                            .iter()
                            .zip(allowed)
                            .all(|(realized, allowed)| realized <= allowed)
                })
    }
}

#[cfg(test)]
mod tests {
    use super::MpsShape;
    use crate::MpsShapeError;

    fn chain(qubit_count: usize, bond: usize) -> Vec<Vec<usize>> {
        (0..qubit_count)
            .map(|site| {
                if site == 0 {
                    vec![2, bond]
                } else if site + 1 == qubit_count {
                    vec![bond, 2]
                } else {
                    vec![bond, 2, bond]
                }
            })
            .collect()
    }

    #[test]
    fn describes_a_three_site_chain() {
        let shape = MpsShape::new(chain(3, 4)).expect("chain should be valid");

        assert_eq!(shape.site_count(), 3);
        assert_eq!(shape.site(1), Some([4, 2, 4].as_slice()));
        assert_eq!(shape.site(3), None);
        assert_eq!(shape.physical_dim(0), Some(2));
        assert_eq!(shape.physical_dim(1), Some(2));
        assert_eq!(shape.physical_dim(2), Some(2));
        assert_eq!(shape.bond_dim(0), Some(4));
        assert_eq!(shape.bond_dim(2), None);
        assert_eq!(shape.max_bond(), 4);
        assert_eq!(shape.element_counts(), vec![8, 32, 8]);
    }

    #[test]
    fn accepts_a_lone_site() {
        let shape = MpsShape::new(vec![vec![2]]).expect("a single site should be valid");

        assert_eq!(shape.site_count(), 1);
        assert_eq!(shape.physical_dim(0), Some(2));
        assert_eq!(shape.bond_dim(0), None);
        assert_eq!(shape.max_bond(), 1);
    }

    #[test]
    fn accepts_sites_that_are_not_qubits() {
        let shape = MpsShape::new(vec![vec![3, 2], vec![2, 5]]).expect("chain should be valid");

        assert_eq!(shape.physical_dim(0), Some(3));
        assert_eq!(shape.physical_dim(1), Some(5));
    }

    #[test]
    fn reports_the_largest_bond() {
        let shape = MpsShape::new(vec![vec![2, 2], vec![2, 2, 7], vec![7, 2]])
            .expect("chain should be valid");

        assert_eq!(shape.max_bond(), 7);
    }

    #[test]
    fn rejects_an_empty_chain() {
        assert_eq!(MpsShape::new(Vec::new()), Err(MpsShapeError::Empty));
    }

    #[test]
    fn rejects_a_site_of_the_wrong_rank() {
        assert_eq!(
            MpsShape::new(vec![vec![2, 2, 2], vec![2, 2]]),
            Err(MpsShapeError::Rank {
                site: 0,
                expected: 2,
                actual: 3
            })
        );
    }

    #[test]
    fn rejects_a_zero_extent() {
        assert_eq!(
            MpsShape::new(vec![vec![2, 0], vec![0, 2]]),
            Err(MpsShapeError::ZeroExtent { site: 0 })
        );
    }

    #[test]
    fn rejects_neighbours_that_disagree_about_their_bond() {
        assert_eq!(
            MpsShape::new(vec![vec![2, 4], vec![5, 2]]),
            Err(MpsShapeError::BondMismatch {
                cut: 0,
                left: 4,
                right: 5
            })
        );
    }

    #[test]
    fn rejects_a_site_too_large_to_count() {
        assert_eq!(
            MpsShape::new(vec![vec![2, usize::MAX], vec![usize::MAX, 2]]),
            Err(MpsShapeError::ElementCountOverflow { site: 0 })
        );
    }

    #[test]
    fn a_truncated_chain_fits_the_capacity_it_was_asked_for() {
        let capacity = MpsShape::new(chain(3, 8)).expect("chain should be valid");
        let realized = MpsShape::new(chain(3, 3)).expect("chain should be valid");

        assert!(realized.fits_within(&capacity));
        assert!(capacity.fits_within(&capacity));
        assert!(!capacity.fits_within(&realized));
    }

    #[test]
    fn a_chain_of_a_different_shape_does_not_fit() {
        let capacity = MpsShape::new(chain(3, 8)).expect("chain should be valid");

        assert!(
            !MpsShape::new(chain(2, 4))
                .expect("chain should be valid")
                .fits_within(&capacity)
        );
    }
}
