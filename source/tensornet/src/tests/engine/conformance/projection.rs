// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit catches defects of projections and forks.

use super::caught_by;
use super::defect::Defect;

#[test]
fn project_catches_a_projection_on_the_other_value() {
    caught_by(Defect::ProjectsOtherValue, "project");
}

/// The state is right, so only the probability returned sees it.
#[test]
fn project_catches_the_other_value_s_probability_returned() {
    caught_by(Defect::ReturnsOtherProbability, "project");
}

#[test]
fn project_catches_an_accepted_value_of_probability_zero() {
    caught_by(Defect::AcceptsZeroOutcome, "project");
}

/// Only exactly zero is refused: a value of probability 1e-18 is not.
#[test]
fn project_catches_a_tiny_probability_refused() {
    caught_by(Defect::ProjectThreshold, "project");
}

/// Only exactly zero is refused: a value of probability 1e-200, which an
/// engine might take for rounding, is not.
#[test]
fn project_catches_a_probability_below_epsilon_squared_refused() {
    caught_by(Defect::ProjectThresholdEpsilonSquared, "project");
}

/// The refusal is right, so only the reads after it see the state changed.
#[test]
fn project_catches_a_state_changed_by_a_refused_projection() {
    caught_by(Defect::ProjectsStateOnZeroOutcome, "project");
}

/// The reads are right, and the chain fits the capacity, so only the
/// bonds compared with the Schmidt ranks see it.
#[test]
fn project_catches_a_chain_below_the_schmidt_ranks_after_a_projection() {
    caught_by(Defect::ReportsProductChainAfterProjection, "project");
}

#[test]
fn project_catches_reads_cached_across_gates() {
    caught_by(Defect::CachesReadsAcrossGates, "project");
}

/// The first projection of the state is right, so only the reads after
/// the later ones see them skipped.
#[test]
fn project_catches_skipped_later_projections() {
    caught_by(Defect::SkipsLaterProjections, "project");
}

/// The reads are right, so only the chain after a projection sees it.
#[test]
fn project_catches_a_chain_beyond_the_capacity_after_a_projection() {
    caught_by(Defect::WidensChainAfterProjection, "project");
}

/// The probability returned is right, so only the reads after the
/// projection see it.
#[test]
fn project_catches_a_value_of_tiny_probability_not_projected() {
    caught_by(Defect::SkipsTinyProjection, "project");
}

#[test]
fn fork_catches_a_fork_that_differs() {
    caught_by(Defect::ForkDiffers, "fork");
}

#[test]
fn fork_catches_a_fork_that_shares_its_state() {
    caught_by(Defect::ForkShares, "fork");
}

/// The first fork is independent, so only the reads of a fork after its
/// own fork changed see the second share it.
#[test]
fn fork_catches_a_fork_of_a_fork_that_shares_its_state() {
    caught_by(Defect::ForkOfForkShares, "fork");
}

/// The fork's reads are right, so only its chain sees it.
#[test]
fn fork_catches_a_fresh_fork_that_reports_another_chain() {
    caught_by(Defect::ForkReportsPreparedShape, "fork");
}

/// Reads are right, so only the original's chain, after its fork's
/// projection, sees it.
#[test]
fn fork_catches_a_chain_shared_with_the_fork() {
    caught_by(Defect::ForkSharesShape, "fork");
}
