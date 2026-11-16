// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA algebra's gate split: the rules of `SiteAlgebra`'s contract,
//! on the fake libraries (`algebra_rules.rs`), a report its library could
//! not have made, and the release of what a split made.

use super::*;

/// Pairs, square and not, give the reference algebra's truncated SVD
/// (`algebra_rules.rs`).
#[test]
fn a_split_gives_the_reference_algebras_truncated_svd() {
    let shapes = [(1, 1, 1), (2, 2, 4), (4, 3, 2), (8, 6, 4)];
    algebra_rules::splits_give_the_reference_algebras_truncated_svd(&algebra().0, &shapes, 1e-12);
}

/// The site away from the centre is an isometry (`algebra_rules.rs`).
#[test]
fn a_split_leaves_an_isometry_away_from_the_centre() {
    algebra_rules::a_split_leaves_an_isometry_away_from_the_centre(
        &algebra().0,
        &[(2, 3, 4)],
        1e-12,
    );
}

/// The bond kept lies between 1 and its bounds (`algebra_rules.rs`), on
/// pairs whose sides hold one to eight values.
#[test]
fn the_bond_of_a_split_lies_between_one_and_its_bounds() {
    let shapes = [(1, 2, 1), (1, 2, 4), (4, 2, 1), (2, 3, 2), (4, 1, 4)];
    algebra_rules::the_bond_of_a_split_lies_between_one_and_its_bounds(&algebra().0, &shapes);
}

/// A split returns the bond and share its library reports, which the fake
/// computes from the singular values (`algebra_rules.rs`).
#[test]
fn a_split_returns_the_bond_and_share_it_kept() {
    algebra_rules::a_split_returns_the_bond_and_share_it_kept(&algebra().0);
}

/// A library that reports keeping no value, or more than the split asked
/// for (here 4, either side of the pair), though no more than its SVD had:
/// the report is refused as invalid, rather than read past the factors.
#[test]
fn a_bond_outside_one_to_the_largest_asked_is_refused() {
    for reduced_extent in [-1, 0, 5] {
        assert_refused(SvdTruncation {
            reduced_extent,
            full_extent: 8,
            discarded_weight: 0.0,
        });
    }
}

/// A library that reports keeping more values than its SVD had: the
/// report is refused as invalid.
#[test]
fn a_bond_above_the_full_extent_is_refused() {
    assert_refused(SvdTruncation {
        reduced_extent: 3,
        full_extent: 2,
        discarded_weight: 0.0,
    });
}

/// A library that reports a discarded weight outside `[0, 1)`: the report
/// is refused as invalid, since the share kept must lie in `(0, 1]`.
#[test]
fn a_discarded_weight_outside_zero_to_one_is_refused() {
    for discarded_weight in [-1e-3, 1.0, f64::NAN] {
        assert_refused(SvdTruncation {
            reduced_extent: 2,
            full_extent: 4,
            discarded_weight,
        });
    }
}

/// A split that fails at any of its native calls, after two of the same
/// shape that succeeded (so the algebra holds its scratch already): it
/// fails with that call's failure and leaves no object or buffer it made
/// behind.
#[test]
fn a_failed_split_releases_what_it_made() {
    let (a, b) = random_pair(2, 2, 2, 16);
    let split = Split {
        l: 2,
        m: 2,
        r: 2,
        ..one_qubit_split(&a, &b)
    };
    for operation in [
        "create_tensor_descriptor",
        "create_svd_config",
        "configure_svd",
        "create_svd_info",
        "copy_to_device",
        "synchronize_copies",
        "gate_split_workspace_sizes",
        "workspace_size",
        "set_workspace",
        "allocate",
        "gate_split",
        "synchronize_stream",
        "svd_truncation",
    ] {
        let (algebra, libraries) = algebra();
        let pair = [upload(&algebra, &a), upload(&algebra, &b)];
        let call = split.call(&pair[0], &pair[1]);
        drop(algebra.gate_split(&call).expect("the first split"));
        let live = libraries.live_objects();
        let before = libraries.count(operation);
        drop(algebra.gate_split(&call).expect("the second split"));
        let per_split = libraries.count(operation) - before;
        assert!(per_split > 0, "a split calls {operation}");
        // The split's last call to `operation`, so that every object made
        // before it is released.
        libraries.fail(operation, libraries.count(operation) + per_split);
        let failed = algebra.gate_split(&call).map(drop);
        assert!(
            matches!(
                &failed,
                Err(NativeError::NativeCallFailed { operation: at, .. }) if *at == operation
            ),
            "{operation}: {failed:?}"
        );
        assert_eq!(libraries.live_objects(), live, "{operation}");
    }
}

/// Splits a random pair with a library that reports `truncation`: the
/// split fails with an invalid result.
fn assert_refused(truncation: SvdTruncation) {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(2, 2, 2, 17);
    libraries.misreport_truncation(truncation);
    let split = Split {
        l: 2,
        m: 2,
        r: 2,
        ..one_qubit_split(&a, &b)
    };
    let failed = split.on(&algebra).map(drop);
    assert!(
        matches!(failed, Err(NativeError::InvalidNativeResult { .. })),
        "{truncation:?}: {failed:?}"
    );
}
