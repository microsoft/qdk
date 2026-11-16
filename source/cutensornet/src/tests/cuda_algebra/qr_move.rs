// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA algebra's move of the centre: the rules of `SiteAlgebra`'s
//! contract, on the fake libraries (`algebra_rules.rs`), the contraction
//! networks it keeps, and the release of what a move made.

use super::*;
use crate::cuda_algebra::NETWORKS_KEPT;
use algebra_rules::one_qubit_gate;

/// Moves give the reference algebra's product and bond
/// (`algebra_rules.rs`), the bond below, at and above either side of the
/// site the centre leaves.
#[test]
fn a_move_gives_the_reference_algebras_product_and_bond() {
    let shapes = [(1, 1, 1), (2, 2, 4), (4, 3, 2), (1, 4, 2), (2, 5, 1)];
    algebra_rules::moves_give_the_reference_algebras_product_and_bond(&algebra().0, &shapes, 1e-12);
}

/// The site the centre leaves is an isometry (`algebra_rules.rs`).
#[test]
fn a_move_leaves_an_isometry_behind() {
    algebra_rules::a_move_leaves_an_isometry_behind(&algebra().0, &[(2, 3, 2)], 1e-12);
}

/// A move of a shape the algebra moved before, on other tensors: it
/// prepares no contraction network, reusing those of the first.
#[test]
fn a_shape_moved_before_reuses_its_networks() {
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

/// A move of a new shape that fails at any of its native calls, after one
/// of another shape that succeeded (so the algebra holds its scratch and a
/// network already): it fails with that call's failure, leaves no object
/// or buffer it made behind but the networks it prepared, which the
/// algebra keeps (each a network and its path), and keeps no network half
/// made, so the same move then succeeds.
#[test]
fn a_failed_move_releases_what_it_made() {
    let (a, b) = random_pair(2, 2, 2, 30);
    let (c, d) = random_pair(2, 3, 2, 32);
    let moved = |a, b, m| Move {
        a,
        b,
        l: 2,
        m,
        r: 2,
        gate: Some(one_qubit_gate(34)),
        centre: CentreSide::Right,
    };
    let warm = |algebra: &CudaAlgebra| {
        moved(&a, &b, 2).on(algebra).expect("the first move");
    };
    let failing = moved(&c, &d, 3);
    for operation in [
        "allocate",
        "copy_to_device",
        "synchronize_copies",
        "create_tensor_descriptor",
        "create_contraction_network",
        "create_optimizer_info",
        "optimize_contraction",
        "contraction_workspace_sizes",
        "workspace_size",
        "set_workspace",
        "prepare_contraction",
        "contract",
        "qr_workspace_sizes",
        "tensor_qr",
        "synchronize_stream",
    ] {
        // The move's last call to `operation`, so that every object made
        // before it is released: counted on another algebra.
        let per_move = {
            let (algebra, libraries) = algebra();
            warm(&algebra);
            let before = libraries.count(operation);
            failing.on(&algebra).expect("the move");
            libraries.count(operation) - before
        };
        assert!(per_move > 0, "a move calls {operation}");
        let (algebra, libraries) = algebra();
        warm(&algebra);
        let live = libraries.live_objects();
        let prepared = libraries.count("prepare_contraction");
        libraries.fail(operation, libraries.count(operation) + per_move);
        let failed = failing.on(&algebra).map(drop);
        let failed_preparation = usize::from(operation == "prepare_contraction");
        let prepared = libraries.count("prepare_contraction") - prepared - failed_preparation;
        assert!(
            matches!(
                &failed,
                Err(NativeError::NativeCallFailed { operation: at, .. }) if *at == operation
            ),
            "{operation}: {failed:?}"
        );
        assert_eq!(libraries.live_objects(), live + 2 * prepared, "{operation}");
        let retried = failing.on(&algebra).map(drop);
        assert!(retried.is_ok(), "{operation}, the retry: {retried:?}");
    }
}
