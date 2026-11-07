// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit catches defects of the chain and its truncation: of the
//! shape, the retained weight, and the reads of a truncated state, a lone
//! site included; and accepts the ways an engine may hold its bonds.

use super::defect::{Defect, Method};
use super::double::Double;
use super::{caught_by, caught_saying};
use crate::conformance::check;

/// The truncation cases expect what an engine that truncates does: no
/// failure for a correct one.
#[test]
fn an_engine_that_truncates_conforms() {
    let failures = check(&Double::new(None));
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The kit asks exact bonds only after gates that truncate nothing: an
/// engine that recomputes a bond only when an operator acts across its cut
/// conforms.
#[test]
fn an_engine_of_lazy_bonds_conforms() {
    let failures = check(&Double::lazy_bonds());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// An engine that moves a site to bring a gate's distant sites together
/// must not truncate where the state fits the capacity in the caller's
/// order: one that caps each cut on the way at twice its bond (two at the
/// narrow cut) loses about a third of the weight of two entangled pairs.
#[test]
fn uneven_capacity_catches_a_route_capped_by_the_narrow_cut() {
    caught_saying(
        Defect::RoutesThroughTheNarrowCut,
        "uneven capacity",
        "the retained weight",
    );
}

/// The kit gates the two pairs with the lower site named first: an engine
/// that routes wrongly in that order only fails.
#[test]
fn uneven_capacity_catches_a_route_wrong_with_the_lower_site_first() {
    caught_saying(
        Defect::RoutesUpThroughTheNarrowCut,
        "uneven capacity",
        "the retained weight",
    );
}

/// The kit gates the two pairs with the higher site named first too: an
/// engine that routes wrongly in that order only fails.
#[test]
fn uneven_capacity_catches_a_route_wrong_with_the_higher_site_first() {
    caught_saying(
        Defect::RoutesDownThroughTheNarrowCut,
        "uneven capacity",
        "the retained weight",
    );
}

/// A gate on distant sites whose result does not fit the capacity must
/// truncate as one on neighbours does: an engine that skips the truncation
/// after its routing keeps the weight the cap drops.
#[test]
fn wide_gate_truncation_catches_a_distant_gate_left_untruncated() {
    caught_saying(
        Defect::SkipsDistantTruncation,
        "wide gate truncation",
        "a gate on sites 0 and 2",
    );
}

/// A gate on three sites whose result does not fit the capacity must
/// truncate too.
#[test]
fn wide_gate_truncation_catches_a_three_site_gate_left_untruncated() {
    caught_saying(
        Defect::SkipsThreeSiteTruncation,
        "wide gate truncation",
        "[2, 0, 1]",
    );
}

/// A gate on distant sites whose result exceeds a bond above one must
/// truncate too: the kit's other distant gates truncate only bonds of one.
#[test]
fn wide_gate_truncation_catches_a_distant_gate_left_untruncated_above_bond_one() {
    caught_saying(
        Defect::SkipsDistantTruncationAboveBondOne,
        "wide gate truncation",
        "a gate on sites 3 and 1",
    );
}

/// Where the kit asks no state, the chain must fit the capacity after a
/// gate on three sites that widens two bonds above one, though the weight
/// fell at one of them.
#[test]
fn wide_gate_truncation_catches_a_three_site_gate_that_truncates_one_cut_of_two() {
    caught_saying(
        Defect::ThreeSiteTruncatesOneWideCut,
        "wide gate truncation",
        "[3, 1, 2] of five sites under a bond of two: the chain",
    );
}

/// Where the kit asks no state, a chain that fits the capacity after a
/// gate whose result does not fit it must still have lost weight.
#[test]
fn wide_gate_truncation_catches_a_distant_truncation_whose_loss_is_forgotten() {
    caught_saying(
        Defect::RenormalizesDistantTruncationAboveBondOne,
        "wide gate truncation",
        "as before the gate",
    );
}

/// Where the kit asks no state, the retained weight must stay above zero.
#[test]
fn wide_gate_truncation_catches_a_weight_that_falls_to_zero() {
    caught_saying(
        Defect::WeightZeroAfterDistantTruncationAboveBondOne,
        "wide gate truncation",
        "the retained weight fell to 0",
    );
}

/// A distant gate's reads are compared after it truncates: an engine
/// whose routing leaves a Z on the site between keeps the weight and the
/// chain, but flips that site's ⟨X⟩.
#[test]
fn wide_gate_truncation_catches_a_routing_that_leaves_a_phase() {
    caught_saying(
        Defect::RoutesWithPhasedSwaps,
        "wide gate truncation",
        "a gate on sites 0 and 2 under a bond of one",
    );
}

/// Only a gate on more than two sites may fail where the kit checks
/// truncation beyond the capacity.
#[test]
fn wide_gate_truncation_catches_a_failed_distant_gate() {
    caught_saying(
        Defect::FailsDistantTruncationAboveBondOne,
        "wide gate truncation",
        "a gate on sites 3 and 1",
    );
}

/// An engine's own cutoff discards Schmidt pairs of tiny weight even where
/// the state fits the capacity; the kit's tolerance absorbs a cutoff of
/// 1e-14 (the CPU engine's default) at every cut after every gate.
#[test]
fn an_engine_with_a_cutoff_of_1e_14_conforms() {
    let failures = check(&Double::with_cutoff(1e-14));
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The reads after a gate truncates the narrow cut are compared: an
/// engine that leaves a Z on a site there keeps the weight, the chain and
/// every P(One), but not ⟨X⟩ of the pair the site belongs to.
#[test]
fn uneven_capacity_catches_a_narrow_cut_truncation_that_leaves_a_phase() {
    caught_saying(
        Defect::PhasesANarrowCutTruncation,
        "uneven capacity",
        "a gate across the narrow cut",
    );
}

/// An engine that caps every cut at the largest bond keeps two Schmidt
/// pairs at a cut capped at one, and with them the weight the cap drops.
/// An engine may apply a product of one-site gates without routing, so
/// only an entangling gate on distant sites whose result fits sees a
/// routing capped at the capacity's bond.
#[test]
fn distant_gate_within_capacity_catches_an_entangling_gate_routed_at_the_bond() {
    caught_by(
        Defect::RoutesEntanglingGatesAtTheBond,
        "distant gate within capacity",
    );
}

/// An engine whose fork copies the block its last split cut, not the
/// truncated one, shows it only where that split lowered a bond above one:
/// the fork of the two-site state under a cap of one, whose last gate
/// truncated a bond of one, cannot see it.
#[test]
fn truncated_chain_catches_a_fork_that_undoes_a_truncation_above_bond_one() {
    caught_saying(
        Defect::ForkUndoesATruncationAboveBondOne,
        "truncated chain",
        "a fork of five sites",
    );
}

/// A fork that copies the state as it was before the last gate's
/// truncation, where that truncation lowered the capacity's narrow cut.
#[test]
fn uneven_capacity_catches_a_fork_that_undoes_a_narrow_cut_truncation() {
    caught_saying(
        Defect::ForkUndoesANarrowCutTruncation,
        "uneven capacity",
        "a fork of a state truncated at the narrow cut",
    );
}

#[test]
fn uneven_capacity_catches_a_narrow_cut_left_uncapped() {
    caught_saying(
        Defect::IgnoresNarrowCuts,
        "uneven capacity",
        "the retained weight is 1",
    );
}

/// Only a fork of a truncated state truncates apart from its original.
#[test]
fn retained_weight_catches_a_weight_shared_with_the_fork() {
    caught_by(Defect::ForkSharesWeight, "retained weight");
}

/// A fork that reads its weight from its original's last operation is
/// seen once the original changes after the fork truncated.
#[test]
fn retained_weight_catches_a_fork_that_reads_its_originals_weight() {
    caught_saying(
        Defect::ForkReadsSharedWeight,
        "retained weight",
        "a fork after its original changed",
    );
}

/// An original that reads its weight from its fork's last operation is
/// seen once the fork truncated apart from it.
#[test]
fn retained_weight_catches_an_original_that_reads_its_forks_weight() {
    caught_saying(
        Defect::OriginalReadsSharedWeight,
        "retained weight",
        "the original after its fork truncated",
    );
}

/// Only the kit's fork of a truncated state sees a fork that differs once
/// a truncation lost weight.
#[test]
fn retained_weight_catches_a_fork_of_a_truncated_state_that_differs() {
    caught_saying(
        Defect::ForkOfTruncatedDiffers,
        "retained weight",
        "a fork of the truncated state",
    );
}

/// A fork of a truncated state must read every expectation its original
/// reads, not only the probability of each value.
#[test]
fn retained_weight_catches_a_fork_of_a_truncated_state_with_a_flipped_phase() {
    caught_saying(
        Defect::ForkOfTruncatedFlipsPhase,
        "retained weight",
        "a fork of the truncated state",
    );
}

/// A fork renormalized reads what its original reads, so only its
/// retained weight shows it.
#[test]
fn retained_weight_catches_a_fork_whose_weight_is_reset() {
    caught_saying(
        Defect::ForkResetsWeight,
        "retained weight",
        "a fork of the truncated state: the retained weight",
    );
}

#[test]
fn shape_catches_a_mirrored_chain() {
    caught_by(Defect::MirrorsShape, "shape");
}

#[test]
fn shape_catches_a_cutoff_that_drops_a_small_schmidt_value() {
    caught_by(Defect::CoarseCutoff, "shape");
}

#[test]
fn retained_weight_catches_an_overstated_weight() {
    caught_by(Defect::OverstatesWeight, "retained weight");
}

#[test]
fn retained_weight_catches_a_weight_other_than_one_without_truncation() {
    caught_by(Defect::WeighsOnlyTruncation, "retained weight");
}

#[test]
fn retained_weight_catches_a_lone_site_weight_other_than_one() {
    caught_saying(Defect::LoneSiteWeight, "retained weight", "a lone site");
}

#[test]
fn retained_weight_catches_a_projection_that_lowers_the_weight() {
    caught_by(Defect::KeepsProjectionNorm, "retained weight");
}

#[test]
fn retained_weight_catches_a_weight_that_rises() {
    caught_saying(Defect::RegainsWeight, "retained weight", "rose");
}

#[test]
fn retained_weight_catches_a_weight_that_turns_nan() {
    caught_saying(Defect::WeightBecomesNan, "retained weight", "fell");
}

/// Zero is never more than an earlier weight, so only the check that the
/// weight stays above zero sees it.
#[test]
fn retained_weight_catches_a_weight_that_falls_to_zero() {
    caught_saying(Defect::WeightFallsToZero, "retained weight", "fell");
}

/// The weight is right before the projection and after the next gate, so
/// only the read right after the projection sees it.
#[test]
fn retained_weight_catches_a_weight_that_is_nan_after_a_projection() {
    caught_saying(
        Defect::WeightNanAfterProjection,
        "retained weight",
        "after a projection",
    );
}

/// The weight is right after the first truncation, and never rises, so
/// only its value after the second truncation sees it.
#[test]
fn retained_weight_catches_a_weight_frozen_at_the_first_truncation() {
    caught_by(Defect::WeightFrozen, "retained weight");
}

/// `1 − ε` and `1 − Σ ε` agree after one truncation, so only the weight
/// after the second sees it.
#[test]
fn retained_weight_catches_a_weight_that_sums_the_losses() {
    caught_by(Defect::WeightOneMinusSum, "retained weight");
}

/// Without truncation the weight is 1, so only the projection of a
/// truncated state sees the probability it returns.
#[test]
fn retained_weight_catches_a_probability_not_divided_by_the_weight() {
    caught_by(Defect::ReturnsUnnormalizedProbability, "retained weight");
}

#[test]
fn retained_weight_catches_a_projection_that_renormalizes() {
    caught_by(Defect::RenormalizesProjection, "retained weight");
}

#[test]
fn truncated_reads_catches_reads_not_divided_by_the_weight() {
    caught_by(Defect::UnnormalizedReads, "truncated reads");
}

#[test]
fn truncated_reads_catches_reads_of_the_state_before_truncation() {
    caught_by(Defect::ReadsBeforeTruncation, "truncated reads");
}

/// The reads right after the truncation are right, so only the reads
/// after a later gate see them stale.
#[test]
fn truncated_reads_catches_reads_of_the_state_the_last_truncation_left() {
    caught_by(Defect::CachesReadsAtTruncation, "truncated reads");
}

/// The reads are right until the second truncation lowers the weight.
#[test]
fn truncated_reads_catches_reads_divided_by_the_first_truncation_s_weight() {
    caught_by(Defect::ReadsByFirstWeight, "truncated reads");
}

#[test]
fn truncated_reads_catches_a_conditional_probability_divided_by_the_weight() {
    caught_by(Defect::ConditionalDividedByWeight, "truncated reads");
}

/// Without truncation ⟨ψ|ψ⟩ is 1 to rounding on the kit's unitary gates, so
/// only the samples of a truncated state see it.
#[test]
fn truncated_reads_catches_samples_drawn_from_the_state_not_normalized() {
    caught_by(Defect::SampleUnnormalized, "truncated reads");
}

/// Two sites have one cut, so only the chains of three and five sites see
/// it.
#[test]
fn truncated_chain_catches_a_truncation_of_the_first_cut_only() {
    caught_by(Defect::TruncatesFirstCutOnly, "truncated chain");
}

/// The truncations at cut 0 are weighed right, so only those at the other
/// cuts see it.
#[test]
fn truncated_chain_catches_a_weight_of_the_first_cut_only() {
    caught_by(Defect::WeighsFirstCutOnly, "truncated chain");
}

#[test]
fn truncated_chain_catches_a_weight_of_one_beyond_two_sites() {
    caught_by(Defect::WeightOneBeyondTwoSites, "truncated chain");
}

#[test]
fn truncated_chain_catches_reads_normalized_on_two_sites_only() {
    caught_by(Defect::NormalizesTwoSitesOnly, "truncated chain");
}

/// A bond of one more keeps every pair of three sites under a bond of one,
/// so only the five sites under a bond of two, whose middle cuts reach
/// rank four, see it.
#[test]
fn truncated_chain_catches_one_pair_kept_beyond_the_bond() {
    caught_by(Defect::KeepsChiPlusOne, "truncated chain");
}

/// The capacity fits itself and holds every Schmidt rank, so only the
/// chain compared with the ranks before a truncation sees it, on a chain
/// whose capacity exceeds the ranks of its first states.
#[test]
fn truncated_chain_catches_the_capacity_reported_as_the_chain() {
    caught_by(Defect::ReportsCapacityUnderCap, "truncated chain");
}

/// The reads are right and every bond is at least the Schmidt rank, so
/// only the chain checked against the capacity after a truncation sees it.
#[test]
fn truncated_reads_catches_a_chain_beyond_the_capacity_after_truncations() {
    caught_by(Defect::WidensChainAfterTruncations, "truncated reads");
}

/// The reads are right and the chain fits the capacity, so only the bonds
/// compared with the Schmidt ranks after a truncation see it, on a chain
/// whose truncated ranks exceed one.
#[test]
fn truncated_chain_catches_a_chain_below_the_schmidt_ranks_after_a_truncation() {
    caught_by(
        Defect::ReportsProductChainAfterTruncation,
        "truncated chain",
    );
}

/// P(One) is right, and so is every Pauli read of two sites, so only the
/// Pauli reads of a chain truncated at an interior cut see it.
#[test]
fn truncated_chain_catches_an_expectation_divided_by_the_first_cut_s_weight() {
    caught_by(Defect::ExpectationByFirstCutWeight, "truncated chain");
}

#[test]
fn lone_site_catches_a_padded_chain() {
    caught_by(Defect::PadsLoneSite, "lone site");
}

/// The padding is hidden from `shape`, so only the refusals of the
/// padding's site see it.
#[test]
fn lone_site_catches_a_hidden_padding_that_takes_its_site() {
    caught_by(Defect::HidesPadding, "lone site");
}

/// An engine whose library pads a lone site may hide the padding from
/// every method but one: the refusal of the padding's site in each method
/// sees it.
#[test]
fn lone_site_catches_the_padding_taken_by_one_site_gates_only() {
    caught_saying(
        Defect::TakesPaddingOnlyIn(Method::OneSiteApply),
        "lone site",
        "apply on site 1 of a lone site",
    );
}

#[test]
fn lone_site_catches_the_padding_taken_by_two_site_gates_only() {
    caught_saying(
        Defect::TakesPaddingOnlyIn(Method::TwoSiteApply),
        "lone site",
        "apply on sites 0 and 1 of a lone site",
    );
}

#[test]
fn lone_site_catches_the_padding_taken_by_probability_of_one_only() {
    caught_saying(
        Defect::TakesPaddingOnlyIn(Method::ProbabilityOfOne),
        "lone site",
        "P(One) of site 1 of a lone site",
    );
}

#[test]
fn lone_site_catches_the_padding_taken_by_project_only() {
    caught_saying(
        Defect::TakesPaddingOnlyIn(Method::Project),
        "lone site",
        "a projection of site 1 of a lone site",
    );
}

#[test]
fn lone_site_catches_the_padding_taken_by_expectation_only() {
    caught_saying(
        Defect::TakesPaddingOnlyIn(Method::Expectation),
        "lone site",
        "an expectation on site 1 of a lone site",
    );
}

#[test]
fn lone_site_catches_the_padding_taken_by_sample_only() {
    caught_saying(
        Defect::TakesPaddingOnlyIn(Method::Sample),
        "lone site",
        "a sample of site 1 of a lone site",
    );
}

/// The chain is right until the projection, so only the chain after it
/// sees the padding.
#[test]
fn lone_site_catches_a_chain_padded_by_a_projection() {
    caught_by(Defect::PadsLoneSiteAtProjection, "lone site");
}

#[test]
fn lone_site_catches_samples_drawn_with_the_wrong_probability() {
    caught_by(Defect::InvertsLoneSiteSamples, "lone site");
}
