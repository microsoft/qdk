// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit catches defects of conditional probabilities, and accepts
//! the rounding an engine may leave on a condition impossible in exact
//! arithmetic.

use super::defect::Defect;
use super::double::Double;
use super::{caught_by, caught_saying};
use crate::conformance::check;

/// An fp64 engine that moves its orthogonality centre through a projected
/// site may leave
/// the value the projection removed a probability of rounding size (one
/// measured 3.6e-32), and the contract lets it answer a condition on that
/// value: an engine that answers such conditions, its own projections
/// returning a probability as large as the kit takes for rounding (10⁻²⁰,
/// the documented bound), conforms.
#[test]
fn an_engine_that_answers_an_impossible_condition_left_by_rounding_conforms() {
    let failures = check(&Double::rounding(1e-20, true));
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The kit reads the engine's probability of an impossible condition it
/// answered two ways: by projections of a fork, which may rightly refuse a
/// value of probability exactly zero (the probability is then zero), and
/// along its conditional probabilities. An engine whose projections refuse
/// the value, and whose conditional probabilities keep the condition
/// within rounding, conforms.
#[test]
fn an_engine_that_answers_an_impossible_condition_it_refuses_to_project_conforms() {
    let failures = check(&Double::rounding(1e-20, false));
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Along its conditional path, the kit asks the engine for P(One) given
/// each prefix of the condition it answered; a prefix may be impossible
/// too, and the engine may refuse it, which counts as probability zero. An
/// engine whose rounding leaves only conditions of two values or more a
/// probability, refusing those of one value, conforms.
#[test]
fn an_engine_that_refuses_an_impossible_prefix_of_a_condition_it_answers_conforms() {
    let failures = check(&Double::rounding_on_longer_conditions(1e-20));
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A projection that returns NaN gives no probability the kit could bound:
/// an engine that answers an impossible condition while its projections of
/// a fork return NaN on it fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_projected_as_nan() {
    let failures = check(&Double::rounding(f64::NAN, true));
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == "truncated chain"
                && failure.message.contains("own probability of it is NaN")),
        "{failures:#?}"
    );
}

/// Only rounding excuses an answer to an impossible condition: an engine
/// that leaves the projected value of the five-site chain a probability of
/// 1e-19, just above the kit's bound of 1e-20, fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_left_above_rounding() {
    let failures = check(&Double::rounding(1e-19, true));
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == "truncated chain"
                && failure.message.contains("not within rounding")),
        "{failures:#?}"
    );
}

/// An engine whose projections leave an impossible condition no more than
/// the kit's bound of 1e-20, but whose conditional probabilities give it
/// about 1e-10 along the condition (each within the kit's tolerance of
/// the reference's 0 or 1), answers the condition from a leak, not from
/// rounding: it fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_leaked_along_its_conditional_path() {
    let engine = Double {
        defect: Some(Defect::LeaksConditionalPath),
        ..Double::rounding(1e-20, true)
    };
    let failures = check(&engine);
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == "truncated chain"
                && failure.message.contains("conditional path")),
        "{failures:#?}"
    );
}

/// P(Zero) along the path is one minus P(One), so a P(One) of 1 − 1e-10
/// where the value One is certain leaks 1e-10 on the value Zero: an engine
/// whose conditional path leaks only there, its projections keeping the
/// condition within rounding, fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_leaked_on_its_zero_steps() {
    let engine = Double {
        defect: Some(Defect::LeaksConditionalPathOnZeroSteps),
        ..Double::rounding(1e-20, true)
    };
    let failures = check(&engine);
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == "truncated chain"
                && failure.message.contains("conditional path")),
        "{failures:#?}"
    );
}

/// Fails the test unless `engine` fails the truncated chain, on its
/// conditional path.
fn caught_on_the_conditional_path(engine: &Double) {
    let failures = check(engine);
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == "truncated chain"
                && failure.message.contains("conditional path")),
        "{failures:#?}"
    );
}

/// The path bound is the projections' bound, 1e-20: an engine whose
/// projections leave an impossible condition 1e-20, but whose conditional
/// path leaks 1e-19 on it, just above, fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_leaked_just_above_rounding_along_its_path() {
    caught_on_the_conditional_path(&Double {
        defect: Some(Defect::LeaksConditionalPath),
        leak: 1e-19,
        ..Double::rounding(1e-20, true)
    });
}

/// A negative probability along the path is as much a leak as a positive
/// one: an engine whose conditional path answers −1e-10 where the
/// reference's P(One) is zero, and 1 + 1e-10 where it is one, fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_leaked_below_zero_along_its_conditional_path() {
    caught_on_the_conditional_path(&Double {
        defect: Some(Defect::LeaksConditionalPath),
        leak: -1e-10,
        ..Double::rounding(1e-20, true)
    });
}

/// A P(One) above one by more than rounding, where the value One is
/// certain, is no certain One: P(Zero) = 1 − P(One) is −1e-10 there, a
/// leak below zero. An engine whose conditional path answers 1 + 1e-10
/// there, and nowhere else, fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_leaked_below_zero_on_its_zero_steps() {
    caught_on_the_conditional_path(&Double {
        defect: Some(Defect::LeaksConditionalPathOnZeroSteps),
        leak: -1e-10,
        ..Double::rounding(1e-20, true)
    });
}

