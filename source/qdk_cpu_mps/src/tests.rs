// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's unit tests, one file per module (a module's tests split by
//! behaviour in a folder of the same name), with the gates and observables
//! they share.

mod chain;
mod engine;
mod linear_algebra;

use num_complex::Complex64;
use tensornet::{MpsShape, Pauli, PauliSum, PauliTerm};

fn c(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![c(k), c(-s), c(s), c(k)]
}

fn phase(theta: f64) -> Vec<Complex64> {
    vec![c(1.0), c(0.0), c(0.0), Complex64::from_polar(1.0, theta)]
}

fn cnot() -> Vec<Complex64> {
    let mut m = vec![c(0.0); 16];
    for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
        m[4 * row + col] = c(1.0);
    }
    m
}

fn cz() -> Vec<Complex64> {
    let mut m = vec![c(0.0); 16];
    for (at, value) in [1.0, 1.0, 1.0, -1.0].into_iter().enumerate() {
        m[5 * at] = c(value);
    }
    m
}

fn swap() -> Vec<Complex64> {
    let mut m = vec![c(0.0); 16];
    for (row, col) in [(0, 0), (1, 2), (2, 1), (3, 3)] {
        m[4 * row + col] = c(1.0);
    }
    m
}

/// A complex entangling gate, symmetric under the exchange of its operands
/// and equal to its transpose: a partial iSWAP on |01⟩, |10⟩ and a phase on
/// |11⟩.
fn entangler(theta: f64) -> Vec<Complex64> {
    let mut m = vec![c(0.0); 16];
    let (s, k) = (theta / 2.0).sin_cos();
    m[0] = c(1.0);
    m[4 + 1] = c(k);
    m[4 + 2] = Complex64::new(0.0, -s);
    m[8 + 1] = Complex64::new(0.0, -s);
    m[8 + 2] = c(k);
    m[15] = Complex64::from_polar(1.0, theta);
    m
}

/// A chain of `sites` qubits whose bonds are at most `chi`.
fn chain(sites: usize, chi: usize) -> MpsShape {
    MpsShape::qubit_chain(sites, chi).expect("a chain of qubits")
}

/// A chain of `sites` qubits that holds every state: for the exact
/// reference.
fn exact(sites: usize) -> MpsShape {
    chain(sites, 1 << sites)
}

fn z_string(sites: &[usize]) -> PauliSum {
    let factors = sites.iter().map(|&s| (s, Pauli::Z)).collect();
    PauliSum::new(
        c(0.0),
        vec![PauliTerm::new(c(1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

fn mixed_string() -> PauliSum {
    let factors = vec![(0, Pauli::X), (3, Pauli::Y), (5, Pauli::Z)];
    PauliSum::new(
        c(0.25),
        vec![PauliTerm::new(Complex64::new(0.5, -1.0), factors).expect("a term")],
    )
    .expect("an observable")
}
