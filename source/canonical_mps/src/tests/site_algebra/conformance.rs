// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The conformance kit against algebras that break one rule each
//! ([`DefectiveFakeAlgebra`]): each case catches the faults its rule names,
//! and the kit accepts an algebra that keeps every rule and reports every
//! case an algebra fails. That the CPU algebra conforms is the
//! integration test `tests/site_algebra_conformance.rs`.

mod defective_fake_algebra;

use crate::site_algebra::conformance::{self, Failure};
use defective_fake_algebra::{Defect, DefectiveFakeAlgebra};

/// The CPU algebra on tensors that a drop fills with NaN and that
/// refuse a read past their end: the kit passes it, at the default seed
/// and at others, so it reads no tensor after dropping it and none past the
/// bond the algebra reported, and its cases hold on other inputs than the
/// default seed's.
#[test]
fn an_algebra_that_keeps_every_rule_passes() {
    let algebra = DefectiveFakeAlgebra::faithful();
    let failures = conformance::check(&algebra);
    assert!(failures.is_empty(), "{failures:#?}");
    for seed in [1, 7, 12_345, u64::MAX] {
        let failures = conformance::check_with_seed(&algebra, seed);
        assert!(failures.is_empty(), "kit seed {seed}: {failures:#?}");
    }
}

/// An algebra whose moves all fail: the kit reports each case that moves
/// the centre, and only those, in the kit's order, rather than stopping
/// at the first.
#[test]
fn every_failing_case_is_reported() {
    let failures = failures(Defect::MoveFails);
    let cases: Vec<_> = failures.iter().map(|failure| failure.case).collect();
    assert_eq!(
        cases,
        [
            "a duplicate is a copy in its own storage",
            "a move gives back the gated pair's product",
            "a move's bond is the smaller of the pair's bond and the side it leaves",
            "a move leaves the site it leaves an isometry",
            "a move leaves its inputs unchanged",
        ],
        "{failures:#?}"
    );
}

/// A backend whose reads return one element short fails every case: each
/// failure names the kit's seed, its inputs' seed derives from it (input
/// `n` of kit seed 7 has seed `7000 + n`), and running the kit again at that
/// seed repeats the failures, so that the backend's author can repeat the
/// scene.
#[test]
fn every_failure_names_the_kit_seed_that_repeats_it() {
    let algebra = DefectiveFakeAlgebra::with(Defect::ShortRead);
    let failures = conformance::check_with_seed(&algebra, 7);
    assert_eq!(
        failures.len(),
        15,
        "a short read fails every case: {failures:#?}"
    );
    for failure in &failures {
        assert_eq!(failure.seed, 7, "{failure}");
        let input_seed = (failure.message.split("of seed ").nth(1))
            .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|digits| digits.parse::<u64>().ok())
            .expect("a failure names its input's seed");
        assert!(
            failure.to_string().contains("kit seed 7") && (7000..8000).contains(&input_seed),
            "the kit's seed and the input's are named: {failure}"
        );
    }
    assert_eq!(
        conformance::check_with_seed(&algebra, 7),
        failures,
        "the seed repeats the failures"
    );
}

#[test]
fn read_case_catches_a_read_of_fewer_or_other_elements() {
    caught_by(
        "a read returns what was uploaded",
        &[Defect::ShortRead, Defect::AlteredRead],
    );
}

#[test]
fn duplicate_case_catches_other_elements_and_shared_storage() {
    caught_by(
        "a duplicate is a copy in its own storage",
        &[Defect::ShiftedDuplicate, Defect::AliasedDuplicate],
    );
}

#[test]
fn untruncated_split_case_catches_a_transposed_gate() {
    caught_by(
        "an untruncated split gives back θ/‖θ‖",
        &[Defect::TransposedGate],
    );
}

#[test]
fn truncated_split_case_catches_other_values_than_the_largest_unnormalised_values_and_nan_sites() {
    caught_by(
        "a truncated split keeps θ's largest values, normalised",
        &[
            Defect::SmallestValuesKept,
            Defect::UnnormalisedValues,
            Defect::NanSitesAtAPositiveCutoff,
        ],
    );
}

