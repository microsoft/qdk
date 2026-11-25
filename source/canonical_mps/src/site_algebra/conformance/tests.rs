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
/// refuse a read past their end. The kit passes it, at the default seed
/// and at others. So the kit reads no tensor after dropping it, and none
/// past the bond the algebra reported. Its cases hold on other inputs than
/// the default seed's.
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
            "a move's new sites hold exactly their elements",
        ],
        "{failures:#?}"
    );
}

/// A backend whose reads return one element short fails every case that
/// reads elements back, all but the two whose read past a new site's end
/// it refuses.
///
/// - Each failure names the kit's seed.
/// - Its inputs' seed derives from it: input `n` of kit seed 7 has seed
///   `7000 + n`.
/// - Running the kit again at that seed repeats the failures, so that the
///   backend's author can repeat the scene.
#[test]
fn every_failure_names_the_kit_seed_that_repeats_it() {
    let algebra = DefectiveFakeAlgebra::with(Defect::ShortRead);
    let failures = conformance::check_with_seed(&algebra, 7);
    assert_eq!(
        failures.len(),
        15,
        "a short read fails every case that reads elements back: {failures:#?}"
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

/// Each case of the kit catches the defects that break its rule: the kit
/// run on the CPU algebra with each defect reports a failure of that case.
/// The rows are [`DEFECTS_EACH_CASE_CATCHES`].
#[test]
fn each_case_catches_the_defects_that_break_its_rule() {
    for &(case, defects) in &DEFECTS_EACH_CASE_CATCHES {
        for &defect in defects {
            let failures = failures(defect);
            assert!(
                failures.iter().any(|failure| failure.case == case),
                "{defect:?} passes `{case}`: {failures:#?}"
            );
        }
    }
}

/// Each case of the kit, named by the rule it checks, and the defects it
/// must catch.
const DEFECTS_EACH_CASE_CATCHES: [(&str, &[Defect]); 17] = [
    (
        "a read returns what was uploaded",
        &[Defect::ShortRead, Defect::AlteredRead],
    ),
    (
        "a duplicate is a copy in its own storage",
        &[Defect::ShiftedDuplicate, Defect::AliasedDuplicate],
    ),
    (
        "an untruncated split gives back θ/‖θ‖",
        &[Defect::TransposedGate],
    ),
    (
        "a truncated split keeps θ's largest values, normalised",
        &[
            Defect::SmallestValuesKept,
            Defect::UnnormalisedValues,
            Defect::NanSitesAtAPositiveCutoff,
        ],
    ),
    // A bond padded to the pair's bond leaves `θ`'s sides only on pairs
    // whose bond exceeds a side of `θ`.
    (
        "a split's bond lies within its cap and θ's sides",
        &[
            Defect::CapIgnored,
            Defect::ZeroBond,
            Defect::BondPaddedToThePairsBond,
        ],
    ),
    // The scaled bond keeps the product, the bond and the retained
    // fraction right, at cutoffs above zero only: only the isometry
    // case catches it.
    (
        "a split leaves the site away from the centre an isometry",
        &[
            Defect::CentreOnTheWrongSide,
            Defect::BondScaledAtAPositiveCutoff,
        ],
    ),
    // Each defect drops more than its cutoff allows: twice the cutoff's
    // share, or, at a cutoff of zero, a value of 1e-8 of the norm.
    (
        "a split drops at most its cutoff's share",
        &[
            Defect::CutoffDoubled,
            Defect::SmallValuesDroppedAtCutoffZero,
        ],
    ),
    // A fraction two roundings above 1, within the tolerance of the kept
    // squares' fraction, and a NaN both lie outside `(0, 1]`.
    (
        "a split retains the fraction of its kept squares",
        &[
            Defect::RootOfTheRetainedFraction,
            Defect::RetainedFractionAboveOne,
            Defect::NanFractionAtAPositiveCutoff,
        ],
    ),
    // The defect's retained fraction is one rounding below 1, within the
    // tolerance of `a split retains the fraction of its kept squares`,
    // which so does not catch it.
    (
        "a split that drops nothing retains exactly 1",
        &[Defect::RetainedFractionBelowOne],
    ),
    (
        "a split at a tie retains the fraction of its bond",
        &[
            Defect::TieTakenOneWayReportedTheOther,
            Defect::NanFractionAtAPositiveCutoff,
            Defect::NanSitesAtAPositiveCutoff,
        ],
    ),
    (
        "a split leaves its inputs unchanged",
        &[Defect::InputsOverwritten],
    ),
    (
        "a move gives back the gated pair's product",
        &[Defect::TruncatingMove, Defect::GateOnTheWrongSite],
    ),
    (
        "a move's bond is the smaller of the pair's bond and the side it leaves",
        &[Defect::MoveBondUnchanged],
    ),
    (
        "a move leaves the site it leaves an isometry",
        &[Defect::FactorLeftOnTheLeavingSite],
    ),
    (
        "a move leaves its inputs unchanged",
        &[Defect::InputsOverwritten],
    ),
    // The padded sites keep every element the bond gives them, so only
    // the reads past their ends tell them apart.
    (
        "a split's new sites hold exactly their elements",
        &[Defect::SitesHeldPastTheirEnd],
    ),
    (
        "a move's new sites hold exactly their elements",
        &[Defect::SitesHeldPastTheirEnd],
    ),
];

/// The kit's failures on the CPU algebra with `defect`.
fn failures(defect: Defect) -> Vec<Failure> {
    conformance::check(&DefectiveFakeAlgebra::with(defect))
}
