// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The matrices of the gates the circuits use, row-major with the first
//! qubit the most significant.

use num_complex::Complex64;

use super::{c, rng::Rng};

pub const H: [Complex64; 4] = [
    Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    Complex64::new(-std::f64::consts::FRAC_1_SQRT_2, 0.0),
];

pub fn phase(theta: f64) -> [Complex64; 4] {
    [
        c(1.0, 0.0),
        c(0.0, 0.0),
        c(0.0, 0.0),
        Complex64::from_polar(1.0, theta),
    ]
}

pub fn rx(theta: f64) -> [Complex64; 4] {
    let (s, k) = (theta / 2.0).sin_cos();
    [c(k, 0.0), c(0.0, -s), c(0.0, -s), c(k, 0.0)]
}

pub fn ry(theta: f64) -> [Complex64; 4] {
    let (s, k) = (theta / 2.0).sin_cos();
    [c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)]
}

pub fn rz(theta: f64) -> [Complex64; 4] {
    [
        Complex64::from_polar(1.0, -theta / 2.0),
        c(0.0, 0.0),
        c(0.0, 0.0),
        Complex64::from_polar(1.0, theta / 2.0),
    ]
}

/// `Rz(φ)·Ry(θ)·Rz(λ)`.
pub fn u3(phi: f64, theta: f64, lambda: f64) -> [Complex64; 4] {
    let (s, k) = (theta / 2.0).sin_cos();
    let ph = |a: f64| Complex64::from_polar(1.0, a / 2.0);
    [
        k * ph(-phi - lambda),
        -s * ph(-phi + lambda),
        s * ph(phi - lambda),
        k * ph(phi + lambda),
    ]
}

/// `|0⟩⟨0| ⊗ 1 + |1⟩⟨1| ⊗ u`, the control first.
pub fn controlled(u: [Complex64; 4]) -> [Complex64; 16] {
    let mut m = [c(0.0, 0.0); 16];
    m[0] = c(1.0, 0.0);
    m[5] = c(1.0, 0.0);
    m[10] = u[0];
    m[11] = u[1];
    m[14] = u[2];
    m[15] = u[3];
    m
}

pub fn pauli_x() -> [Complex64; 4] {
    [c(0.0, 0.0), c(1.0, 0.0), c(1.0, 0.0), c(0.0, 0.0)]
}

pub fn pauli_y() -> [Complex64; 4] {
    [c(0.0, 0.0), c(0.0, -1.0), c(0.0, 1.0), c(0.0, 0.0)]
}

pub fn pauli_z() -> [Complex64; 4] {
    [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(-1.0, 0.0)]
}

/// `exp(−iθ/2 · Z⊗Z)`.
pub fn rzz(theta: f64) -> [Complex64; 16] {
    let mut m = [c(0.0, 0.0); 16];
    let (a, b) = (
        Complex64::from_polar(1.0, -theta / 2.0),
        Complex64::from_polar(1.0, theta / 2.0),
    );
    m[0] = a;
    m[5] = b;
    m[10] = b;
    m[15] = a;
    m
}

/// A Haar-like random unitary of dimension `d`, row-major: Gram–Schmidt
/// of a complex Gaussian matrix.
pub fn random_unitary<const N: usize>(rng: &mut Rng, d: usize) -> [Complex64; N] {
    let mut cols: Vec<Vec<Complex64>> = (0..d)
        .map(|_| (0..d).map(|_| c(rng.normal(), rng.normal())).collect())
        .collect();
    for k in 0..d {
        for j in 0..k {
            let dot: Complex64 = (0..d).map(|i| cols[j][i].conj() * cols[k][i]).sum();
            let (done, rest) = cols.split_at_mut(k);
            for (x, y) in rest[0].iter_mut().zip(&done[j]) {
                *x -= dot * y;
            }
        }
        let norm = cols[k].iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
        for x in &mut cols[k] {
            *x /= norm;
        }
    }
    std::array::from_fn(|at| cols[at % d][at / d])
}
