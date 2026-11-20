// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{MpsShape, MpsShapeError};

fn shape(sites: Vec<Vec<usize>>) -> MpsShape {
    MpsShape::new(sites).expect("the test's extents form a chain")
}

fn bonds(shape: &MpsShape) -> Vec<Option<usize>> {
    (0..shape.site_count().saturating_sub(1))
        .map(|cut| shape.bond_dim(cut))
        .collect()
}

/// A chain of three sites whose extents all differ, so that an axis read
/// from the wrong position shows.
#[test]
fn the_physical_axis_is_the_first_of_the_first_site_and_the_second_of_the_others() {
    let shape = shape(vec![vec![3, 4], vec![4, 5, 6], vec![6, 7]]);
    assert_eq!(
        (0..3)
            .map(|site| shape.physical_dim(site))
            .collect::<Vec<_>>(),
        [Some(3), Some(5), Some(7)]
    );
}

#[test]
fn the_bond_at_a_cut_is_the_right_extent_of_its_left_site() {
    let shape = shape(vec![vec![3, 4], vec![4, 5, 6], vec![6, 7]]);
    assert_eq!(bonds(&shape), [Some(4), Some(6)]);
}

#[test]
fn a_site_or_cut_past_the_chain_is_none() {
    let shape = shape(vec![vec![2, 4], vec![4, 2]]);
    assert_eq!((shape.physical_dim(2), shape.bond_dim(1)), (None, None));
}

/// The largest cut a caller can name, one below which overflows.
#[test]
fn a_cut_at_the_largest_machine_word_is_none() {
    let shape = shape(vec![vec![2, 4], vec![4, 2]]);
    assert_eq!(shape.bond_dim(usize::MAX), None);
}

/// Two qubits hold at most a Schmidt rank of 2 at their cut, yet a chain
/// may name a bond of 3 there: a capacity or an engine's buffers can hold
/// more than any state needs.
#[test]
fn a_bond_may_exceed_the_largest_schmidt_rank_at_its_cut() {
    let shape = MpsShape::new(vec![vec![2, 3], vec![3, 2]])
        .expect("positive extents whose neighbours agree form a chain");
    assert_eq!(shape.bond_dim(0), Some(3));
}

#[test]
fn the_element_count_of_a_site_is_the_product_of_its_extents() {
    let shape = shape(vec![vec![3, 4], vec![4, 5, 6], vec![6, 7]]);
    assert_eq!(shape.element_counts(), [12, 120, 42]);
}

#[test]
fn the_largest_bond_is_reported_wherever_it_is() {
    let shape = shape(vec![vec![2, 2], vec![2, 2, 7], vec![7, 2]]);
    assert_eq!(shape.largest_bond(), 7);
}

/// A one-qubit program's capacity: a site with no bond.
#[test]
fn a_lone_site_is_a_chain_whose_largest_bond_is_one() {
    let shape = MpsShape::new(vec![vec![2]]).expect("a lone site is a chain");
    assert_eq!(shape.largest_bond(), 1);
}

#[test]
fn sites_that_are_not_qubits_are_describable() {
    let shape = MpsShape::new(vec![vec![3, 2], vec![2, 5]]);
    assert_eq!(shape.map(|s| s.physical_dim(1)), Ok(Some(5)));
}

#[test]
fn a_chain_needs_a_site() {
    assert_eq!(MpsShape::new(Vec::new()), Err(MpsShapeError::Empty));
}

#[test]
fn a_site_has_the_rank_of_its_position() {
    for (sites, site, expected, actual) in [
        (vec![vec![2, 1]], 0, 1, 2),
        (vec![vec![2, 2, 2], vec![2, 2]], 0, 2, 3),
        (vec![vec![2, 2], vec![2, 2], vec![2, 2]], 1, 3, 2),
        (vec![vec![2, 2], vec![2, 2, 2]], 1, 2, 3),
    ] {
        assert_eq!(
            MpsShape::new(sites.clone()),
            Err(MpsShapeError::Rank {
                site,
                expected,
                actual
            }),
            "{sites:?}"
        );
    }
}

#[test]
fn an_extent_of_zero_is_refused() {
    assert_eq!(
        MpsShape::new(vec![vec![2, 1], vec![1, 0]]),
        Err(MpsShapeError::ZeroExtent { site: 1 })
    );
}

/// Either side may name the larger extent.
#[test]
fn neighbours_agree_about_their_bond() {
    assert_eq!(
        MpsShape::new(vec![vec![2, 4], vec![4, 2, 3], vec![5, 2]]),
        Err(MpsShapeError::BondMismatch {
            cut: 1,
            left: 3,
            right: 5
        })
    );
    assert_eq!(
        MpsShape::new(vec![vec![2, 4], vec![4, 2, 5], vec![3, 2]]),
        Err(MpsShapeError::BondMismatch {
            cut: 1,
            left: 5,
            right: 3
        })
    );
}

