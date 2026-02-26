// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod noiseless_tests;
pub mod noisy_tests;

use crate::{TOLERANCE, eq_with_tolerance};

/// Assert that two f64 are equal up to a `TOLERANCE`.
pub fn assert_approx_eq(left: f64, right: f64) {
    assert_approx_eq_with_tolerance(left, right, TOLERANCE);
}

pub fn assert_approx_eq_with_tolerance(left: f64, right: f64, tolerance: f64) {
    assert!(
        eq_with_tolerance(left, right, tolerance),
        "aprox_equal failed, left = {left}, right = {right}"
    );
}
