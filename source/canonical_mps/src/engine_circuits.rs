// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Two circuits that run a [`CanonicalMpsEngine`] on a backend's algebra
//! and compare what it reads with `tensornet`'s exact reference. The
//! algebra's conformance kit (`algebra::conformance`) checks the backend's
//! calls one at a time; the circuits add hundreds of calls in a row, on
//! the backend's own memory and settings, so a backend's tests run them
//! on its device too:
//!
//! ```no_run
//! use canonical_mps::engine_circuits::{
//!     a_long_circuit_agrees_with_the_reference_without_truncation,
//!     a_truncated_circuit_stays_within_the_distance_its_splits_allow,
//! };
//! use canonical_mps::{CanonicalMpsEngine, CpuAlgebra};
//!
//! let engine = CanonicalMpsEngine::new(CpuAlgebra).with_cutoff(0.0);
//! a_long_circuit_agrees_with_the_reference_without_truncation(&engine);
//! a_truncated_circuit_stays_within_the_distance_its_splits_allow(&engine);
//! ```
//!
//! Each circuit asserts its rule, so a test calls it and nothing else. It
//! needs the `conformance` feature.

use crate::algebra::SiteAlgebra;
use crate::{CanonicalMpsEngine, CanonicalMpsState};
use num_complex::Complex64;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// A pseudo-random circuit on 8 sites, mostly on sites far apart, with
/// projections on the way, on `engine`: every read agrees with the
/// reference within `1e-10`.
///
/// `engine` must have a cutoff of 0 (`with_cutoff(0.0)`): a cutoff of
/// `1e-14` over the circuit's hundreds of splits moves a probability by
/// about `1e-9`.
///
/// # Panics
///
/// When a read differs from the reference's, the chain outgrows its
/// capacity, or a call fails.
pub fn a_long_circuit_agrees_with_the_reference_without_truncation<A: SiteAlgebra>(
    engine: &CanonicalMpsEngine<A>,
) {
    let sites = 8;
    let reference = ExactReference::new();
    let mut state = engine.prepare(&chain(sites, 64)).expect("prepare");
    let mut exact_state = reference.prepare(&exact(sites)).expect("prepare");
    let mut next = xorshift(SEED);
    let agree = |state: &CanonicalMpsState<A>, exact_state: &ReferenceState| {
        assert_reads_agree(engine, state, exact_state, 1e-10);
        let value = engine.expectation(state, &mixed_string()).expect("value");
        let expected = reference
            .expectation(exact_state, &mixed_string())
            .expect("value");
        assert!((value - expected).norm() < 1e-10, "{value} vs {expected}");
    };
    for step in 0..120 {
        let x = f64::from(u32::try_from(step).expect("small"));
        let a = next(sites);
        let b = (a + 1 + next(sites - 1)) % sites;
        let gate = if step % 3 == 0 {
            cnot()
        } else {
            entangler(0.3 + 0.05 * x)
        };
        apply_to_both(engine, &mut state, &mut exact_state, &[a, b], &gate);
        let turn = ry(0.2 + 0.01 * x);
        apply_to_both(engine, &mut state, &mut exact_state, &[a], &turn);
        if step % 40 == 39 {
            agree(&state, &exact_state);
            let site = next(sites);
            let p1 = reference
                .probability_of_one(&exact_state, site)
                .expect("P(One)");
            let one = p1 > 0.5;
            engine.project(&mut state, site, one).expect("project");
            reference
                .project(&mut exact_state, site, one)
                .expect("project");
        }
    }
    agree(&state, &exact_state);
    let shape = engine.shape(&state).expect("shape");
    assert!(shape.fits_within(&chain(sites, 64)), "{shape:?}");
}

/// A circuit of weak entangling gates on 8 sites, mostly far apart, at a
/// bond of 4, which truncates, on `engine`: the distribution of all eight
/// qubits stays within the distance the splits' kept shares allow of the
/// reference's.
///
/// The bound holds on any backend that keeps the contract. A split of a
/// normalised state `ψ` that drops a share `ε` of its squared Schmidt
/// values and renormalises maps it to `Pψ / ‖Pψ‖`, at distance
/// `√(2 − 2√(1 − ε)) ≤ √(2ε)`; gates and moves of the centre keep
/// distances, so after splits dropping `ε₁, ε₂, …` the state is within
/// `Σ √(2εᵢ)` of the exact one, up to a global phase, and the total
/// variation distance of its distribution is at most that. The retained
/// weight is `W = Π (1 − εᵢ)`, so `Σ εᵢ ≤ −ln W`, and over at most `k`
/// splits, the statistics' count, `Σ √(2εᵢ) ≤ √(2k · (−ln W))`. The circuit
/// projects nothing: a projection divides by a probability, which can
/// enlarge the distance.
///
/// `engine` must have a cutoff of 0 (`with_cutoff(0.0)`), so that the
/// bond alone truncates and the distance is large enough to compare.
///
/// # Panics
///
/// When the distance exceeds the bound, the bound or the distance is too
/// small for the check to mean anything, or a call fails.
pub fn a_truncated_circuit_stays_within_the_distance_its_splits_allow<A: SiteAlgebra>(
    engine: &CanonicalMpsEngine<A>,
) {
    let sites = 8;
    let reference = ExactReference::new();
    let mut state = engine.prepare(&chain(sites, 4)).expect("prepare");
    let mut exact_state = reference.prepare(&exact(sites)).expect("prepare");
    let mut next = xorshift(SEED);
    for step in 0..20 {
        let x = f64::from(u32::try_from(step).expect("small"));
        let a = next(sites);
        let b = (a + 1 + next(sites - 1)) % sites;
        let gate = entangler(0.05 + 0.01 * x);
        apply_to_both(engine, &mut state, &mut exact_state, &[a, b], &gate);
        let turn = ry(0.2 + 0.01 * x);
        apply_to_both(engine, &mut state, &mut exact_state, &[a], &turn);
    }
    let weight = engine.retained_weight(&state).expect("weight");
    let splits = state.split_statistics().splits();
    let bound = (2.0 * f64::from(u32::try_from(splits).expect("few splits")) * -weight.ln()).sqrt();
    let tvd: f64 = distribution(engine, &state, sites)
        .iter()
        .zip(distribution(&reference, &exact_state, sites))
        .map(|(p, q)| 0.5 * (p - q).abs())
        .sum();
    assert!(
        tvd > 1e-5,
        "precondition: the cap truncates, so the distributions differ \
         (TVD {tvd:e})"
    );
    assert!(
        bound < 0.2,
        "precondition: the bound is far below 1, so the check is not \
         vacuous (bound {bound:e})"
    );
    assert!(tvd <= bound, "TVD {tvd:e} above the bound {bound:e}");
}

