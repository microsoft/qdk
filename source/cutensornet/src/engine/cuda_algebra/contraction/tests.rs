// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The contraction networks the CUDA algebra prepares for its moves of the
//! centre, and keeps: a move of a shape seen before prepares no network,
//! and the networks kept are bounded. The move's maths is the contract's,
//! checked by the conformance kit (`the_cuda_algebra_conforms`).

use crate::engine::{
    cuda_algebra::{
        settings::NETWORKS_KEPT,
        tests::{Move, move_of_a_new_shape, one_qubit_gate, random_pair},
    },
    tests::algebra_on_the_fake as algebra,
};

/// A move of a shape the algebra moved before, on other tensors: it
/// prepares no contraction network, reusing those of the first.
#[test]
fn a_move_of_a_shape_seen_before_prepares_no_network() {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(2, 3, 2, 24);
    let (c, d) = random_pair(2, 3, 2, 26);
    let moved = |a, b| Move {
        gate: Some(one_qubit_gate(28)),
        ..Move::to_the_right(a, b, [2, 3, 2])
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
    for shape in 0..NETWORKS_KEPT {
        move_of_a_new_shape(&algebra, shape).expect("a move");
    }
    assert_eq!(libraries.count("create_contraction_network"), NETWORKS_KEPT);
    assert_eq!(libraries.count("destroy_contraction_network"), 0);
    move_of_a_new_shape(&algebra, NETWORKS_KEPT).expect("the move past the bound");
    assert_eq!(
        libraries.count("destroy_contraction_network"),
        NETWORKS_KEPT
    );
}
