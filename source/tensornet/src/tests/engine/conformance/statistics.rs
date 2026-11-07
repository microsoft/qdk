// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests that call the kit's statistics directly, an internal seam: they
//! pin its documented false-alarm bounds (Chernoff's for a frequency,
//! Azuma's for independence) at their exact edges, which `check` cannot
//! reach without contrived engines. The `sample_catches_*` tests, in
//! `sampling.rs`, cover the public path.

use crate::conformance::statistics;

/// The kit fails an outcome whose frequency `f` in `N` shots has
/// `N·KL(f‖p) > ln(2/δ)` ≈ 26.02, with δ = 1e-11: of 150 000 shots of an
/// outcome of probability 0.49, counts of 72 118 and 74 883, either side
/// of it, have `N·KL` ≈ 25.5, and fit.
#[test]
fn a_frequency_within_the_chernoff_bound_fits() {
    for count in [72_118, 74_883] {
        assert!(
            statistics::frequency_fits(count, 150_000, 0.49),
            "{count} draws of 150 000"
        );
    }
}

/// Counts of 72 091 and 74 910, of 150 000 shots of an outcome of
/// probability 0.49, have `N·KL` ≈ 26.5, beyond `ln(2/δ)` ≈ 26.02, and do
/// not fit.
#[test]
fn a_frequency_beyond_the_chernoff_bound_does_not_fit() {
    for count in [72_091, 74_910] {
        assert!(
            !statistics::frequency_fits(count, 150_000, 0.49),
            "{count} draws of 150 000"
        );
    }
}

/// 4000 shots of a site of probability 1/2, in which `changes` of the 3999
/// pairs of neighbouring shots differ, at places a fixed-seed shuffle
/// picks: the sum at lag 1 is `(3999 − 2·changes)/4`, and the other lags
/// look random.
fn shots_with_changes(changes: usize) -> Vec<bool> {
    let mut seed = 0x5EED_u64;
    let mut next = || {
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let mut pairs: Vec<usize> = (0..3999).collect();
    for i in 0..changes {
        let pick = i + usize::try_from(next() % (3999 - i) as u64).expect("below 3999");
        pairs.swap(i, pick);
    }
    let mut differs = [false; 3999];
    for &pair in &pairs[..changes] {
        differs[pair] = true;
    }
    let mut shot = false;
    let mut values = vec![shot];
    for &change in &differs {
        shot ^= change;
        values.push(shot);
    }
    values
}

/// The kit fails a sum beyond `λ = c·√(2n·ln(2T/δ))`, with δ = 4e-9: of
/// 4000 shots of one site of probability 1/2, λ is about 118.99 at lag 1
/// (`c` = 1/4, `n` = `T` = 3999); 1769 changes give a sum of 115.25, within
/// it, and the shots pass.
#[test]
fn shots_whose_sums_are_within_the_azuma_bound_are_independent() {
    let values = shots_with_changes(1769);
    statistics::independent(&values, 1, &[0.5]).expect("a sum of 115.25 at lag 1 is within 118.99");
}

/// 1754 changes give a sum of 122.75 at lag 1, beyond λ ≈ 118.99, and the
/// shots fail.
#[test]
fn shots_whose_sum_is_beyond_the_azuma_bound_are_not_independent() {
    let values = shots_with_changes(1754);
    let error = statistics::independent(&values, 1, &[0.5])
        .expect_err("a sum of 122.75 at lag 1 is beyond 118.99");
    assert!(error.contains("for shots 1 apart"), "{error}");
}
