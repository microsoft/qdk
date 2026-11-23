// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA algebra's gate split: a report its library could not have made,
//! and the factors' lengths at the bond kept. The split's maths is the
//! contract's, checked by the conformance kit
//! (`the_cuda_algebra_conforms`), and the release of what
//! a failed split made is `call_scope.rs`'s.

use super::*;
use crate::engine::cuda_algebra::policy::DISCARDED_WEIGHT_TOLERANCE;

/// A library's report of a split (the bond it kept, the values its SVD had, the
/// weight it discarded) is checked before the algebra reads the factors: a
/// report it could not have made is refused as invalid, rather than read past
/// the factors or returned as a retained fraction outside `(0, 1]`. Cases, each
/// on a split asked to keep at most 4 values of a pair whose sides hold 4: a
/// bond of none or more than asked (though no more than the SVD had); a bond
/// above the values the SVD had; a weight below the tolerance, of 1 or more, or
/// NaN. A weight rounded below zero, as an A100 reported for a split that
/// discarded nothing, down to the worst rounding over a split of 4,096 values
/// and the tolerance itself, is read as nothing discarded: the bond reported,
/// the retained fraction exactly 1.
#[test]
fn an_impossible_gate_split_report_is_refused() {
    let report = |reduced_extent, full_extent, discarded_weight| SvdTruncation {
        reduced_extent,
        full_extent,
        discarded_weight,
    };
    let below_the_tolerance = (-DISCARDED_WEIGHT_TOLERANCE).next_down();
    let refused = [
        ("no bond", report(-1, 8, 0.0)),
        ("no bond", report(0, 8, 0.0)),
        ("a bond above the one asked", report(5, 8, 0.0)),
        ("a bond above the full extent", report(3, 2, 0.0)),
        (
            "a weight below the tolerance",
            report(2, 4, below_the_tolerance),
        ),
        ("a weight below the tolerance", report(2, 4, -1e-3)),
        ("a weight of 1", report(2, 4, 1.0)),
        ("a weight that is NaN", report(2, 4, f64::NAN)),
    ];
    for (case, truncation) in refused {
        let split = split_reported_as(truncation);
        assert!(
            matches!(split, Err(CudaAlgebraError::InvalidNativeResult { .. })),
            "{case}, {truncation:?}: {split:?}"
        );
    }
    let worst_rounding_of_4096_values = -4096.0 * f64::EPSILON;
    for discarded_weight in [
        -2.220_446_049_250_313e-16,
        worst_rounding_of_4096_values,
        -DISCARDED_WEIGHT_TOLERANCE,
    ] {
        let kept = split_reported_as(report(2, 4, discarded_weight))
            .expect("a weight rounded below zero, within the tolerance");
        assert_eq!(kept, (2, 1.0), "{discarded_weight:e}");
    }
}

/// A split's factors are sites of extents `[l, 2, bond]` and `[bond, 2, r]`
/// at the bond kept, as `SiteAlgebra` packs them, though their buffers were
/// allocated for the largest bond the split could keep: a read one element
/// past either site is a defect of its caller, refused like any read past a
/// tensor's elements. Cases: a pair of bond 2 whose library reports keeping
/// 1 of the 4 values asked, and `|00⟩` under the identity, a product state,
/// whose split keeps 1 of 2.
#[test]
fn a_read_past_a_split_factors_kept_bond_is_refused() {
    let (a, b) = random_pair(2, 2, 2, 17);
    let zero = [c(1.0, 0.0), c(0.0, 0.0)];
    let identity = std::array::from_fn(|at| c(if at % 5 == 0 { 1.0 } else { 0.0 }, 0.0));
    let reported_as_truncated = Split {
        l: 2,
        m: 2,
        r: 2,
        max_extent: 4,
        ..one_qubit_split(&a, &b)
    };
    let a_product_state = Split {
        gate: identity,
        max_extent: 2,
        ..one_qubit_split(&zero, &zero)
    };
    let report = SvdTruncation {
        reduced_extent: 1,
        full_extent: 4,
        discarded_weight: 0.25,
    };
    for (case, split, misreport) in [
        (
            "reported as keeping 1 of 4",
            reported_as_truncated,
            Some(report),
        ),
        ("a product state", a_product_state, None),
    ] {
        let (algebra, libraries) = algebra();
        if let Some(report) = misreport {
            libraries.misreport_truncation(report);
        }
        let outcome = split.on(&algebra).expect("a split");
        assert_eq!(outcome.bond, 1, "{case}: the bond kept");
        for (side, factor, elements) in [
            ("left", &outcome.left, split.l * 2),
            ("right", &outcome.right, 2 * split.r),
        ] {
            assert_eq!(read(&algebra, factor, elements).len(), elements);
            let past = algebra.read(factor, elements + 1);
            assert!(
                matches!(past, Err(CudaAlgebraError::InvalidNativeCall { .. })),
                "{case}, {side} factor: {past:?}"
            );
        }
    }
}

/// Splits a random pair of `[2, 2, 2]` sites, its library reporting
/// `truncation`: the bond kept and the retained fraction, or the split's
/// failure.
fn split_reported_as(truncation: SvdTruncation) -> Result<(usize, f64), CudaAlgebraError> {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(2, 2, 2, 17);
    libraries.misreport_truncation(truncation);
    let split = Split {
        l: 2,
        m: 2,
        r: 2,
        max_extent: 4,
        ..one_qubit_split(&a, &b)
    };
    split
        .on(&algebra)
        .map(|kept| (kept.bond, kept.retained_fraction))
}
