// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn non_finite_falls_back_to_decimal() {
    assert_eq!(format_angle(f64::NAN), "NaN");
    assert_eq!(format_angle(f64::INFINITY), "inf");
    assert_eq!(format_angle(f64::NEG_INFINITY), "-inf");
}

#[test]
fn zero_and_near_zero() {
    assert_eq!(format_angle(0.0), "0");
    assert_eq!(format_angle(-0.0), "0");
    assert_eq!(format_angle(1e-12), "0");
    assert_eq!(format_angle(-1e-12), "0");
}

#[test]
fn magnitudes_between_eps_and_pi_eps_do_not_become_a_zero_multiple() {
    // Without the lower bound on the whole-multiple rule these round to a
    // multiple of zero and would emit "0 * π".
    assert_eq!(format_angle(2e-9), "0.0000");
    assert_eq!(format_angle(3e-9), "0.0000");
    assert_eq!(format_angle(-2e-9), "-0.0000");
}

#[test]
fn whole_multiples_of_pi() {
    assert_eq!(format_angle(PI), "π");
    assert_eq!(format_angle(-PI), "-π");
    assert_eq!(format_angle(2.0 * PI), "2 * π");
    assert_eq!(format_angle(3.0 * PI), "3 * π");
    assert_eq!(format_angle(-3.0 * PI), "-3 * π");
}

#[test]
fn whole_multiples_are_not_capped_by_the_fraction_ceiling() {
    assert_eq!(format_angle(20.0 * PI), "20 * π");
}

#[test]
fn simple_fractions_of_pi() {
    assert_eq!(format_angle(PI / 2.0), "π / 2");
    assert_eq!(format_angle(PI / 3.0), "π / 3");
    assert_eq!(format_angle(PI / 4.0), "π / 4");
    assert_eq!(format_angle(PI / 8.0), "π / 8");
    assert_eq!(format_angle(PI / 99.0), "π / 99");
}

#[test]
fn negative_values_are_signed_for_every_symbolic_rule() {
    assert_eq!(format_angle(-PI / 4.0), "-π / 4");
    assert_eq!(format_angle(-2.0 * PI / 3.0), "-2 * π / 3");
    assert_eq!(format_angle(-PI / 6.0), "-π / 6");
}

#[test]
fn reduced_fractions_of_pi() {
    assert_eq!(format_angle(2.0 * PI / 3.0), "2 * π / 3");
    assert_eq!(format_angle(3.0 * PI / 2.0), "3 * π / 2");
    assert_eq!(format_angle(7.0 * PI / 8.0), "7 * π / 8");
    assert_eq!(format_angle(15.0 * PI / 16.0), "15 * π / 16");
}

#[test]
fn fractions_are_reported_in_lowest_terms() {
    // 2/4 and 8/16 both reduce to 1/2, which the simple-fraction rule matches.
    assert_eq!(format_angle(2.0 * PI / 4.0), "π / 2");
    assert_eq!(format_angle(8.0 * PI / 16.0), "π / 2");
    // 6/8 reduces to 3/4.
    assert_eq!(format_angle(6.0 * PI / 8.0), "3 * π / 4");
}

#[test]
fn denominators_beyond_the_simple_fraction_ceiling_fall_back() {
    assert_eq!(format_angle(PI / 129.0), "0.0244");
}

#[test]
fn magnitudes_beyond_the_fraction_ceiling_fall_back() {
    // Not a whole multiple, and larger than any recognized fraction.
    assert_eq!(format_angle(64.5 * PI), "202.6327");
}

#[test]
fn values_that_are_not_related_to_pi_fall_back() {
    assert_eq!(format_angle(0.5), "0.5000");
    assert_eq!(format_angle(1.0), "1.0000");
    assert_eq!(format_angle(1.2345), "1.2345");
    assert_eq!(format_angle(-0.3), "-0.3000");
}

#[test]
fn tolerance_boundary() {
    // Inside the tolerance, still recognized.
    assert_eq!(format_angle(PI / 4.0 + 1e-11), "π / 4");
    // Outside the tolerance, reported as the decimal it actually is.
    assert_eq!(format_angle(PI / 4.0 + 1e-6), "0.7854");
}

#[test]
fn a_narrow_openqasm_angle_width_falls_back_rather_than_claiming_a_fraction() {
    // An `angle[8]` holding pi/3 round-trips to this value, which is off by
    // about 8e-3 and is genuinely not pi/3.
    let narrow = 1.055_378_782_065_321;
    assert_eq!(format_angle(narrow), "1.0554");
}

#[test]
fn an_unsized_openqasm_angle_round_trip_is_still_recognized() {
    // The 53-bit fixed-point grid reproduces these within rounding error.
    let tau = 2.0 * PI;
    let grid = tau / f64::from(1u32 << 31) / f64::from(1u32 << 22);
    let quarter_turn = (PI / 4.0 / grid).round() * grid;
    assert_eq!(format_angle(quarter_turn), "π / 4");
    let third = (PI / 3.0 / grid).round() * grid;
    assert_eq!(format_angle(third), "π / 3");
}

#[test]
fn wrapped_negative_angle_reports_its_wrapped_value() {
    // OpenQASM wraps -pi/4 into [0, tau) before the circuit sees it.
    let wrapped = 2.0 * PI - PI / 4.0;
    assert_eq!(format_angle(wrapped), "7 * π / 4");
}

#[test]
fn no_emitted_form_uses_implicit_multiplication() {
    // The angle expression parser emits no operator between a number and a
    // following π, so a digit directly followed by π would be rejected.
    let samples = [
        0.0,
        PI,
        -PI,
        2.0 * PI,
        PI / 2.0,
        -PI / 4.0,
        2.0 * PI / 3.0,
        -15.0 * PI / 16.0,
        0.5,
    ];
    for value in samples {
        let formatted = format_angle(value);
        let chars: Vec<char> = formatted.chars().collect();
        for pair in chars.windows(2) {
            assert!(
                !(pair[0].is_ascii_digit() && pair[1] == 'π'),
                "implicit multiplication in {formatted}"
            );
        }
    }
}

#[test]
fn emitted_forms_contain_no_parentheses() {
    // Every symbolic form is a flat expression; the reciprocal form that would
    // have needed grouping is deliberately not recognized.
    for value in [PI, PI / 4.0, 2.0 * PI / 3.0, 3.0 * PI / 2.0] {
        assert!(!format_angle(value).contains(['(', ')']));
    }
}