#[test]
fn split_bond_case_catches_an_ignored_cap() {
    caught_by(
        "a split's bond lies within its cap and θ's sides",
        &[Defect::CapIgnored],
    );
}

/// The scaled bond keeps the product, the bond and the retained fraction
/// right, at cutoffs above zero only: only the isometry case catches it.
#[test]
fn split_isometry_case_catches_values_absorbed_away_from_the_centre_and_a_scaled_bond() {
    caught_by(
        "a split leaves the site away from the centre an isometry",
        &[
            Defect::CentreOnTheWrongSide,
            Defect::BondScaledAtAPositiveCutoff,
        ],
    );
}

/// Each defect drops more than its cutoff allows: twice the cutoff's
/// share, or, at a cutoff of zero, a value of 1e-8 of the norm.
#[test]
fn cutoff_case_catches_a_split_that_drops_more_than_its_cutoff() {
    caught_by(
        "a split drops at most its cutoff's share",
        &[
            Defect::CutoffDoubled,
            Defect::SmallValuesDroppedAtCutoffZero,
        ],
    );
}

/// A fraction two roundings above 1, within the tolerance of the kept
/// squares' fraction, and a NaN both lie outside `(0, 1]`.
#[test]
fn retained_fraction_case_catches_a_fraction_other_than_that_of_the_kept_squares() {
    caught_by(
        "a split retains the fraction of its kept squares",
        &[
            Defect::RootOfTheRetainedFraction,
            Defect::RetainedFractionAboveOne,
            Defect::NanFractionAtAPositiveCutoff,
        ],
    );
}

/// The defect's retained fraction is one rounding below 1, within the
/// tolerance of `a split retains the fraction of its kept squares`, which so
/// does not catch it.
#[test]
fn drops_nothing_case_catches_a_fraction_below_one() {
    caught_by(
        "a split that drops nothing retains exactly 1",
        &[Defect::RetainedFractionBelowOne],
    );
}

#[test]
fn tie_case_catches_a_tie_taken_one_way_and_reported_the_other_and_nan_outcomes() {
    caught_by(
        "a split at a tie retains the fraction of its bond",
        &[
            Defect::TieTakenOneWayReportedTheOther,
            Defect::NanFractionAtAPositiveCutoff,
            Defect::NanSitesAtAPositiveCutoff,
        ],
    );
}

#[test]
fn split_inputs_case_catches_an_overwritten_input() {
    caught_by(
        "a split leaves its inputs unchanged",
        &[Defect::InputsOverwritten],
    );
}

#[test]
fn move_product_case_catches_a_truncating_move_and_a_gate_on_the_wrong_site() {
    caught_by(
        "a move gives back the gated pair's product",
        &[Defect::TruncatingMove, Defect::GateOnTheWrongSite],
    );
}

#[test]
fn move_bond_case_catches_the_pairs_bond_reported_unchanged() {
    caught_by(
        "a move's bond is the smaller of the pair's bond and the side it leaves",
        &[Defect::MoveBondUnchanged],
    );
}

#[test]
fn move_isometry_case_catches_a_factor_left_on_the_leaving_site() {
    caught_by(
        "a move leaves the site it leaves an isometry",
        &[Defect::FactorLeftOnTheLeavingSite],
    );
}

#[test]
fn move_inputs_case_catches_an_overwritten_input() {
    caught_by(
        "a move leaves its inputs unchanged",
        &[Defect::InputsOverwritten],
    );
}

/// The kit's failures on the CPU algebra with `defect`.
fn failures(defect: Defect) -> Vec<Failure> {
    conformance::check(&DefectiveFakeAlgebra::with(defect))
}

/// Asserts that the kit's `case` fails on the CPU algebra with each
/// of `defects`.
fn caught_by(case: &str, defects: &[Defect]) {
    for &defect in defects {
        let failures = failures(defect);
        assert!(
            failures.iter().any(|failure| failure.case == case),
            "{defect:?} passes `{case}`: {failures:#?}"
        );
    }
}
