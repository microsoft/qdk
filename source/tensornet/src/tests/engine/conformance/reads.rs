// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit catches defects of reads of states no truncation touched:
//! P(One), expectations and the chain, after gates on neighbours and on
//! distant sites.

use super::caught_by;
use super::defect::Defect;

/// The weighted sum, alone of the reads, has coefficients that are not
/// real.
#[test]
fn bell_catches_conjugated_coefficients() {
    caught_by(Defect::ConjugatesCoefficients, "bell");
}

/// The weighted sum, alone of the reads, has terms with coefficients that
/// are not real.
#[test]
fn bell_catches_coefficients_read_by_their_real_parts() {
    caught_by(Defect::ReadsRealCoefficients, "bell");
}

/// The weighted sum has many terms, so the defect leaves it alone: only the
/// reads of each Pauli string on its own see it.
#[test]
fn bell_catches_a_lone_string_read_coarsely() {
    caught_by(Defect::RoundsLoneStrings, "bell");
}

/// The weighted sum, alone of the reads, has more than one term.
#[test]
fn bell_catches_an_observable_read_by_its_first_term() {
    caught_by(Defect::ReadsFirstTermOnly, "bell");
}

/// The reads that follow see the phase a read left on the state.
#[test]
fn bell_catches_a_read_that_changes_the_state() {
    caught_by(Defect::ReadingLeavesPhase, "bell");
}

/// A NaN is close to nothing, so the weighted sum's read fails.
#[test]
fn bell_catches_a_sum_read_as_nan() {
    caught_by(Defect::ReadsNanForSums, "bell");
}

/// Only the identity alone has no term.
#[test]
fn bell_catches_the_identity_alone_read_as_zero() {
    caught_by(Defect::ReadsIdentityAloneAsZero, "bell");
}

/// The weighted sum repeats Z on every site, which reads one on the Bell
/// state.
#[test]
fn bell_catches_repeated_terms_read_once() {
    caught_by(Defect::MergesRepeatedTerms, "bell");
}

/// A NaN is close to nothing, so the first read of P(One) of site 1 fails.
#[test]
fn bell_catches_a_probability_read_as_nan() {
    caught_by(Defect::ReadsNanProbability, "bell");
}

/// The probabilities of the scrambled state are not representable in
/// single precision, so their rounding exceeds the kit's tolerance.
#[test]
fn layout_catches_reads_in_single_precision() {
    caught_by(Defect::ReadsInSinglePrecision, "layout");
}

/// The Bell state is symmetric under the swap of its sites, so only the
/// weighted sum on the scrambled state sees each Pauli moved.
#[test]
fn layout_catches_paulis_left_in_place_when_sites_are_sorted() {
    caught_by(Defect::SortsSitesNotPaulis, "layout");
}

/// Factors in increasing or decreasing site order are paired right, so
/// only the weighted sum's terms whose factors are rotated see it.
#[test]
fn layout_catches_paulis_paired_right_only_in_a_monotone_site_order() {
    caught_by(Defect::MixedOrderPairing, "layout");
}

/// Factors in increasing order, or rotated from it, are paired right (two
/// factors in decreasing order are a rotation), so only the weighted sum's
/// terms of three factors in decreasing order see it.
#[test]
fn layout_catches_paulis_paired_right_only_in_rotations_of_increasing_order() {
    caught_by(Defect::RotationPairing, "layout");
}

#[test]
fn layout_catches_swapped_sites() {
    caught_by(Defect::SwapsSites, "layout");
}

#[test]
fn distant_sites_catches_routing_left_in_place() {
    caught_by(Defect::LeaksRouting, "distant sites");
}

#[test]
fn distant_sites_catches_a_stale_chain() {
    caught_by(Defect::CachesShape, "distant sites");
}
