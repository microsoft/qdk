// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA algebra's gate split: a report its library could not have made.
//! The report's check is tested directly, at each of its edges; a split
//! is tested once, for reading its report through it. The split's maths,
//! and its factors' lengths at the bond kept, are the contract's, checked
//! by the conformance kit (`the_cuda_algebra_conforms`). The release of
//! what a failed split made is `call_scope/tests.rs`'s.

use super::*;
use crate::engine::{
    cuda_algebra::{
        settings::DISCARDED_WEIGHT_TOLERANCE,
        tests::{Split, random_pair},
    },
    tests::algebra_on_the_fake as algebra,
};

/// A split's report: the bond kept, the values its SVD had, the weight it
/// discarded.
fn report(reduced_extent: i64, full_extent: i64, discarded_weight: f64) -> SvdTruncation {
    SvdTruncation {
        reduced_extent,
        full_extent,
        discarded_weight,
    }
}

/// A library's report of a split is checked before the algebra reads the
/// factors: a report it could not have made is refused as invalid, rather
/// than read past the factors or returned as a retained fraction outside
/// `(0, 1]`. Cases, each on a split that may keep at most 4 values: a bond
/// of none or more than that (though no more than the SVD had); a bond
/// above the values the SVD had; a weight just below the tolerance, or
/// further, of 1, or NaN.
#[test]
fn an_impossible_split_report_is_refused() {
    let below_the_tolerance = (-DISCARDED_WEIGHT_TOLERANCE).next_down();
    for (case, truncation) in [
        ("no bond", report(-1, 8, 0.0)),
        ("no bond", report(0, 8, 0.0)),
        ("a bond above the largest", report(5, 8, 0.0)),
        ("a bond above the full extent", report(3, 2, 0.0)),
        (
            "a weight below the tolerance",
            report(2, 4, below_the_tolerance),
        ),
        ("a weight below the tolerance", report(2, 4, -1e-3)),
        ("a weight of 1", report(2, 4, 1.0)),
        ("a weight that is NaN", report(2, 4, f64::NAN)),
    ] {
        let checked = checked_truncation(truncation, 4);
        assert!(
            matches!(checked, Err(CudaAlgebraError::InvalidNativeResult { .. })),
            "{case}, {truncation:?}: {checked:?}"
        );
    }
}

/// A report the library could have made is read as it says, at each edge
/// of what it may say, on a split that may keep at most 4 values: a bond
/// of 1, and of 4 with the SVD's every value kept; a weight of 0, and just
/// below 1.
#[test]
fn a_possible_split_report_is_read_as_it_says() {
    let almost_all = 1.0_f64.next_down();
    for (truncation, read) in [
        (report(1, 8, 0.0), (1, 0.0)),
        (report(4, 4, 0.0), (4, 0.0)),
        (report(2, 4, almost_all), (2, almost_all)),
    ] {
        let checked = checked_truncation(truncation, 4);
        assert_eq!(checked.ok(), Some(read), "{truncation:?}");
    }
}

/// A weight rounded below zero, as an A100 reported for a split that
/// discarded nothing, down to the worst rounding over a split of 4,096
/// values and the tolerance itself, is read as nothing discarded.
#[test]
fn a_weight_rounded_below_zero_is_read_as_nothing_discarded() {
    let worst_rounding_of_4096_values = -4096.0 * f64::EPSILON;
    for discarded_weight in [
        -2.220_446_049_250_313e-16,
        worst_rounding_of_4096_values,
        -DISCARDED_WEIGHT_TOLERANCE,
    ] {
        let checked = checked_truncation(report(2, 4, discarded_weight), 4);
        assert_eq!(checked.ok(), Some((2, 0.0)), "{discarded_weight:e}");
    }
}

/// A split asked to keep at most 4 values checks its library's report
/// against that bound, and returns the bond reported and the share of the
/// weight not discarded: a report of a bond of 5 is refused, and one of a
/// bond of 2, discarding a quarter of the weight, returns 2 and 3/4.
#[test]
fn a_split_reads_its_report_against_the_bond_it_may_keep() {
    let refused = split_reported_as(report(5, 8, 0.0));
    assert!(
        matches!(refused, Err(CudaAlgebraError::InvalidNativeResult { .. })),
        "{refused:?}"
    );
    let kept = split_reported_as(report(2, 4, 0.25)).expect("a report the library could make");
    assert_eq!(kept, (2, 0.75));
}

/// Splits a random pair of `[2, 2, 2]` sites, keeping at most 4 values,
/// its library reporting `truncation`: the bond kept and the retained
/// fraction, or the split's failure.
fn split_reported_as(truncation: SvdTruncation) -> Result<(usize, f64), CudaAlgebraError> {
    let (algebra, libraries) = algebra();
    let (a, b) = random_pair(2, 2, 2, 17);
    libraries.misreport_truncation(truncation);
    let split = Split {
        max_extent: 4,
        ..Split::untruncated(&a, &b, [2, 2, 2])
    };
    split
        .on(&algebra)
        .map(|kept| (kept.bond, kept.retained_fraction))
}
