// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CPU algebra's rule of truncation, met through
//! [`CpuAlgebra::gate_split`]: the tail below the cutoff, a fraction of
//! the split's own weight, then what exceeds the cap; never the largest
//! value; at a cutoff of 0, the zeros and the values of rounding's size
//! against the largest; and what is kept, normalised into the centre's
//! site, with its share of the weight.
//!
//! Each split is of a pair whose gated pair `θ` is diagonal, so its
//! singular values are the ones a row names.

use super::c;
use crate::CpuAlgebra;
use crate::site_algebra::{CentreSide, GateSplitCall, GateSplitOutcome, SiteAlgebra};
use num_complex::Complex64;

/// The singular values whose squares are `weights`.
fn values_of(weights: &[f64]) -> Vec<f64> {
    weights.iter().map(|w| w.sqrt()).collect()
}

/// The CPU algebra's split, at `cutoff` and the cap `max_extent`, of a pair
/// whose gated pair is diagonal with the nonincreasing `values`: the left
/// site the identity's first `k` columns, the right site the values on its
/// diagonal, the gate the identity, and the centre on the left.
fn split_of(values: &[f64], cutoff: f64, max_extent: usize) -> GateSplitOutcome<Vec<Complex64>> {
    let k = values.len();
    let extent = k.div_ceil(2);
    let rows = 2 * extent;
    let left: Vec<Complex64> = (0..rows * k)
        .map(|at| c(if at % rows == at / rows { 1.0 } else { 0.0 }, 0.0))
        .collect();
    let on_diagonal = |at: usize| at % k == at / k;
    let right: Vec<Complex64> = (0..k * rows)
        .map(|at| c(if on_diagonal(at) { values[at % k] } else { 0.0 }, 0.0))
        .collect();
    let identity = std::array::from_fn(|i| c(if i % 5 == 0 { 1.0 } else { 0.0 }, 0.0));
    let call = GateSplitCall {
        left: &left,
        right: &right,
        left_extent: extent,
        bond: k,
        right_extent: extent,
        gate: identity,
        max_extent,
        cutoff,
        centre: CentreSide::Left,
    };
    CpuAlgebra
        .gate_split(&call)
        .expect("the SVD of a diagonal matrix")
}

/// The kept values, normalised, which the split multiplied into the left
/// site's columns: each column's norm, as the SVD's columns are unit.
fn kept_values(outcome: &GateSplitOutcome<Vec<Complex64>>) -> Vec<f64> {
    let rows = outcome.left.len() / outcome.bond;
    outcome
        .left
        .chunks_exact(rows)
        .map(|column| column.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt())
        .collect()
}

/// The cutoff drops the longest tail whose squares sum to at most its
/// share of the split's weight, but never the largest value; at a cutoff
/// of 0, it drops only the zeros and the values of rounding's size against
/// the largest, which the contract allows it to drop. Each row: the values,
/// the cutoff, and the bond kept.
#[test]
fn the_cutoff_discards_the_tail_within_its_share() {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let rows: [(&str, Vec<f64>, f64, usize); 6] = [
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
        // value's square is exactly the cutoff's share of the sum.
        // COMPATIBILITY NOTE: this behavior was present in the earlier
        // CPU MPS engine's truncation (a tail is dropped while its squares
        // sum to at most the cutoff's share), hence we keep it: it is what
        // makes a cutoff of 0 drop the zeros.
        ("a tail of exactly the cutoff's share", vec![h, h], 0.5, 1),
        // A Schmidt weight of 1e-20, far below the cutoffs simulations
        // use, is kept.
        ("zeros at a cutoff of 0", vec![1.0, 1e-10, 0.0, 0.0], 0.0, 2),
        // The SVD leaves out a value below rounding's size against the
        // largest (`Svd::of`'s trim), which the cutoff's doc allows.
        (
            "a value of rounding's size at a cutoff of 0",
            vec![1.0, 1e-17],
            0.0,
            1,
        ),
    ];
    for (name, values, cutoff, bond) in rows {
        assert_eq!(split_of(&values, cutoff, usize::MAX).bond, bond, "{name}");
    }
}

