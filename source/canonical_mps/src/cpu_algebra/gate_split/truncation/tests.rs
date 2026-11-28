// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CPU algebra's rule of truncation, [`Truncation::of`], called on
//! singular values written out.
//!
//! - It drops the tail below the cutoff, a fraction of the values' own
//!   weight, then what exceeds the cap.
//! - It never drops the largest value.
//! - At a cutoff of 0, it drops the zeros and the values whose squares,
//!   against the largest, underflow.
//! - It keeps the values divided by their norm, with their share of the
//!   weight.

use super::Truncation;
use crate::cpu_algebra::tests::panic_message;

/// The singular values whose squares are `weights`.
fn values_of(weights: &[f64]) -> Vec<f64> {
    weights.iter().map(|weight| weight.sqrt()).collect()
}

/// The cutoff drops the longest tail whose squares sum to at most its
/// share of the values' weight, but never the largest value. At a cutoff
/// of 0, it drops only the zeros and the values whose squares underflow.
/// Each row: the values, the cutoff, and the bond kept.
#[test]
fn the_cutoff_discards_the_tail_within_its_share() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let rows: [(&str, Vec<f64>, f64, usize); 7] = [
        // In floating point, the two smaller squares and the largest sum,
        // from the smallest, to no more than the cutoff's share of the
        // total, yet the largest value is kept.
        (
            "the largest at a cutoff just below 1",
            vec![1.0, 0.4, 0.2],
            1.0 - f64::EPSILON / 2.0,
            1,
        ),
        ("a tail above the cutoff", values_of(&[0.8, 0.2]), 0.25, 1),
        ("a tail below the cutoff", values_of(&[0.8, 0.2]), 0.15, 2),
        // A Bell pair's two Schmidt values, the same float: the smaller
        // value's square is exactly the cutoff's share of the sum, and is
        // dropped. A tail is dropped while its squares sum to at most the
        // share, not below it, so that a cutoff of 0 drops the zeros.
        ("a tail of exactly the cutoff's share", vec![h, h], 0.5, 1),
        // A Schmidt weight of 1e-20, far below the cutoffs simulations
        // use, is kept.
        ("zeros at a cutoff of 0", vec![1.0, 1e-10, 0.0, 0.0], 0.0, 2),
        // 1e-170 squared, about 1e-340, underflows to 0; 1e-161 squared,
        // about 1e-322, does not.
        (
            "a value whose square underflows, at a cutoff of 0",
            vec![1.0, 1e-170],
            0.0,
            1,
        ),
        (
            "a value whose square does not underflow, at a cutoff of 0",
            vec![1.0, 1e-161],
            0.0,
            2,
        ),
    ];
    for (name, values, cutoff, bond) in rows {
        assert_eq!(
            Truncation::of(&values, cutoff, usize::MAX).bond(),
            bond,
            "{name}"
        );
    }
}

/// The Schmidt weights 0.81, 0.09, 0.09 and 0.01 (two pairs, each 0.9 and
/// 0.1, cut together) at a cutoff of 0.095 and a cap of 2. The cutoff
/// keeps three, then the cap two, a retained fraction of 0.9. The cap
/// applied first would leave the cutoff to drop a 0.09: a bond of 1 and a
/// retained fraction of 0.81.
#[test]
fn the_capacity_caps_the_bond_the_cutoff_keeps() {
    let truncation = Truncation::of(&values_of(&[0.81, 0.09, 0.09, 0.01]), 0.095, 2);
    assert_eq!(truncation.bond(), 2);
    let fraction = truncation.retained_fraction;
    assert!((fraction - 0.9).abs() < 1e-15, "{fraction}");
}

