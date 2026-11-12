// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the conformance kit: every failing case is reported, and each
//! defect is caught by the case that checks its rule, filed by contract
//! area as the kit's cases are. That the reference conforms is tested
//! outside the crate, in `tests/conformance_tests.rs`, as an engine's own
//! tests run the kit.

use crate::conformance::{Failure, check};
use crate::reference::ExactReference;
use defect::Defect;
use double::Double;

mod capacity;
mod conditionals;
mod defect;
mod double;
mod projection;
mod reads;
mod refusals;
mod sampling;
mod schmidt;
mod statistics;

const REFERENCE: ExactReference = ExactReference;

fn failures(defect: Defect) -> Vec<Failure> {
    check(&Double::new(Some(defect)))
}

/// Asserts that `case` is among the cases `defect` fails.
fn caught_by(defect: Defect, case: &str) {
    let failures = failures(defect);
    assert!(
        failures.iter().any(|failure| failure.case == case),
        "{defect:?} should fail {case}: {failures:#?}"
    );
}

/// Asserts that `case` is among the cases `defect` fails, with a message
/// that contains `words`.
fn caught_saying(defect: Defect, case: &str, words: &str) {
    let failures = failures(defect);
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == case && failure.message.contains(words)),
        "{defect:?} should fail {case} saying {words:?}: {failures:#?}"
    );
}

/// A failing case does not stop the others: an engine whose every refusal
/// is a failure fails each case that checks a refusal.
#[test]
fn every_failing_case_is_reported() {
    let cases: Vec<&str> = failures(Defect::RefusalAsFailure)
        .iter()
        .map(|failure| failure.case)
        .collect();
    assert_eq!(
        cases,
        [
            "conditional",
            "project",
            "retained weight",
            "truncated chain",
            "uneven capacity",
            "distant gate within capacity",
            "wide gate truncation",
            "lone site",
            "refusals"
        ]
    );
}
