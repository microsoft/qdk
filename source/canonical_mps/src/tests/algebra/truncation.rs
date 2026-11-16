// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rule of truncation, [`Truncation`]: the tail below the cutoff, a
//! fraction of the split's own weight, then what exceeds the cap; never
//! the largest value; at a cutoff of 0, the zeros and the values whose
//! squares, against the largest's, underflow; and what is kept,
//! normalised, with its share of the weight; and the arguments it refuses.

use crate::algebra::truncation::Truncation;

/// The singular values whose squares are `weights`.
fn values_of(weights: &[f64]) -> Vec<f64> {
    weights.iter().map(|w| w.sqrt()).collect()
}

/// Values 1, 0.4 and 0.2 at a cutoff just below 1: in floating point, the
/// two smaller squares and the largest sum, from the smallest, to no more
/// than the cutoff's share of the total, yet the largest value is kept.
#[test]
fn a_split_keeps_its_largest_value_at_any_cutoff() {
    let cutoff = 1.0 - f64::EPSILON / 2.0;
    let truncation = Truncation::of(&[1.0, 0.4, 0.2], cutoff, usize::MAX);
    assert_eq!(truncation.normalised_values, [1.0]);
}

/// Schmidt weights 0.8 and 0.2: a cutoff above 0.2 keeps one value and a
/// share of 0.8, one below keeps both.
#[test]
fn cuts_off_the_tail_below_the_cutoff() {
    let values = values_of(&[0.8, 0.2]);
    for (cutoff, bond, share) in [(0.25, 1, 0.8), (0.15, 2, 1.0)] {
        let truncation = Truncation::of(&values, cutoff, usize::MAX);
        assert_eq!(truncation.normalised_values.len(), bond, "cutoff {cutoff}");
        let fraction = truncation.retained_fraction;
        assert!(
            (fraction - share).abs() < 1e-15,
            "cutoff {cutoff}: {fraction}"
        );
    }
}

/// A Bell pair's two Schmidt values, the same float, at a cutoff of 1/2:
/// the smaller value's square is exactly the cutoff's share of the sum,
/// and it is dropped.
// COMPATIBILITY NOTE: this behavior was present in the dev line's
// `qdk_cpu_mps`'s `Engine::kept` (a tail is dropped while its squares sum
// to at most the cutoff's share), hence we keep it: it is what makes a
// cutoff of 0 drop the zeros.
#[test]
fn a_tail_equal_to_the_cutoffs_share_is_discarded() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let truncation = Truncation::of(&[h, h], 0.5, usize::MAX);
    assert_eq!(truncation.normalised_values, [1.0]);
}

/// A cutoff of zero keeps a Schmidt weight of 1e-20, far below the cutoffs
/// simulations use, and drops the zeros after it.
#[test]
fn a_cutoff_of_zero_discards_only_zeros() {
    let truncation = Truncation::of(&[1.0, 1e-10, 0.0, 0.0], 0.0, usize::MAX);
    assert_eq!(truncation.normalised_values.len(), 2);
}

/// A cutoff of zero after a value of 1: a value of 1e-161, whose square is
/// a subnormal, is kept; one of 1e-163, whose square underflows to zero,
/// counts as zero and is dropped, with a share of exactly 1.
// COMPATIBILITY NOTE: this behavior was present in the dev line's
// `qdk_cpu_mps`'s `Engine::kept` (its squares are of the values divided by
// the largest, so those below about 1e-162 of it underflow), hence we keep
// it: no Schmidt weight that small changes a simulation's result.
#[test]
#[expect(clippy::float_cmp, reason = "the share is exactly 1")]
fn a_cutoff_of_zero_drops_values_whose_squares_underflow() {
    let kept = Truncation::of(&[1.0, 1e-161], 0.0, usize::MAX);
    assert_eq!(kept.normalised_values.len(), 2, "1e-161");
    let dropped = Truncation::of(&[1.0, 1e-163], 0.0, usize::MAX);
    assert_eq!(dropped.normalised_values, [1.0], "1e-163");
    assert_eq!(dropped.retained_fraction, 1.0, "1e-163");
}

/// The Schmidt weights 0.81, 0.09, 0.09 and 0.01 (two pairs, each 0.9 and
/// 0.1, cut together) at a cutoff of 0.095 and a cap of 2: the cutoff
/// keeps three, then the cap two, a share of 0.9; the cap applied first
/// would leave the cutoff to drop a 0.09, a bond of 1 and a share of 0.81.
#[test]
fn the_capacity_caps_the_bond_the_cutoff_keeps() {
    let values = values_of(&[0.81, 0.09, 0.09, 0.01]);
    let truncation = Truncation::of(&values, 0.095, 2);
    assert_eq!(truncation.normalised_values.len(), 2);
    let fraction = truncation.retained_fraction;
    assert!((fraction - 0.9).abs() < 1e-15, "{fraction}");
}

