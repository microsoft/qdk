// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The chain's layout as engines and capacities read it (its axes,
//! bonds, element counts and the qubit chain), the chains it refuses,
//! and when a realized chain fits a capacity.

use crate::{MpsShape, MpsShapeError};

fn shape(sites: Vec<Vec<usize>>) -> MpsShape {
    MpsShape::new(sites).expect("the test's extents form a chain")
}

fn bonds(shape: &MpsShape) -> Vec<Option<usize>> {
    (0..shape.site_count().saturating_sub(1))
        .map(|cut| shape.bond_dimension(cut))
        .collect()
}

/// A chain of three sites whose extents all differ, so that an axis read
/// from the wrong position shows.
#[test]
fn the_physical_axis_is_the_first_of_the_first_site_and_the_second_of_the_others() {
    let shape = shape(vec![vec![3, 4], vec![4, 5, 6], vec![6, 7]]);
    assert_eq!(
        (0..3)
            .map(|site| shape.physical_dimension(site))
            .collect::<Vec<_>>(),
        [Some(3), Some(5), Some(7)]
    );
}

/// The same chain: an engine sizes the bond between two sites from the
/// site on its left.
#[test]
fn the_bond_at_a_cut_is_the_right_extent_of_its_left_site() {
    let shape = shape(vec![vec![3, 4], vec![4, 5, 6], vec![6, 7]]);
    assert_eq!(bonds(&shape), [Some(4), Some(6)]);
}

/// A caller asking past the chain gets no extent, not a panic.
#[test]
fn a_site_or_cut_past_the_chain_is_none() {
    let shape = shape(vec![vec![2, 4], vec![4, 2]]);
    assert_eq!(
        (shape.physical_dimension(2), shape.bond_dimension(1)),
        (None, None)
    );
    // The largest cut, whose `cut + 1` would overflow.
    assert_eq!(shape.bond_dimension(usize::MAX), None);
}

/// Two qubits hold at most a Schmidt rank of 2 at their cut, yet a chain
/// may name a bond of 3 there: a capacity or an engine's buffers can hold
/// more than any state needs.
#[test]
fn a_bond_may_exceed_the_largest_schmidt_rank_at_its_cut() {
    let shape = MpsShape::new(vec![vec![2, 3], vec![3, 2]])
        .expect("positive extents whose neighbours agree form a chain");
    assert_eq!(shape.bond_dimension(0), Some(3));
}

/// The count an engine allocates each site's buffer for.
#[test]
fn the_element_count_of_a_site_is_the_product_of_its_extents() {
    let shape = shape(vec![vec![3, 4], vec![4, 5, 6], vec![6, 7]]);
    assert_eq!(shape.element_counts(), [12, 120, 42]);
}

/// The largest bond dimension wherever it is, here at the last cut, not
/// the first; and on a one-qubit program's capacity, a lone site with no
/// bond, one.
#[test]
fn the_largest_bond_dimension_is_reported_wherever_it_is() {
    let rows = [
        (
            "at the last cut",
            shape(vec![vec![2, 2], vec![2, 2, 7], vec![7, 2]]),
            7,
        ),
        ("a lone site", shape(vec![vec![2]]), 1),
    ];
    for (scene, shape, largest) in rows {
        assert_eq!(shape.largest_bond_dimension(), largest, "{scene}");
    }
}

/// A chain of a qutrit and a five-level site: the shape describes any
/// levels, and only an engine's check refuses what it cannot hold.
#[test]
fn a_chain_may_have_sites_of_any_physical_dimension() {
    let shape = MpsShape::new(vec![vec![3, 2], vec![2, 5]]);
    assert_eq!(shape.map(|s| s.physical_dimension(1)), Ok(Some(5)));
}

/// A program with no qubit has no chain to describe.
#[test]
fn a_chain_needs_a_site() {
    assert_eq!(MpsShape::new(Vec::new()), Err(MpsShapeError::Empty));
}

