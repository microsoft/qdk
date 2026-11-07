// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The gates the cases apply: their row-major matrices, and the
//! products that build gates of more sites.

use crate::Pauli;
use num_complex::Complex64;

pub(super) fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

pub(super) fn hadamard() -> Vec<Complex64> {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    vec![c(h, 0.0), c(h, 0.0), c(h, 0.0), c(-h, 0.0)]
}

pub(super) fn pauli_x() -> Vec<Complex64> {
    Pauli::X.matrix().to_vec()
}

/// `Ry(θ)`, real and not symmetric.
pub(crate) fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)]
}

/// `S·H`, complex and not symmetric.
pub(crate) fn s_h() -> Vec<Complex64> {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    vec![c(h, 0.0), c(h, 0.0), c(0.0, h), c(0.0, -h)]
}

/// CNOT with its first site as the control.
pub(crate) fn cnot() -> Vec<Complex64> {
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    vec![l, o, o, o, o, l, o, o, o, o, o, l, o, o, l, o]
}

/// Controlled-Y with its first site as the control: complex, and neither
/// symmetric nor symmetric under swapping its sites.
pub(crate) fn controlled_y() -> Vec<Complex64> {
    let (o, l, i) = (c(0.0, 0.0), c(1.0, 0.0), c(0.0, 1.0));
    vec![l, o, o, o, o, l, o, o, o, o, o, -i, o, o, i, o]
}

/// The product `a·b` of two row-major `dim`×`dim` matrices.
pub(crate) fn product(a: &[Complex64], b: &[Complex64], dim: usize) -> Vec<Complex64> {
    (0..dim * dim)
        .map(|at| {
            let (row, col) = (at / dim, at % dim);
            (0..dim).map(|k| a[row * dim + k] * b[k * dim + col]).sum()
        })
        .collect()
}

/// The side of a square matrix of `len` entries.
fn side(len: usize) -> usize {
    (1..=len).find(|d| d * d >= len).unwrap_or(1)
}

/// `a ⊗ b` of two square matrices, `a` on the first (most significant)
/// sites.
pub(crate) fn kron(a: &[Complex64], b: &[Complex64]) -> Vec<Complex64> {
    let (da, db) = (side(a.len()), side(b.len()));
    let dim = da * db;
    (0..dim * dim)
        .map(|at| {
            let (row, col) = (at / dim, at % dim);
            a[(row / db) * da + col / db] * b[(row % db) * db + col % db]
        })
        .collect()
}

/// The Toffoli gate, its first two sites the controls.
pub(super) fn toffoli() -> Vec<Complex64> {
    (0..64)
        .map(|at| {
            let (row, col) = (at / 8, at % 8);
            let target = if row >= 6 { row ^ 1 } else { row };
            c(if col == target { 1.0 } else { 0.0 }, 0.0)
        })
        .collect()
}

/// A two-site unitary built as the product of many gates, as fusion builds
/// one: unitary to rounding only.
pub(crate) fn fused_gates(count: usize) -> Vec<Complex64> {
    let layers = [
        cnot(),
        kron(&ry(0.3), &s_h()),
        controlled_y(),
        kron(&s_h(), &ry(-1.7)),
    ];
    let identity: Vec<Complex64> = (0..16)
        .map(|at| c(if at % 5 == 0 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    (0..count).fold(identity, |fused, k| {
        product(&layers[k % layers.len()], &fused, 4)
    })
}

/// `[[1, 2⁻ᵏ], [0, 1]]`, whose `M·M† − I` is `[[2⁻²ᵏ, 2⁻ᵏ], [2⁻ᵏ, 0]]`: for
/// `k` of 27 or more, `1 + 2⁻²ᵏ` rounds to 1, so its deviation is exactly
/// `2¹⁻ᵏ` in any order of summation. With `k = 30` it is 1.86e-9, beyond
/// [`UNITARITY_TOLERANCE`](crate::checks::UNITARITY_TOLERANCE); with
/// `k = 32`, 4.66e-10, within it.
pub(super) fn shear(k: i32) -> Vec<Complex64> {
    vec![c(1.0, 0.0), c(2f64.powi(-k), 0.0), c(0.0, 0.0), c(1.0, 0.0)]
}
