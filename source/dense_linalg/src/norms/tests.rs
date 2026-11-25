// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the norms (`norms`): the square root of the squared
//! moduli summed from the first element to the last, and the norm divided
//! by a divisor, in range at any scale.

use crate::test_utils::c;
use crate::{norm, norm_divided_by, squared_norm};
use num_complex::Complex64;

/// `[1, h, h, h, h]` with `|h|² = 2⁻⁵³`, half of 1's ulp: summed from the
/// first to the last, each `2⁻⁵³` rounds away against the 1 (a tie, to
/// even), and the sum is 1. From the last to the first, the four make
/// `2⁻⁵¹`, which 1 keeps, and so does the square root: `1 + 2⁻⁵²`.
fn one_then_four_halves_of_an_ulp() -> Vec<Complex64> {
    let h = 2_f64.powi(-27);
    let mut x = vec![c(h, h); 5];
    x[0] = c(1.0, 0.0);
    x
}

/// The squared norm sums the elements' squared moduli from the first to
/// the last, and the norm is its square root. One row per scene, with the
/// squared norm expected: no elements; `[3, 4i]`, of norm 5; and
/// [`one_then_four_halves_of_an_ulp`], whose sum is 1 in that order only.
#[test]
fn the_norm_sums_the_squared_moduli_from_the_first_element_to_the_last() {
    let rows: [(&str, Vec<Complex64>, f64); 3] = [
        ("no elements", Vec::new(), 0.0),
        ("[3, 4i]", vec![c(3.0, 0.0), c(0.0, 4.0)], 25.0),
        ("[1, h, h, h, h]", one_then_four_halves_of_an_ulp(), 1.0),
    ];
    for (scene, x, expected) in rows {
        assert_eq!(squared_norm(&x), expected, "{scene}: the squared norm");
        assert_eq!(norm(&x), expected.sqrt(), "{scene}: the norm");
    }
}

/// The norm divided by a divisor about the largest modulus is exact where
/// the squares of the elements themselves would overflow or vanish. One
/// row per scene, each `‖x‖ / divisor` exact:
///
/// - `[3, 4i]` divided by 2: 2.5;
/// - `[3, 4i]·2¹⁰⁰⁰` divided by `2¹⁰⁰⁰`: 5, where the squares overflow;
/// - `[3, 4i]·2⁻¹⁰⁶⁰`, subnormal, divided by `2⁻¹⁰⁶⁰`: 5, where the
///   squares vanish;
/// - [`one_then_four_halves_of_an_ulp`] divided by 1: 1, summed from the
///   first element to the last.
#[test]
fn the_norm_divided_by_stays_in_range_at_any_scale() {
    let three_four = |scale: f64| vec![c(3.0 * scale, 0.0), c(0.0, 4.0 * scale)];
    // `powi(-1060)` would round its `2¹⁰⁶⁰` to infinity on the way.
    let (large, small) = (2_f64.powi(1000), 2_f64.powi(-530) * 2_f64.powi(-530));
    let rows: [(&str, Vec<Complex64>, f64, f64); 4] = [
        ("[3, 4i] / 2", three_four(1.0), 2.0, 2.5),
        ("squares that overflow", three_four(large), large, 5.0),
        ("squares that vanish", three_four(small), small, 5.0),
        (
            "[1, h, h, h, h]",
            one_then_four_halves_of_an_ulp(),
            1.0,
            1.0,
        ),
    ];
    for (scene, x, divisor, expected) in rows {
        assert_eq!(norm_divided_by(&x, divisor), expected, "{scene}");
    }
}