/// The retained fraction is the share of the squares kept, and exactly 1
/// when only zeros were dropped, so that the state's retained weight does
/// not move by a rounding error at every such split. Each row: the values,
/// the cutoff, the cap, the bond kept (a precondition), the expected
/// fraction, and how far from it the fraction may round.
#[test]
fn the_retained_fraction_is_that_of_the_kept_squares() {
    type Row = (&'static str, Vec<f64>, f64, usize, usize, f64, f64);
    let small = 2f64.powi(-27);
    let rows: [Row; 5] = [
        // 9 + 4 of the 14 the squares sum to.
        (
            "3, 2 and 1 capped at two",
            vec![3.0, 2.0, 1.0],
            0.0,
            2,
            2,
            13.0 / 14.0,
            1e-15,
        ),
        (
            "a tail above the cutoff",
            values_of(&[0.8, 0.2]),
            0.25,
            usize::MAX,
            1,
            0.8,
            1e-15,
        ),
        (
            "a tail below the cutoff",
            values_of(&[0.8, 0.2]),
            0.15,
            usize::MAX,
            2,
            1.0,
            1e-15,
        ),
        (
            "two zeros dropped",
            vec![0.8, 0.6, 0.0, 0.0],
            0.1,
            usize::MAX,
            2,
            1.0,
            0.0,
        ),
        // The kept squares, summed from the smallest, would round to
        // 1 + 2⁻⁵².
        (
            "four values of 2⁻²⁷ kept and a zero dropped",
            vec![1.0, small, small, small, small, 0.0],
            0.0,
            usize::MAX,
            5,
            1.0,
            0.0,
        ),
    ];
    for (name, values, cutoff, cap, bond, expected, tolerance) in rows {
        let truncation = Truncation::of(&values, cutoff, cap);
        assert_eq!(truncation.bond(), bond, "precondition: {name}");
        let fraction = truncation.retained_fraction;
        assert!(
            (fraction - expected).abs() <= tolerance,
            "{name}: {fraction} for {expected}"
        );
    }
}

/// Values 3, 2 and 1, all kept: they are divided by their L2 norm, √14.
#[test]
fn the_kept_values_are_divided_by_their_norm() {
    let truncation = Truncation::of(&[3.0, 2.0, 1.0], 0.0, usize::MAX);
    let norm = 14f64.sqrt();
    assert_eq!(truncation.bond(), 3, "precondition: all kept");
    for (kept, value) in truncation.normalised_values.iter().zip([3.0, 2.0, 1.0]) {
        assert!((kept - value / norm).abs() < 1e-15, "{kept} for {value}");
    }
}

/// Values scaled by 2⁻¹⁰⁰⁰, whose squares (about 1e-603) would underflow
/// to zero: the same truncation as at scale 1, to the last bit.
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

/// The values of a zero gated pair, all zero (`Svd::of` gives one): one
/// value, 0, with a retained fraction of 1 and no NaN. The fraction, the
/// kept weight over the whole, is 0 over 0 here; the truncation answers 1,
/// as it does whenever the dropped values are all zero, rather than fail
/// on a NaN. The engine does not reach it, since its states have a norm of
/// 1 to rounding (a gate may drift from it within the unitarity check's
/// tolerance, `tensornet::checks::UNITARITY_TOLERANCE`).
#[test]
fn the_zero_spectrum_keeps_one_zero_value() {
    let truncation = Truncation::of(&[0.0, 0.0, 0.0], 0.5, usize::MAX);
    assert_eq!(truncation.normalised_values, [0.0]);
    assert_eq!(truncation.retained_fraction, 1.0);
}

/// A split keeps at least one value, so no values, or a cap of 0, is the
/// caller's bug: each panics, saying which.
#[test]
fn no_values_or_a_cap_of_zero_panics() {
    let rows: [(&str, fn(), &str); 2] = [
        (
            "no values",
            || {
                let _ = Truncation::of(&[], 0.0, 1);
            },
            "singular values",
        ),
        (
            "a cap of 0",
            || {
                let _ = Truncation::of(&[1.0], 0.0, 0);
            },
            "at least one value",
        ),
    ];
    for (name, call, said) in rows {
        let message = panic_message(call);
        assert!(message.contains(said), "{name}: {message}");
    }
}
