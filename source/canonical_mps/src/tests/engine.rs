// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the engine's rules, one file per area of the contract or
//! of the engine's own promises, on [`FakeAlgebra`]. This file holds what
//! several areas share: the engine with and without fusion, gates and
//! observables, and a circuit run on the engine and on the exact
//! reference. Two stand-ins wrap [`FakeAlgebra`] for the tests that need
//! more than its answers: the spy [`SpyAlgebra`] counts the engine's calls
//! and records its uploads, and the fake [`FailingFakeAlgebra`] fails the
//! call a test chooses.

mod chain;
mod conformance;
mod expectation;
mod failing_fake_algebra;
mod failures;
mod fusion;
mod reads;
mod refusals;
mod retained_weight;
mod routing;
mod sampling;
mod spy_algebra;
mod statistics;
mod truncation;

use crate::algebra::fake_algebra::FakeAlgebra;
use crate::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, DEFAULT_CUTOFF, SplitStatistics,
};
use failing_fake_algebra::FailingFakeAlgebra;
use num_complex::Complex64;
use spy_algebra::SpyAlgebra;
use tensornet::{
    MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, PauliTerm, Refusal,
    reference::{ExactReference, ReferenceState},
};

/// The engine the tests run, on the test algebra.
type Engine = CanonicalMpsEngine<FakeAlgebra>;

/// A state of [`Engine`].
type State = CanonicalMpsState<FakeAlgebra>;

fn real(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![real(k), real(-s), real(s), real(k)]
}

fn phase(theta: f64) -> Vec<Complex64> {
    vec![
        real(1.0),
        real(0.0),
        real(0.0),
        Complex64::from_polar(1.0, theta),
    ]
}

fn cnot() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
        m[4 * row + col] = real(1.0);
    }
    m
}

fn cz() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (at, value) in [1.0, 1.0, 1.0, -1.0].into_iter().enumerate() {
        m[5 * at] = real(value);
    }
    m
}

/// The Z⊗Z gate, which commutes with X on every site.
fn zz() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (at, value) in [1.0, -1.0, -1.0, 1.0].into_iter().enumerate() {
        m[5 * at] = real(value);
    }
    m
}

/// A complex entangling gate, symmetric under the exchange of its operands
/// and equal to its transpose: a partial iSWAP on |01⟩, |10⟩ and a phase on
/// |11⟩.
fn entangler(theta: f64) -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    let (s, k) = (theta / 2.0).sin_cos();
    m[0] = real(1.0);
    m[4 + 1] = real(k);
    m[4 + 2] = Complex64::new(0.0, -s);
    m[8 + 1] = Complex64::new(0.0, -s);
    m[8 + 2] = real(k);
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
        real(0.0),
        vec![PauliTerm::new(real(1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

fn mixed_string() -> PauliSum {
    let factors = vec![(0, Pauli::X), (3, Pauli::Y), (5, Pauli::Z)];
    PauliSum::new(
        real(0.25),
        vec![PauliTerm::new(Complex64::new(0.5, -1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

/// The engine with and without fusion, each with its setting's name.
fn both_fusions(cutoff: f64) -> [(Engine, &'static str); 2] {
    let engine = Engine::new(FakeAlgebra).with_cutoff(cutoff);
    [
        (engine.clone(), "fused"),
        (engine.with_fusion(false), "unfused"),
    ]
}

/// The engines of both fusion settings at cutoffs 0 and the default.
fn both_fusions_and_cutoffs() -> impl Iterator<Item = (Engine, String)> {
    [0.0, DEFAULT_CUTOFF].into_iter().flat_map(|cutoff| {
        both_fusions(cutoff).map(|(engine, name)| (engine, format!("{name}, cutoff {cutoff:e}")))
    })
}

/// Applies the same circuit, with gates on sites far apart and in both
/// operand orders, to `engine` and to the reference, and returns both.
fn circuit(engine: &Engine, sites: usize) -> (State, ReferenceState) {
    let reference = ExactReference::new();
    let mut state = engine.prepare(&chain(sites, 64)).expect("prepare");
    let mut exact_state = reference.prepare(&exact(sites)).expect("prepare");
    let mut both = |on: &[usize], m: &[Complex64]| {
        engine.apply(&mut state, on, m).expect("apply");
        reference.apply(&mut exact_state, on, m).expect("apply");
    };
    for site in 0..sites {
        let x = f64::from(u32::try_from(site).expect("a small site"));
        both(&[site], &ry(0.3 + 0.2 * x));
        both(&[site], &phase(0.1 * x));
    }
    both(&[0, sites - 1], &cnot());
    both(&[sites - 2, 1], &entangler(0.9));
    both(&[2, 3], &entangler(-0.4));
    both(&[4, 2], &cnot());
    both(&[1], &ry(1.3));
    both(&[sites - 1, 0], &entangler(0.6));
    (state, exact_state)
}

/// Each site's P(One) on `state` and on the reference's `exact_state`
/// agree within `tolerance`.
fn assert_reads_agree(
    engine: &Engine,
    state: &State,
    exact_state: &ReferenceState,
    tolerance: f64,
) {
    let reference = ExactReference::new();
    for site in 0..exact_state.site_count() {
        let p = engine.probability_of_one(state, site).expect("P(One)");
        let q = reference
            .probability_of_one(exact_state, site)
            .expect("P(One)");
        assert!((p - q).abs() < tolerance, "site {site}: {p} vs {q}");
    }
}

/// One round of a Bell pair on sites 0 and 1 truncated to a bond of 1: a
/// turn by π/2, a read of `centre` (0 or 1), which moves the centre there,
/// and a CNOT, whose split keeps half the weight and leaves the centre on
/// that side, forced by a read of the retained weight, which it returns.
fn halve_the_weight(
    engine: &Engine,
    state: &mut State,
    centre: usize,
) -> Result<f64, CanonicalMpsError<std::convert::Infallible>> {
    engine.apply(state, &[0], &ry(std::f64::consts::FRAC_PI_2))?;
    engine.probability_of_one(state, centre)?;
    engine.apply(state, &[0, 1], &cnot())?;
    engine.retained_weight(state)
}