/// Values 3, 2 and 1 capped at two: the split keeps 9 + 4 of the 14 the
/// squares sum to.
#[test]
fn the_retained_fraction_is_the_kept_share_of_the_squares() {
    let truncation = Truncation::of(&[3.0, 2.0, 1.0], 0.0, 2);
    let fraction = truncation.retained_fraction;
    assert!((fraction - 13.0 / 14.0).abs() < 1e-15, "{fraction}");
}

/// Values 0.8, 0.6 and two zeros, which a cutoff of 0.1 drops: the share
/// kept is exactly 1, so the state's retained weight does not move by a
/// rounding error at every split that drops only zeros. So too with values
/// 1, four of 2⁻²⁷ and a zero at a cutoff of 0, where the kept squares,
/// summed from the smallest, would round to 1 + 2⁻⁵².
#[test]
#[expect(clippy::float_cmp, reason = "the share is exactly 1")]
fn the_retained_fraction_is_exactly_one_when_only_zeros_are_dropped() {
    let truncation = Truncation::of(&[0.8, 0.6, 0.0, 0.0], 0.1, usize::MAX);
    assert_eq!(truncation.normalised_values.len(), 2, "two zeros dropped");
    assert_eq!(truncation.retained_fraction, 1.0, "two zeros dropped");
    let small = 2f64.powi(-27);
    let values = [1.0, small, small, small, small, 0.0];
    let truncation = Truncation::of(&values, 0.0, usize::MAX);
    assert_eq!(truncation.normalised_values.len(), 5, "small values kept");
    assert_eq!(truncation.retained_fraction, 1.0, "small values kept");
}

/// Values 3, 2 and 1, all kept: the kept values come back divided by their
/// L2 norm, √14.
#[test]
fn the_kept_values_are_divided_by_their_norm() {
    let truncation = Truncation::of(&[3.0, 2.0, 1.0], 0.0, usize::MAX);
    let norm = 14f64.sqrt();
    for (kept, value) in truncation.normalised_values.iter().zip([3.0, 2.0, 1.0]) {
        assert!((kept - value / norm).abs() < 1e-15, "{kept} for {value}");
    }
}

/// The values of a split, scaled by 2⁻¹⁰⁰⁰ (their squares, about 1e-603,
/// would underflow to zero): the same bond, retained fraction and
/// normalised values as at scale 1, to the last bit.
#[test]
fn tiny_values_truncate_as_at_scale_one() {
    let values = [0.9, 0.3, 0.1, 0.01];
    let scale = 2f64.powi(-1000);
    let tiny: Vec<f64> = values.iter().map(|v| v * scale).collect();
    assert_eq!(
        Truncation::of(&tiny, 0.011, usize::MAX),
        Truncation::of(&values, 0.011, usize::MAX)
    );
}

/// The singular values of the zero matrix, which a general SVD gives as
/// `min(rows, cols)` zeros: one value is kept, zero, with a share of 1 and
/// no NaN.
// DISCUSS COMPATIBILITY: the dev line's `qdk_cpu_mps`'s SVD gave the zero
// matrix one value, zero, so its `Engine::kept` and `kept_rescale` never
// met an all-zero spectrum (their `0 / 0` would have failed the split with
// a weight underflow); the engine does not reach it either, since its
// states have norm 1 and its gates are unitary. We think a total rule is
// better than a NaN, and a share of 1 agrees with "exactly 1 when every
// dropped value is zero", so we recommend keeping it.
#[test]
#[expect(clippy::float_cmp, reason = "the share is exactly 1")]
fn the_zero_spectrum_keeps_one_zero_value() {
    let truncation = Truncation::of(&[0.0, 0.0, 0.0], 0.5, usize::MAX);
    assert_eq!(truncation.normalised_values, [0.0]);
    assert_eq!(truncation.retained_fraction, 1.0);
}

/// A cap of zero: no split keeps no value, and the rule refuses it.
#[test]
#[should_panic(expected = "a split keeps at least one value")]
fn a_cap_of_zero_is_refused() {
    let _ = Truncation::of(&[1.0], 0.0, 0);
}

/// No singular values: every split has at least one, and the rule refuses
/// an empty list with a message rather than an index error.
#[test]
#[should_panic(expected = "a split has singular values")]
fn empty_values_are_refused() {
    let _ = Truncation::of(&[], 0.0, 1);
}