/// The first and last sites have two axes, the others three, and a lone
/// site one: a site of the wrong rank is reported with both ranks.
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

/// A site of no element holds no state.
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

/// A site whose element count does not fit a `usize` is never allocated.
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

/// With no cap, each bond is the largest Schmidt rank at its cut,
/// `2^min(left, right)`: what an exact simulation needs.
#[test]
fn a_qubit_chain_has_the_largest_schmidt_rank_at_each_cut() {
    let chain = MpsShape::qubit_chain(5, usize::MAX).expect("five sites form a chain");
    assert_eq!(bonds(&chain), [Some(2), Some(4), Some(4), Some(2)]);
}

/// A simulator's χ caps the bonds above it; those below it stay.
#[test]
fn a_qubit_chain_caps_each_bond_at_its_max_bond_dimension() {
    let chain = MpsShape::qubit_chain(6, 3).expect("six sites form a chain");
    assert_eq!(bonds(&chain), [Some(2), Some(3), Some(3), Some(3), Some(2)]);
}

/// A one-qubit program's chain, whatever its χ.
#[test]
fn a_qubit_chain_of_one_site_is_a_lone_qubit() {
    let chain = MpsShape::qubit_chain(1, 4).expect("one site forms a chain");
    assert_eq!(chain, shape(vec![vec![2]]));
}

/// Every site of a qubit chain is a qubit.
#[test]
fn a_qubit_chain_has_two_levels_at_every_site() {
    let chain = MpsShape::qubit_chain(4, 8).expect("four sites form a chain");
    assert!((0..4).all(|site| chain.physical_dimension(site) == Some(2)));
}

/// No qubit, or a χ of 0, is refused as `MpsShape::new` refuses it.
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

/// A chain fits a capacity when it differs from it by smaller bonds only,
/// since truncation shrinks bonds and nothing else. Accepted: a state
/// truncated below its capacity's bonds, which an engine's buffers still
/// hold, and a state at its capacity, which an untruncated run reaches.
/// Refused: one cut over the cap, wherever it is (the first, an interior
/// one, the last); a chain of fewer or more sites, another program's; and
/// a site of another physical extent, larger or smaller.
#[test]
fn a_chain_fits_a_capacity_when_it_differs_from_it_by_smaller_bonds_only() {
    let qubits = |sites, bond| MpsShape::qubit_chain(sites, bond).expect("a chain of qubits");
    let rows = [
        (
            "truncated",
            shape(vec![vec![2, 2], vec![2, 2, 1], vec![1, 2, 2], vec![2, 2]]),
            qubits(4, 8),
            true,
        ),
        ("at its capacity", qubits(4, 8), qubits(4, 8), true),
        (
            "a larger first bond",
            shape(vec![vec![2, 3], vec![3, 2, 2], vec![2, 2, 2], vec![2, 2]]),
            qubits(4, 2),
            false,
        ),
        (
            "a larger interior bond",
            shape(vec![vec![2, 2], vec![2, 2, 3], vec![3, 2, 2], vec![2, 2]]),
            qubits(4, 2),
            false,
        ),
        (
            "a larger last bond",
            shape(vec![vec![2, 2], vec![2, 2, 2], vec![2, 2, 3], vec![3, 2]]),
            qubits(4, 2),
            false,
        ),
        ("fewer sites", qubits(2, 1), qubits(3, 8), false),
        ("more sites", qubits(4, 1), qubits(3, 8), false),
        (
            "a qutrit",
            shape(vec![vec![2, 1], vec![1, 3]]),
            qubits(2, 2),
            false,
        ),
        (
            "a one-level site",
            shape(vec![vec![2, 1], vec![1, 1]]),
            qubits(2, 2),
            false,
        ),
    ];
    for (scene, chain, capacity, fits) in rows {
        assert_eq!(chain.fits_within(&capacity), fits, "{scene}");
    }
}
