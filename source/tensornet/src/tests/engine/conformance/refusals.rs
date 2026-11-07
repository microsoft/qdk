// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit catches defects of refusals.

use super::defect::{Defect, Fault};
use super::{caught_by, caught_saying, failures};

/// Every sample with shots checks its sites, so only the samples of no
/// shots see a check skipped for them.
#[test]
fn refusals_catches_no_shots_of_a_site_out_of_range_accepted() {
    caught_by(Defect::SampleZeroShotsUnchecked, "refusals");
}

/// An engine that returns early for no shots must still check the sites
/// for repeats, not only their range.
#[test]
fn refusals_catches_no_shots_of_a_repeated_site_accepted() {
    caught_saying(
        Defect::SampleZeroShotsChecksRangeOnly,
        "refusals",
        "no shots of a repeated site",
    );
}

#[test]
fn refusals_catches_an_accepted_capacity_of_qutrits() {
    caught_by(Defect::AcceptsQutrits, "refusals");
}

#[test]
fn refusals_catches_an_accepted_matrix_that_is_not_unitary() {
    caught_by(Defect::AcceptsNonUnitary, "refusals");
}

/// The kit's other matrices that are not unitary act on one site.
#[test]
fn refusals_catches_an_accepted_two_site_matrix_that_is_not_unitary() {
    caught_saying(
        Defect::AcceptsNonUnitaryOnTwoSites,
        "refusals",
        "a two-site matrix whose last row is zero",
    );
}

/// An engine may fail a gate on three sites it does not support, but not
/// one that breaks a rule: it must check before it fails.
#[test]
fn refusals_catches_three_site_gates_failed_before_their_checks() {
    caught_by(Defect::FailsBeforeChecksOnThreeSites, "refusals");
}

#[test]
fn refusals_catches_a_three_site_matrix_not_unitary_failed_as_no_refusal() {
    caught_saying(
        Defect::FailsThreeSiteRefusal(Fault::NotUnitary),
        "refusals",
        "a three-site matrix whose last row is zero",
    );
}

#[test]
fn refusals_catches_a_three_site_matrix_of_the_wrong_length_failed_as_no_refusal() {
    caught_saying(
        Defect::FailsThreeSiteRefusal(Fault::MatrixLength),
        "refusals",
        "a two-site matrix on three sites",
    );
}

#[test]
fn refusals_catches_three_sites_one_repeated_failed_as_no_refusal() {
    caught_saying(
        Defect::FailsThreeSiteRefusal(Fault::RepeatedSite),
        "refusals",
        "three sites, one of them repeated",
    );
}

#[test]
fn refusals_catches_three_sites_one_out_of_range_failed_as_no_refusal() {
    caught_saying(
        Defect::FailsThreeSiteRefusal(Fault::SiteOutOfRange),
        "refusals",
        "three sites, one of them out of range",
    );
}

#[test]
fn refusals_catches_an_accepted_matrix_with_an_infinite_entry() {
    caught_by(Defect::AcceptsInfinity, "refusals");
}

/// The product of a thousand gates is the first matrix the kit applies
/// whose deviation exceeds the defect's tolerance: the gates before it are
/// unitary to a few ulps.
#[test]
fn refusals_catches_a_refused_matrix_that_is_unitary_to_rounding() {
    let failures = failures(Defect::RefusesRoundedUnitary);
    assert!(
        matches!(failures.as_slice(), [failure] if failure.case == "refusals"
            && failure.message.starts_with("engine apply on [1, 0]")),
        "{failures:#?}"
    );
}

/// Only a matrix just within the contract's tolerance exceeds the defect's.
#[test]
fn refusals_catches_a_refused_matrix_within_the_unitarity_tolerance() {
    let failures = failures(Defect::RefusesNearUnitary);
    assert!(
        matches!(failures.as_slice(), [failure] if failure.case == "refusals"),
        "{failures:#?}"
    );
}

/// Only a matrix just beyond the contract's tolerance is within the
/// defect's.
#[test]
fn refusals_catches_an_accepted_matrix_just_beyond_the_unitarity_tolerance() {
    let failures = failures(Defect::AcceptsLooseUnitary);
    assert!(
        matches!(failures.as_slice(), [failure] if failure.case == "refusals"),
        "{failures:#?}"
    );
}

/// The site out of range is in the second factor of the second term, so
/// an engine that checks only the first factor of a sum misses it.
#[test]
fn refusals_catches_an_expectation_out_of_range_in_a_later_factor() {
    caught_saying(Defect::ChecksFirstFactorOnly, "refusals", "a later term");
}

/// The kit repeats a site of a sample after another site, which a check
/// that compares neighbours only does not see.
#[test]
fn refusals_catches_a_sample_site_repeated_after_another_accepted() {
    caught_by(Defect::SampleRefusesOnlyAdjacentRepeats, "refusals");
}

/// The kit repeats a site of a condition after another site, which a check
/// that compares neighbours only does not see.
#[test]
fn refusals_catches_a_condition_site_repeated_after_another_accepted() {
    caught_by(Defect::ConditionRefusesOnlyAdjacentRepeats, "refusals");
}

#[test]
fn refusals_catches_the_wrong_refusal() {
    caught_by(Defect::WrongRefusal, "refusals");
}

#[test]
fn refusals_catches_a_refusal_reported_as_a_failure() {
    caught_by(Defect::RefusalAsFailure, "refusals");
}

#[test]
fn refusals_catches_a_refusal_displayed_in_other_words() {
    caught_by(Defect::RewordsRefusal, "refusals");
}

#[test]
fn refusals_catches_a_state_changed_by_a_refusal() {
    caught_by(Defect::ChangesStateWhenRefusing, "refusals");
}