/// The Schmidt weights 0.81, 0.09, 0.09 and 0.01 (two pairs, each 0.9 and
/// 0.1, cut together) at a cutoff of 0.095 and a cap of 2: the cutoff
/// keeps three, then the cap two, a retained fraction of 0.9; the cap
/// applied first would leave the cutoff to drop a 0.09, a bond of 1 and a
/// retained fraction of 0.81.
#[test]
fn the_capacity_caps_the_bond_the_cutoff_keeps() {
    let values = values_of(&[0.81, 0.09, 0.09, 0.01]);
    let split = split_of(&values, 0.095, 2);
    assert_eq!(split.bond, 2);
    let fraction = split.retained_fraction;
    assert!((fraction - 0.9).abs() < 1e-15, "{fraction}");
}

/// The retained fraction is the share of the squares the split kept, and
/// exactly 1 when it dropped only zeros, so that the state's retained
/// weight does not move by a rounding error at every such split. Each row:
/// the values, the cutoff, the cap, the bond kept (a precondition), the
/// expected fraction, and how far from it the fraction may round.
#[test]
fn the_retained_fraction_is_that_of_the_kept_squares() {
    type Row = (&'static str, Vec<f64>, f64, usize, usize, f64, f64);
    let small = 2f64.powi(-27);
    let rows: [Row; 6] = [
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
        (
            "a value of rounding's size dropped",
            vec![1.0, 1e-17],
            0.0,
            usize::MAX,
            1,
            1.0,
            0.0,
        ),
    ];
    for (name, values, cutoff, cap, bond, expected, tolerance) in rows {
        let split = split_of(&values, cutoff, cap);
        assert_eq!(split.bond, bond, "precondition: {name}");
        let fraction = split.retained_fraction;
        assert!(
            (fraction - expected).abs() <= tolerance,
            "{name}: {fraction} for {expected}"
        );
    }
}

/// Values 3, 2 and 1, all kept: the centre's site holds them divided by
/// their L2 norm, √14.
#[test]
fn the_kept_values_are_divided_by_their_norm() {
    let split = split_of(&[3.0, 2.0, 1.0], 0.0, usize::MAX);
    let norm = 14f64.sqrt();
    for (kept, value) in kept_values(&split).iter().zip([3.0, 2.0, 1.0]) {
        assert!((kept - value / norm).abs() < 1e-15, "{kept} for {value}");
    }
}

/// The values of a split, scaled by 2⁻¹⁰⁰⁰ (their squares, about 1e-603,
/// would underflow to zero): the same bond, retained fraction and sites
/// as at scale 1, to the last bit.
#[test]
fn tiny_values_truncate_as_at_scale_one() {
    let values = [0.9, 0.3, 0.1, 0.01];
    let scale = 2f64.powi(-1000);
    let tiny: Vec<f64> = values.iter().map(|v| v * scale).collect();
    let (tiny, at_one) = (
        split_of(&tiny, 0.011, usize::MAX),
        split_of(&values, 0.011, usize::MAX),
    );
    assert_eq!(tiny.bond, at_one.bond);
    assert_eq!(tiny.retained_fraction, at_one.retained_fraction);
    assert_eq!((tiny.left, tiny.right), (at_one.left, at_one.right));
}

/// A zero gated pair, which the contract rules out but a caller may still
/// send, and whose SVD gives one value, 0 (`Svd::of`): the split keeps it,
/// with a retained fraction of 1 and no NaN.
// DISCUSS COMPATIBILITY: the earlier CPU MPS engine's SVD gave the zero
// matrix one value, 0, as this crate's does; its truncation then divided
// 0 by 0, which would have failed the split with a weight underflow, where
// this one keeps the value with a retained fraction of 1. The engine does
// not reach it, since its states have a norm of 1 to rounding (a gate may
// drift from it within the unitarity check's tolerance). We think a total
// rule is better than failing on a NaN, and a retained fraction of 1
// agrees with "exactly 1 when every dropped value is zero", so we
// recommend keeping it.
#[test]
fn the_zero_spectrum_keeps_one_zero_value() {
    let split = split_of(&[0.0, 0.0, 0.0], 0.5, usize::MAX);
    assert_eq!(split.bond, 1);
    assert_eq!(kept_values(&split), [0.0]);
    assert_eq!(split.retained_fraction, 1.0);
}