/// The seed of every circuit's choice of sites.
const SEED: u64 = 0x2545_F491_4F6C_DD1D;

/// Each site's P(One) on `state` and on the reference's `exact_state`
/// agree within `tolerance`.
pub(crate) fn assert_reads_agree<A: SiteAlgebra>(
    engine: &CanonicalMpsEngine<A>,
    state: &CanonicalMpsState<A>,
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

/// The probability of every outcome of `sites` qubits, the outcome's bit
/// `q` the value of qubit `q`: the product of the probabilities of its
/// values, each projected on a fork in turn.
fn distribution<E: MpsEngine>(engine: &E, state: &E::State, sites: usize) -> Vec<f64> {
    let mut out = vec![0.0; 1 << sites];
    let fork = engine.fork(state).expect("fork");
    branch(engine, &fork, 0, 0, 1.0, &mut out);
    out
}

/// Fills `out` with the outcomes that extend `prefix`, the values of the
/// qubits below `site`, which `state` is projected on with probability `p`.
fn branch<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    site: usize,
    prefix: usize,
    p: f64,
    out: &mut [f64],
) {
    if 1 << site == out.len() {
        out[prefix] = p;
        return;
    }
    let p_one = engine.probability_of_one(state, site).expect("P(One)");
    for (one, possible) in [(false, p_one < 1.0), (true, p_one > 0.0)] {
        if possible {
            let mut fork = engine.fork(state).expect("fork");
            let q = engine.project(&mut fork, site, one).expect("project");
            branch(
                engine,
                &fork,
                site + 1,
                prefix | usize::from(one) << site,
                p * q,
                out,
            );
        }
    }
}

/// Applies `gate` on `qubits` to the engine's state and to the reference's.
fn apply_to_both<A: SiteAlgebra>(
    engine: &CanonicalMpsEngine<A>,
    state: &mut CanonicalMpsState<A>,
    exact_state: &mut ReferenceState,
    qubits: &[usize],
    gate: &[Complex64],
) {
    engine.apply(state, qubits, gate).expect("apply");
    ExactReference::new()
        .apply(exact_state, qubits, gate)
        .expect("apply");
}

/// A xorshift generator from `seed`: each call gives a number below its
/// argument, the same sequence on every run.
fn xorshift(mut seed: u64) -> impl FnMut(usize) -> usize {
    move |below| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % u64::try_from(below).expect("small")).expect("small")
    }
}

/// The complex number `re + 0i`.
pub(crate) fn real(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

/// The turn about Y by `theta`, row-major.
pub(crate) fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![real(k), real(-s), real(s), real(k)]
}

/// The CNOT on (control, target), row-major.
pub(crate) fn cnot() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
        m[4 * row + col] = real(1.0);
    }
    m
}

/// A complex entangling gate, symmetric under the exchange of its operands
/// and equal to its transpose: a partial iSWAP on |01⟩, |10⟩ and a phase on
/// |11⟩.
pub(crate) fn entangler(theta: f64) -> Vec<Complex64> {
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
pub(crate) fn chain(sites: usize, chi: usize) -> MpsShape {
    MpsShape::qubit_chain(sites, chi).expect("a chain of qubits")
}

/// A chain of `sites` qubits that holds every state: for the exact
/// reference.
pub(crate) fn exact(sites: usize) -> MpsShape {
    chain(sites, 1 << sites)
}

/// `0.25 + (0.5 − i)·X₀Y₃Z₅`: an observable with a constant, a complex
/// coefficient and every Pauli.
pub(crate) fn mixed_string() -> PauliSum {
    let factors = vec![(0, Pauli::X), (3, Pauli::Y), (5, Pauli::Z)];
    PauliSum::new(
        real(0.25),
        vec![PauliTerm::new(Complex64::new(0.5, -1.0), factors).expect("a term")],
    )
    .expect("an observable")
}