/// The projections' bound holds either side of zero: an engine whose
/// projections of a fork return −1e-19 on an impossible value fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_projected_below_zero() {
    let failures = check(&Double::rounding(-1e-19, true));
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == "truncated chain"
                && failure
                    .message
                    .contains("own probability of it is -0.0000000000000000001")),
        "{failures:#?}"
    );
}

/// A Zero value whose P(One) is zero has P(Zero) = 1 − P(One) = 1 along
/// the path: an engine whose conditional path leaks only given Zero values
/// on sites a projection left certain, so that a condition leaks only after
/// such a certain Zero, fails.
#[test]
fn truncated_chain_catches_an_impossible_condition_leaked_after_a_certain_zero() {
    let engine = Double {
        defect: Some(Defect::LeaksConditionalPathGivenCertainZeros),
        ..Double::rounding(1e-20, true)
    };
    let failures = check(&engine);
    assert!(
        failures
            .iter()
            .any(|failure| failure.case == "truncated chain"
                && failure.message.contains("conditional path")),
        "{failures:#?}"
    );
}

/// The kit asks an engine that answers an impossible condition for its own
/// probability of it, by projections of a fork: a projection that fails,
/// with no refusal, fails the case.
#[test]
fn truncated_chain_catches_a_failed_projection_of_an_answered_condition() {
    caught_saying(
        Defect::FailsRoundedProjections,
        "truncated chain",
        "the projection of a fork",
    );
}

/// After a gate on three sites truncates, the reference's own rounding
/// leaves some impossible conditions a probability of about 10⁻³³ instead
/// of zero; an engine whose factorizations compute exactly zero there,
/// and so refuses those conditions, conforms.
#[test]
fn an_engine_that_refuses_a_condition_the_reference_leaves_to_rounding_conforms() {
    let failures = check(&Double::zeroing_rounding());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// An engine may answer a condition the reference's rounding leaves a
/// tiny probability only if its own probability of it is rounding too:
/// one whose projections lift it to 10⁻¹⁰ fails.
#[test]
fn wide_gate_truncation_catches_a_condition_left_above_rounding() {
    caught_saying(
        Defect::LiftsRoundingOutcomes,
        "wide gate truncation",
        "not within rounding",
    );
}

#[test]
fn conditional_catches_an_ignored_condition() {
    caught_by(Defect::IgnoresCondition, "conditional");
}

#[test]
fn conditional_catches_a_condition_read_in_one_order() {
    caught_by(Defect::SkipsUnsortedCondition, "conditional");
}

/// The increasing and decreasing orders are each one direction of a
/// sweep, so only a mixed order sees a sweep that takes its direction from
/// the first two conditions.
#[test]
fn conditional_catches_a_condition_read_in_one_direction_of_a_sweep() {
    caught_by(Defect::SkipsConditionAgainstItsSweep, "conditional");
}

/// The kit reads every condition in decreasing site order too, two sites
/// included, where no mixed order exists.
#[test]
fn conditional_catches_a_condition_skipped_when_its_sites_decrease() {
    caught_by(Defect::SkipsDecreasingCondition, "conditional");
}

#[test]
fn conditional_catches_an_accepted_condition_of_probability_zero() {
    caught_by(Defect::AcceptsZeroCondition, "conditional");
}

/// The scrambled state has no condition of probability zero, so only the
/// GHZ condition asked with its sites in decreasing order sees it.
#[test]
fn conditional_catches_a_condition_of_probability_zero_accepted_in_decreasing_order() {
    caught_by(Defect::AcceptsUnsortedZeroCondition, "conditional");
}

/// The mirror of the case above: only the GHZ condition asked with its sites
/// in increasing order sees it.
#[test]
fn conditional_catches_a_condition_of_probability_zero_accepted_in_increasing_order() {
    caught_by(Defect::AcceptsSortedZeroCondition, "conditional");
}

/// The refusal is right, so only the reads after it see the projection.
#[test]
fn conditional_catches_a_state_projected_by_a_refused_condition() {
    caught_by(Defect::ProjectsStateOnZeroCondition, "conditional");
}

/// On a GHZ state that only gates prepared, the kit takes the impossible
/// condition's probability as exactly 0, as the reference computes it, so
/// even a probability of rounding size is no excuse to answer it.
#[test]
fn conditional_catches_rounding_left_on_a_state_only_gates_prepared() {
    caught_saying(
        Defect::RoundsUnprojectedStates,
        "conditional",
        "a condition of probability zero",
    );
}

/// A phase changes no probability, so only the reads after the
/// conditional probabilities see it.
#[test]
fn conditional_catches_a_conditional_read_that_changes_the_state() {
    caught_by(Defect::ConditionalLeavesPhase, "conditional");
}

/// Only exactly zero is refused: a condition of probability 1e-18 is not.
#[test]
fn conditional_catches_a_tiny_condition_refused() {
    caught_by(Defect::ConditionalThreshold, "conditional");
}

/// Only exactly zero is refused: a condition of probability 1e-200, which
/// an engine might take for rounding, is not.
#[test]
fn conditional_catches_a_condition_below_epsilon_squared_refused() {
    caught_by(Defect::ConditionalThresholdEpsilonSquared, "conditional");
}
