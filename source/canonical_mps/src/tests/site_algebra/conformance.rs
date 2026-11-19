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
/// refuse a read past their end: the kit passes it, so it reads no
/// tensor after dropping it and none past the bond the algebra reported.
#[test]
fn an_algebra_that_keeps_every_rule_passes() {
    let failures = conformance::check(&DefectiveFakeAlgebra::faithful());
    assert!(failures.is_empty(), "{failures:#?}");
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
            "duplicate",
            "move product",
            "move bond",
            "move isometry",
            "move inputs unchanged"
        ],
        "{failures:#?}"
    );
}

#[test]
fn upload_and_read_catches_a_read_of_fewer_or_other_elements() {
    caught_by("upload and read", &[Defect::ShortRead, Defect::AlteredRead]);
}

#[test]
fn duplicate_catches_other_elements_and_shared_storage() {
    caught_by(
        "duplicate",
        &[Defect::ShiftedDuplicate, Defect::AliasedDuplicate],
    );
}

/// The failure names the seed of the pair at fault, so that its author can
/// repeat the scene.
#[test]
fn split_without_truncation_catches_a_transposed_gate() {
    caught_by("split without truncation", &[Defect::TransposedGate]);
    let failures = failures(Defect::TransposedGate);
    let failure = (failures.iter())
        .find(|failure| failure.case == "split without truncation")
        .expect("the case fails, as checked above");
    assert!(
        failure.message.contains("of seed 300"),
        "the seed is named: {failure}"
    );
}

#[test]
fn split_truncation_catches_other_values_than_the_largest_unnormalised_values_and_nan_sites() {
    caught_by(
        "split truncation",
        &[
            Defect::SmallestValuesKept,
            Defect::UnnormalisedValues,
            Defect::NanSitesAtAPositiveCutoff,
        ],
    );
}

#[test]
fn split_bond_catches_an_ignored_cap() {
    caught_by("split bond", &[Defect::CapIgnored]);
}

/// The scaled bond keeps the product, the bond and the retained fraction
/// right, at cutoffs above zero only: only the isometry case catches it.
#[test]
fn split_isometry_catches_values_absorbed_away_from_the_centre_and_a_scaled_bond() {
    caught_by(
        "split isometry",
        &[
            Defect::CentreOnTheWrongSide,
            Defect::BondScaledAtAPositiveCutoff,
        ],
    );
}

/// Each defect drops more than its cutoff allows: twice the cutoff's
/// share, or, at a cutoff of zero, a value of 1e-8 of the norm.
#[test]
fn split_cutoff_catches_a_split_that_drops_more_than_its_cutoff() {
    caught_by(
        "split cutoff",
        &[
            Defect::CutoffDoubled,
            Defect::SmallValuesDroppedAtCutoffZero,
        ],
    );
}

/// A fraction two roundings above 1, within the tolerance of the kept
/// squares' fraction, and a NaN both lie outside `(0, 1]`.
#[test]
fn split_retained_fraction_catches_a_fraction_other_than_that_of_the_kept_squares() {
    caught_by(
        "split retains the fraction of its kept squares",
        &[
            Defect::RootOfTheRetainedFraction,
            Defect::RetainedFractionAboveOne,
            Defect::NanFractionAtAPositiveCutoff,
        ],
    );
}

/// The defect's retained fraction is one rounding below 1, within the
/// tolerance of `split retains the fraction of its kept squares`, which so
/// does not catch it.
#[test]
fn split_that_drops_nothing_catches_a_fraction_below_one() {
    caught_by(
        "split that drops nothing retains exactly 1",
        &[Defect::RetainedFractionBelowOne],
    );
}

#[test]
fn split_at_a_tie_catches_a_tie_taken_one_way_and_reported_the_other_and_nan_outcomes() {
    caught_by(
        "split at a tie retains the fraction of its bond",
        &[
            Defect::TieTakenOneWayReportedTheOther,
            Defect::NanFractionAtAPositiveCutoff,
            Defect::NanSitesAtAPositiveCutoff,
        ],
    );
}

#[test]
fn split_inputs_unchanged_catches_an_overwritten_input() {
    caught_by("split inputs unchanged", &[Defect::InputsOverwritten]);
}

#[test]
fn move_product_catches_a_truncating_move_and_a_gate_on_the_wrong_site() {
    caught_by(
        "move product",
        &[Defect::TruncatingMove, Defect::GateOnTheWrongSite],
    );
}

#[test]
fn move_bond_catches_the_pairs_bond_reported_unchanged() {
    caught_by("move bond", &[Defect::MoveBondUnchanged]);
}

#[test]
fn move_isometry_catches_a_factor_left_on_the_leaving_site() {
    caught_by("move isometry", &[Defect::FactorLeftOnTheLeavingSite]);
}

#[test]
fn move_inputs_unchanged_catches_an_overwritten_input() {
    caught_by("move inputs unchanged", &[Defect::InputsOverwritten]);
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
