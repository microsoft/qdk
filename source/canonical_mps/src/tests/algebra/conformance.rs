// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The conformance kit against algebras that break one rule each
//! ([`DefectiveFakeAlgebra`]): each case catches the faults its rule names,
//! and the kit accepts an algebra that keeps every rule and reports every
//! case an algebra fails. That the reference algebra conforms is the
//! integration test `tests/algebra_conformance.rs`.

mod defective_fake_algebra;

use crate::algebra::conformance::{self, Failure};
use defective_fake_algebra::{Defect, DefectiveFakeAlgebra};

/// The reference algebra on tensors that a drop fills with NaN and that
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
fn upload_and_read_catches_a_read_of_fewer_elements() {
    caught_by("upload and read", &[Defect::ShortRead]);
}

#[test]
fn duplicate_catches_other_elements_and_shared_storage() {
    caught_by(
        "duplicate",
        &[Defect::ShiftedDuplicate, Defect::AliasedDuplicate],
    );
}

#[test]
fn split_without_truncation_catches_a_transposed_gate() {
    caught_by("split without truncation", &[Defect::TransposedGate]);
}

#[test]
fn split_truncation_catches_other_values_than_the_largest_and_values_not_normalised() {
    caught_by(
        "split truncation",
        &[Defect::SmallestValuesKept, Defect::UnnormalisedValues],
    );
}

#[test]
fn split_bond_catches_an_ignored_cap() {
    caught_by("split bond", &[Defect::CapIgnored]);
}

#[test]
fn split_isometry_catches_values_absorbed_away_from_the_centre() {
    caught_by("split isometry", &[Defect::CentreOnTheWrongSide]);
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

#[test]
fn split_share_catches_a_share_other_than_that_of_the_kept_squares() {
    caught_by("split share", &[Defect::RootOfTheShare]);
}

/// The defect's share is one rounding below 1, within the tolerance of
/// `split share`, which so does not catch it.
#[test]
fn split_cutoff_of_zero_catches_a_share_below_one_when_nothing_is_dropped() {
    caught_by("split cutoff of zero", &[Defect::ShareBelowOne]);
}

#[test]
fn split_tie_catches_a_tie_taken_one_way_and_reported_the_other() {
    caught_by("split tie", &[Defect::TieTakenOneWayReportedTheOther]);
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

/// The kit's failures on the reference algebra with `defect`.
fn failures(defect: Defect) -> Vec<Failure> {
    conformance::check(&DefectiveFakeAlgebra::with(defect))
}

/// Asserts that the kit's `case` fails on the reference algebra with each
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
