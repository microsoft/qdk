// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::from_prob;

#[test]
fn test_1_as_q1_63() {
    let uq1_63 = from_prob(1.0);
    assert_eq!(uq1_63, 0x8000_0000_0000_0000);
}

#[test]
fn test_05_as_q1_63() {
    let uq1_63 = from_prob(0.5);
    assert_eq!(uq1_63, 0x4000_0000_0000_0000);
}

#[test]
fn test_tiny_float() {
    // approx 8.8817842E-16
    #[allow(clippy::cast_precision_loss)]
    let num: f64 = 1.0 / (1u64 << 50) as f64;
    let uq1_63 = from_prob(num);
    assert_eq!(uq1_63, 0x0000_0000_0000_2000);
}

#[test]
fn test_tiniest_float() {
    // approx 1.0842E-19
    #[allow(clippy::cast_precision_loss)]
    let num: f64 = 1.0 / (1u64 << 63) as f64;
    let uq1_63 = from_prob(num);
    assert_eq!(uq1_63, 0x0000_0000_0000_0001);
}

#[test]
fn float_with_significant_bits() {
    // approx 1.5521806e-10
    let num: f32 = f32::from_bits(0x2f2a_aa00);
    // signficand (with implicit 1) becomes 1010_1010_1010_1010
    // Shifted right 33 bits will become 0x...5555...
    let uq1_63 = from_prob(f64::from(num));
    assert_eq!(uq1_63, 0x0000_0000_5555_0000);
}
