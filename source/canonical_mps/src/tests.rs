// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's tests, one module per module with run-time behaviour.

mod dense_linalg;
mod engine;
mod site_algebra;

use num_complex::Complex64;

fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}
