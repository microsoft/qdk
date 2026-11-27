// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The gates and the complex constructor that the tests of several modules
//! share. Each module's tests sit beside it, in its `tests.rs`.

use num_complex::Complex64;

/// The complex number `re + i·im`.
pub(crate) fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// The Hadamard gate.
pub(crate) fn hadamard() -> [Complex64; 4] {
    [c(H, 0.0), c(H, 0.0), c(H, 0.0), c(-H, 0.0)]
}

/// CX with its first site as the control.
pub(crate) fn cx() -> [Complex64; 16] {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    [l, o, o, o, o, l, o, o, o, o, o, l, o, o, l, o]
}
