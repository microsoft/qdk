// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA algebra's gate split: a report its library could not have made.
//! The split's maths is the contract's, checked by the conformance kit
//! (`the_cuda_algebra_conforms`), and the release of what
//! a failed split made is `call_scope.rs`'s.

use super::*;
use crate::engine::cuda_algebra::DISCARDED_WEIGHT_TOLERANCE;

/// A library's report of a split (the bond it kept, the values its SVD
/// had, the weight it discarded) is checked before the algebra reads the
/// factors: a report it could not have made is refused as invalid, rather
/// than read past the factors or returned as a share outside `(0, 1]`.
/// Cases, each on a split asked to keep at most 4 values of a pair whose
/// sides hold 4: a bond of none or more than asked (though no more than
/// the SVD had); a bond above the values the SVD had; a weight below the
/// tolerance, of 1 or more, or NaN. A weight rounded below zero, as an A100
/// reported for a split that discarded nothing, down to the worst rounding
/// over a split of 4,096 values and the tolerance itself, is read as
/// nothing discarded: the bond reported, the share kept exactly 1.
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

/// Splits a random pair of `[2, 2, 2]` sites, its library reporting
/// `truncation`: the bond and share kept, or the split's failure.
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
