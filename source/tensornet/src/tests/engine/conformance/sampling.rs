// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit catches defects of sampling: of the frequencies, the
//! independence of the shots, and the seed.

use super::defect::Defect;
use super::{caught_by, caught_saying};

#[test]
fn sample_catches_an_ignored_seed() {
    caught_by(Defect::IgnoresSeed, "sample");
}

#[test]
fn sample_catches_other_samples_for_the_same_seed() {
    caught_by(Defect::ReseedsEachCall, "sample");
}

#[test]
fn sample_catches_values_site_by_site() {
    caught_by(Defect::SamplesSiteMajor, "sample");
}

#[test]
fn sample_catches_values_drawn_with_the_wrong_probability() {
    caught_by(Defect::InvertsSamples, "sample");
}

#[test]
fn sample_catches_shots_that_break_a_correlation() {
    caught_by(Defect::SamplesSitesApart, "sample");
}

#[test]
fn sample_catches_the_wrong_number_of_values() {
    caught_by(Defect::DrawsOneShotTooFew, "sample");
}

/// Every shot is a draw of the right distribution, so every frequency of
/// a joint outcome holds; only the independence of consecutive shots sees
/// it.
#[test]
fn sample_catches_shots_drawn_twice() {
    caught_by(Defect::DrawsEachShotTwice, "sample");
}

/// A bias of 0.02 on a value of probability 0.02 doubles the frequency of
/// two outcomes.
#[test]
fn sample_catches_a_small_bias() {
    caught_by(Defect::SampleBiasedSmall, "sample");
}

/// A bias of 0.02 on a value of probability one half moves an outcome of
/// probability 0.49 to about 0.51, which [`POWER_SHOTS`] shots see:
/// `N·KL` ≈ 115 against a bound of `ln(2/δ)` ≈ 26.0. The double's draws
/// are fixed by the kit's seed, so the test cannot flake; a random draw of
/// this defect would escape with probability below 1e-16 (an 8.3σ
/// shortfall).
#[test]
fn sample_catches_a_small_bias_on_an_even_value() {
    caught_saying(Defect::SampleBiasedEven, "sample", "too far from");
}

/// The outcomes of probability 0.01 never occur.
#[test]
fn sample_catches_rare_outcomes_dropped() {
    caught_by(Defect::SampleDropsRare, "sample");
}

/// Every shot is a draw of the right distribution, so only the
/// independence of shots sees each pair anticorrelated.
#[test]
fn sample_catches_antithetic_shots() {
    caught_by(Defect::SampleAntithetic, "sample");
}

/// The first value of each shot is independent of the shot before, so
/// only the independence of the later values sees it.
#[test]
fn sample_catches_shots_that_copy_the_one_before() {
    caught_by(Defect::SampleReusesPrefixDraws, "sample");
}

/// Each site alone is independent from shot to shot, so only the
/// independence of two sites of consecutive shots sees it.
#[test]
fn sample_catches_a_site_tied_to_another_site_of_the_shot_before() {
    caught_by(Defect::SampleCarriesDraw, "sample");
}

/// One shot in five copies the shot before: at lag 1 each position of the
/// fair coins gives `S` ≈ 800·¼ = 200, against a bound of about 123, so the
/// increment bound `c` of Azuma's inequality must be the kit's (with
/// `c` = 1 the bound is about 492). The double's draws are fixed by the
/// kit's seed, so the test cannot flake; a random draw of this defect would
/// escape with probability below 1e-5 (Azuma–Hoeffding on the 3199 pairs
/// that are not copies: a shortfall of 77, `exp(−2·77²/(3199/4))`).
#[test]
fn sample_catches_one_shot_in_five_copying_the_one_before() {
    caught_saying(
        Defect::SampleCopiesOneShotInFive,
        "sample",
        "not independent",
    );
}

/// Consecutive shots are independent, so only the independence of shots
/// half the samples apart sees it.
#[test]
fn sample_catches_shots_repeated_after_half_the_samples() {
    caught_saying(Defect::SampleRepeatsFirstHalf, "sample", "apart");
}

/// The independence check sees every distance between two shots, not only
/// up to half the samples: repeats 3000 shots apart, of 4000, are caught.
#[test]
fn sample_catches_shots_repeated_more_than_half_the_samples_apart() {
    caught_saying(Defect::SampleRepeatsFirstQuarterLast, "sample", "apart");
}

/// A chunked sampler that seeds every chunk of 8192 shots alike: no two of
/// the 4000 shots whose independence the sums check are 8192 apart, and
/// every shot is a draw of the right distribution, so only the repeat of
/// the 150 000 shots' windows sees it.
#[test]
fn sample_catches_a_stream_restarted_every_8192_shots() {
    caught_saying(Defect::RestartsStreamEvery(8192), "sample", "repeat");
}

/// The farthest restart the repeat check sees: the last 128 of 150 000
/// shots repeat the first 128.
#[test]
fn sample_catches_a_stream_restarted_after_all_but_one_window_of_shots() {
    caught_saying(Defect::RestartsStreamEvery(149_872), "sample", "repeat");
}

#[test]
fn sample_catches_a_seed_whose_high_bits_are_ignored() {
    caught_by(Defect::SeedLow32, "sample");
}

#[test]
fn sample_catches_a_seed_read_by_its_parity() {
    caught_by(Defect::SeedParityOnly, "sample");
}

#[test]
fn sample_catches_a_seed_whose_lowest_bit_is_ignored() {
    caught_by(Defect::SeedDropsLowBit, "sample");
}

#[test]
fn sample_catches_a_seed_whose_highest_bit_is_ignored() {
    caught_by(Defect::SeedDropsTopBit, "sample");
}

/// An `f64` holds every integer up to 2⁵³ only: the seeds `u64::MAX` and
/// the one below it differ in their lowest bit beyond that.
#[test]
fn sample_catches_a_seed_passed_through_a_float() {
    caught_by(Defect::SeedThroughF64, "sample");
}

#[test]
fn sample_catches_a_seed_beyond_a_library_range() {
    caught_by(Defect::NarrowsSeed, "sample");
}

#[test]
fn sample_catches_a_refused_seed_of_zero() {
    caught_by(Defect::RefusesSeedZero, "sample");
}

#[test]
fn sample_catches_values_for_no_site() {
    caught_by(Defect::FillsEmptySample, "sample");
}

/// A phase changes no sample, so only the reads after sampling see it.
#[test]
fn sample_catches_a_sampler_that_changes_the_state() {
    caught_by(Defect::SamplingLeavesPhase, "sample");
}

/// Half the shots all Zero moves each outcome's frequency by about a
/// half of its probability.
#[test]
fn sample_catches_samples_drawn_from_another_distribution() {
    caught_by(Defect::SkewsSamples, "sample");
}

/// Every marginal stays within the frequency tolerance, so only the
/// frequencies of joint outcomes see it.
#[test]
fn sample_catches_shots_whose_joint_outcomes_are_wrong() {
    caught_by(Defect::CorrelatesSamples, "sample");
}

/// One shot of the kit's 4000 moves no frequency beyond the tolerance, so
/// only the Bell pair, never split in any shot, sees it.
#[test]
fn sample_catches_a_single_shot_that_breaks_a_correlation() {
    caught_by(Defect::BreaksOneShot, "sample");
}

/// The Bell pair and its rotated site have real amplitudes, so only the
/// scrambled state's samples see it.
#[test]
fn sample_catches_samples_of_the_real_parts_of_the_amplitudes() {
    caught_by(Defect::SamplesRealParts, "sample");
}
