// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The contraction networks the CUDA algebra prepares for its moves of the
//! centre, and keeps: a move of a shape seen
//! before prepares no network, and the
//! networks kept are bounded. The move's maths is the contract's, checked
//! by the conformance kit (`the_cuda_algebra_conforms`).

use super::*;
use crate::engine::cuda_algebra::policy::NETWORKS_KEPT;

/// A move of a shape the algebra moved before, on other tensors: it
/// prepares no contraction network, reusing those of the first.
#[test]
fn a_move_of_a_shape_seen_before_prepares_no_network() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(2, 3, 2, 24);
    let (c, d) = random_pair(2, 3, 2, 26);
    let moved = |a, b| Move {
        a,
        b,
        l: 2,
        m: 3,
        r: 2,
        gate: Some(one_qubit_gate(28)),
        centre: CentreSide::Right,
    };
    moved(&a, &b).on(&algebra).expect("the first move");
    let prepared = libraries.count("prepare_contraction");
    moved(&c, &d).on(&algebra).expect("the second move");
    assert_eq!(libraries.count("create_contraction_network"), prepared);
    assert_eq!(libraries.count("prepare_contraction"), prepared);
}

/// Moves of ever new shapes: the algebra keeps the networks of the first
/// `NETWORKS_KEPT`, and the next new shape releases them all before it
/// prepares its own.
#[test]
fn the_networks_kept_are_bounded() {
    let (algebra, libraries) = algebra();
    // To the right without a gate, a move contracts one network, of
    // extents `[m, m]` and `[m, 2, r]` when `m <= 2l`.
    let mut shapes = (1..=32).flat_map(|m| (1..=17).map(move |r| (m, r)));
    let mut move_one = || {
        let (m, r) = shapes.next().expect("enough shapes");
        let (a, b) = random_pair(16, m, r, 29);
        let moved = Move {
            a: &a,
            b: &b,
            l: 16,
            m,
            r,
            gate: None,
            centre: CentreSide::Right,
        };
        moved.on(&algebra).map(drop)
    };
    for _ in 0..NETWORKS_KEPT {
        move_one().expect("a move");
    }
    assert_eq!(libraries.count("create_contraction_network"), NETWORKS_KEPT);
    assert_eq!(libraries.count("destroy_contraction_network"), 0);
    move_one().expect("the move past the bound");
    assert_eq!(
        libraries.count("destroy_contraction_network"),
        NETWORKS_KEPT
    );
}
