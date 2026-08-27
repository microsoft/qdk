// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::f64::consts::PI;

/// Tolerance in radians for matching an angle against a candidate fraction of π.
const RADIAN_EPS: f64 = 1e-9;

/// Tolerance on the dimensionless ratio `angle / π` used by the whole-multiple rule.
/// Numerically equal to `RADIAN_EPS`, but it bounds a ratio rather than an angle, so
/// that rule's effective angular tolerance is this value times π.
const RATIO_EPS: f64 = 1e-9;

/// Largest numerator and denominator considered for a reduced fraction of π.
const MAX_FRAC: u32 = 64;

/// Largest denominator considered for a simple `π / d` fraction.
const MAX_PI_FRAC: u32 = 128;

/// Formats a gate angle for display, preferring a symbolic multiple or fraction
/// of π and otherwise falling back to four decimal places.
///
/// Emitted symbolic forms always use explicit operators (`2 * π / 3`, never
/// `2π/3`) so they remain valid input to the circuit editor's angle expression
/// parser and to circuit-to-Q# generation.
pub(crate) fn format_angle(value: f64) -> String {
    // NaN and the infinities have no symbolic form, and every test below would
    // silently misbehave on them.
    if !value.is_finite() {
        return format_decimal(value);
    }

    let magnitude = value.abs();
    // Collapse anything within tolerance of zero to a bare `0`, before `sign`
    // below can turn a tiny negative into a signed zero.
    if magnitude < RADIAN_EPS {
        return "0".to_string();
    }

    let sign = if value < 0.0 { "-" } else { "" };

    // Whole multiples of π. The first test rejects a magnitude below π: its ratio
    // rounds to zero, which would pass the second test and print "0 * π". The
    // second test is the actual rule, that the ratio is an integer within tolerance.
    let multiple = magnitude / PI;
    if multiple >= 1.0 - RATIO_EPS && (multiple - multiple.round()).abs() < RATIO_EPS {
        let count = multiple.round();
        // A single turn reads better unqualified than as "1 * π".
        return if (count - 1.0).abs() < RATIO_EPS {
            format!("{sign}π")
        } else {
            format!("{sign}{count:.0} * π")
        };
    }

    // The largest fraction the rules below can produce is 15π/2, so this bound is
    // conservative; past it only a decimal is possible.
    if magnitude >= f64::from(MAX_FRAC) * PI {
        return format_decimal(value);
    }

    // Simple fractions `π / d`, which allow a larger denominator than the
    // general reduced fraction below. The three tests are, in order: reject a
    // magnitude past 2π, whose reciprocal rounds to zero and must not be divided
    // by; apply the readability ceiling; and confirm the rounded denominator
    // really does reproduce the value, since rounding alone only gets close.
    let denominator = (PI / magnitude).round();
    if denominator >= 1.0
        && denominator <= f64::from(MAX_PI_FRAC)
        && (magnitude - PI / denominator).abs() < RADIAN_EPS
    {
        return format!("{sign}π / {denominator:.0}");
    }

    // Reduced fractions `n * π / d`. Numerator 1 and denominator 1 are already
    // covered above.
    for d in 2..=MAX_FRAC {
        // Fractions at this denominator are spaced far wider than the tolerance,
        // so only the nearest numerator can match.
        let candidate = (magnitude * f64::from(d) / PI).round();
        // Below two is a unit fraction, handled above; above the ceiling is out of
        // range. This also drops the zero candidate a small angle produces.
        if !(2.0..=f64::from(MAX_FRAC)).contains(&candidate) {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded above
        let n = candidate as u32;
        // Keep the printed fraction in lowest terms.
        if gcd(n, d) != 1 {
            continue;
        }
        // Rounding only found the closest candidate; confirm it is the value.
        if (magnitude - f64::from(n) * PI / f64::from(d)).abs() < RADIAN_EPS {
            return format!("{sign}{n} * π / {d}");
        }
    }

    format_decimal(value)
}

fn format_decimal(value: f64) -> String {
    format!("{value:.4}")
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
