// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the engine's rules, one file per area of the contract or
//! of the engine's own promises, on the CPU algebra, [`CpuAlgebra`].
//! This file holds what several areas share: the engine with and without
//! fusion, gates and observables (with those of `engine::conformance`), and a
//! circuit run on the engine and on the exact reference. Two stand-ins
//! wrap [`CpuAlgebra`] for the tests that need more than its answers: the
//! spy [`SpyAlgebra`] counts the engine's calls and records its uploads,
//! and the fake [`FailingFakeAlgebra`] fails the call a test chooses.

mod centre_moves;
mod conformance;
mod expectation;
mod failures;
mod fusion;
mod measurement;
mod mpo;
mod refusals;
mod retained_weight;
mod sampling;
mod stand_ins;
mod state;
mod statistics;
mod truncation;

use crate::dense_linalg::SvdError;
use crate::engine::conformance::{
    assert_reads_agree, chain, cnot, entangler, exact, mixed_string, real, ry,
};
use crate::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, CanonicalMpsStatistics, CpuAlgebra,
    CpuMpsEngine as Engine, DEFAULT_CUTOFF,
};
use num_complex::Complex64;
use stand_ins::failing_fake_algebra::FailingFakeAlgebra;
use stand_ins::spy_algebra::{SpiedCalls, SpyAlgebra};
use std::rc::Rc;
use tensornet::{
    MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, PauliTerm, Refusal,
    reference::{ExactReference, ReferenceState},
};

/// A state of [`Engine`], the engine the tests run, on the CPU
/// algebra.
type State = CanonicalMpsState<CpuAlgebra>;

/// Gates in the order they are applied, each with the sites it acts on.
type Gates = Vec<(&'static [usize], Vec<Complex64>)>;

fn phase(theta: f64) -> Vec<Complex64> {
    vec![
        real(1.0),
        real(0.0),
        real(0.0),
        Complex64::from_polar(1.0, theta),
    ]
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

fn z_string(sites: &[usize]) -> PauliSum {
    let factors = sites.iter().map(|&s| (s, Pauli::Z)).collect();
    PauliSum::new(
        real(0.0),
        vec![PauliTerm::new(real(1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

/// The engine with and without fusion, each with its setting's name.
fn both_fusions(cutoff: f64) -> [(Engine, &'static str); 2] {
    let engine = Engine::new(CpuAlgebra).with_cutoff(cutoff);
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
/// operand orders, to `engine` and to the exact reference, and returns
/// both.
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

/// One round of a Bell pair on sites 0 and 1 truncated to a bond of 1: a
/// turn by π/2, a read of `centre` (0 or 1), which moves the centre there,
/// and a CNOT, whose split keeps half the weight and leaves the centre on
/// that side, forced by a read of the retained weight, which it returns.
fn halve_the_weight(
    engine: &Engine,
    state: &mut State,
    centre: usize,
) -> Result<f64, CanonicalMpsError<SvdError>> {
    engine.apply(state, &[0], &ry(std::f64::consts::FRAC_PI_2))?;
    engine.probability_of_one(state, centre)?;
    engine.apply(state, &[0, 1], &cnot())?;
    engine.retained_weight(state)
}

/// An engine on a spy algebra, a chain of `sites` sites prepared on it,
/// each turned and entangled with the next, and the spy's record of calls.
fn spied_entangled_chain(
    sites: usize,
) -> (
    CanonicalMpsEngine<SpyAlgebra>,
    CanonicalMpsState<SpyAlgebra>,
    Rc<SpiedCalls>,
) {
    let (algebra, calls) = SpyAlgebra::new();
    let engine = CanonicalMpsEngine::new(algebra);
    let mut state = engine.prepare(&chain(sites, 4)).expect("prepare");
    for site in 0..sites {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    for cut in 0..sites - 1 {
        engine
            .apply(&mut state, &[cut, cut + 1], &cnot())
            .expect("apply");
    }
    (engine, state, calls)
}