#[test]
fn a_site_too_large_to_count_is_refused() {
    assert_eq!(
        MpsShape::new(vec![vec![2, usize::MAX], vec![usize::MAX, 2]]),
        Err(MpsShapeError::ElementCountOverflow { site: 0 })
    );
}

/// The first site at fault is reported, before any bond, so a caller
/// fixes the chain in order.
#[test]
fn a_fault_of_a_site_comes_before_a_bond_mismatch() {
    assert_eq!(
        MpsShape::new(vec![vec![2, 4], vec![5, 2, 3], vec![3, 0]]),
        Err(MpsShapeError::ZeroExtent { site: 2 })
    );
}

#[test]
fn a_qubit_chain_has_the_largest_schmidt_rank_at_each_cut() {
    let chain = MpsShape::qubit_chain(5, usize::MAX).expect("five sites form a chain");
    assert_eq!(bonds(&chain), [Some(2), Some(4), Some(4), Some(2)]);
}

#[test]
fn a_qubit_chain_caps_each_bond_at_its_max_bond_dimension() {
    let chain = MpsShape::qubit_chain(6, 3).expect("six sites form a chain");
    assert_eq!(bonds(&chain), [Some(2), Some(3), Some(3), Some(3), Some(2)]);
}

#[test]
fn a_qubit_chain_of_one_site_is_a_lone_qubit() {
    let chain = MpsShape::qubit_chain(1, 4).expect("one site forms a chain");
    assert_eq!(chain, shape(vec![vec![2]]));
}

#[test]
fn a_qubit_chain_has_two_levels_at_every_site() {
    let chain = MpsShape::qubit_chain(4, 8).expect("four sites form a chain");
    assert!((0..4).all(|site| chain.physical_dim(site) == Some(2)));
}

#[test]
fn a_qubit_chain_refuses_what_new_refuses() {
    assert_eq!(MpsShape::qubit_chain(0, 4), Err(MpsShapeError::Empty));
    assert_eq!(
        MpsShape::qubit_chain(2, 0),
        Err(MpsShapeError::ZeroExtent { site: 0 })
    );
}

/// The largest unbounded chain a 64-bit machine counts: its middle site
/// holds 2⁶³ elements; one more site makes it 2⁶⁴.
#[cfg(target_pointer_width = "64")]
#[test]
fn an_unbounded_qubit_chain_of_more_than_63_sites_overflows() {
    assert!(MpsShape::qubit_chain(63, usize::MAX).is_ok());
    assert!(matches!(
        MpsShape::qubit_chain(64, usize::MAX),
        Err(MpsShapeError::ElementCountOverflow { .. })
    ));
}

#[test]
fn a_chain_with_smaller_bonds_fits_its_capacity() {
    let capacity = MpsShape::qubit_chain(4, 8).expect("four sites form a chain");
    let realized = shape(vec![vec![2, 2], vec![2, 2, 1], vec![1, 2, 2], vec![2, 2]]);
    assert!(realized.fits_within(&capacity));
}

#[test]
fn a_chain_fits_itself() {
    let capacity = MpsShape::qubit_chain(4, 8).expect("four sites form a chain");
    assert!(capacity.fits_within(&capacity));
}

/// One cut over the cap is enough not to fit, wherever it is.
#[test]
fn a_chain_with_a_larger_bond_does_not_fit() {
    let capacity = MpsShape::qubit_chain(4, 2).expect("four sites form a chain");
    let first = shape(vec![vec![2, 3], vec![3, 2, 2], vec![2, 2, 2], vec![2, 2]]);
    let interior = shape(vec![vec![2, 2], vec![2, 2, 3], vec![3, 2, 2], vec![2, 2]]);
    let last = shape(vec![vec![2, 2], vec![2, 2, 2], vec![2, 2, 3], vec![3, 2]]);
    assert!(!first.fits_within(&capacity));
    assert!(!interior.fits_within(&capacity));
    assert!(!last.fits_within(&capacity));
}

#[test]
fn a_chain_of_another_length_does_not_fit() {
    let capacity = MpsShape::qubit_chain(3, 8).expect("three sites form a chain");
    let shorter = MpsShape::qubit_chain(2, 1).expect("two sites form a chain");
    let longer = MpsShape::qubit_chain(4, 1).expect("four sites form a chain");
    assert!(!shorter.fits_within(&capacity));
    assert!(!longer.fits_within(&capacity));
}

/// Truncation shrinks bonds only: a site of another physical extent, larger
/// or smaller, is not a truncation of the capacity.
#[test]
fn a_chain_with_another_physical_extent_does_not_fit() {
    let capacity = MpsShape::qubit_chain(2, 2).expect("two sites form a chain");
    let qutrit = shape(vec![vec![2, 1], vec![1, 3]]);
    let one_level = shape(vec![vec![2, 1], vec![1, 1]]);
    assert!(!qutrit.fits_within(&capacity));
    assert!(!one_level.fits_within(&capacity));
}
