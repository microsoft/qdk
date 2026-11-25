// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's conformance check: two circuits that run a
//! [`CanonicalMpsEngine`] on a backend's algebra. They compare what it
//! reads with `tensornet`'s exact reference. Its sibling, the backend kit
//! ([`site_algebra::conformance`](crate::site_algebra::conformance)),
//! checks the backend's calls one at a time. The circuits add hundreds of
//! calls in a row, on the backend's own memory and settings. So a backend's
//! tests run them on its device too:
//!
//! ```no_run
//! use canonical_mps::CpuAlgebra;
//! use canonical_mps::engine_conformance::{
//!     assert_truncated_circuit_stays_within_its_truncation_bound,
//!     assert_untruncated_circuit_agrees_with_exact_reference,
//! };
//!
//! assert_untruncated_circuit_agrees_with_exact_reference(CpuAlgebra);
//! assert_truncated_circuit_stays_within_its_truncation_bound(CpuAlgebra);
//! ```
//!
//! Each function builds the engine on the algebra it is given, at the
//! cutoff its circuit needs, and asserts its rule. So a test calls it and
//! nothing else. The circuits' sites are drawn from one fixed seed, which a
//! broken rule's panic names, so that a failure repeats. The truncated
//! circuit's check means something only on a circuit that truncates within
//! a small bound, which its preconditions check. The module needs the
//! `conformance` feature.

use crate::site_algebra::SiteAlgebra;
use crate::{CanonicalMpsEngine, CanonicalMpsState};
use num_complex::Complex64;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::sampling::SplitMix64;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// A pseudo-random circuit on 8 sites, mostly on sites far apart, with
/// projections on the way, on the engine on `algebra`. Every read agrees
/// with the exact reference within `1e-10`.
///
/// The engine truncates nothing: a cutoff of 0, under a bond the circuit
/// never reaches. A cutoff of [`DEFAULT_CUTOFF`](crate::DEFAULT_CUTOFF)
/// over the circuit's hundreds of splits would move a probability by about
/// `1e-9`.
///
/// # Panics
///
/// When a read differs from the exact reference's, the chain outgrows its
/// capacity, or a call fails.
pub fn assert_untruncated_circuit_agrees_with_exact_reference<A: SiteAlgebra>(algebra: A) {
    assert_untruncated_circuit_agrees_on(&CanonicalMpsEngine::new(algebra).with_cutoff(0.0));
}

/// The circuit of [`assert_untruncated_circuit_agrees_with_exact_reference`]
/// on `engine`. The engine must have a cutoff of 0, for the reason that
/// function gives. The crate's tests run it on the unfused engine too.
pub(super) fn assert_untruncated_circuit_agrees_on<A: SiteAlgebra>(engine: &CanonicalMpsEngine<A>) {
    let mut states = ComparedStates::prepare(engine, 64);
    let mut draws = SiteDraws::new(SEED);
    for step in 0..120 {
        let [a, b] = draws.distinct_pair();
        states.apply(&[a, b], &untruncated_circuit_gate(step));
        states.apply(&[a], &one_site_rotation(step));
        if step % 40 == 39 {
            states.assert_agree();
            states.project_on_likelier_value(draws.site());
        }
    }
    states.assert_agree();
    states.assert_within_capacity();
}

/// The two-site gate of the untruncated circuit's `step`: a CNOT every
/// third step, an [`entangler`] otherwise.
fn untruncated_circuit_gate(step: u32) -> Vec<Complex64> {
    if step.is_multiple_of(3) {
        cnot()
    } else {
        entangler(0.3 + 0.05 * f64::from(step))
    }
}

/// The two-site gate of the truncated circuit's `step`: a weak
/// [`entangler`].
fn truncated_circuit_gate(step: u32) -> Vec<Complex64> {
    entangler(0.05 + 0.01 * f64::from(step))
}

/// The one-site rotation both circuits apply after the two-site gate of
/// their `step`.
fn one_site_rotation(step: u32) -> Vec<Complex64> {
    ry(0.2 + 0.01 * f64::from(step))
}

/// A circuit of weak entangling gates on 8 sites, mostly far apart, at a
/// bond of 4, which truncates, on the engine on `algebra`. The distribution
/// of all eight qubits stays within its truncation bound. That bound is the
/// distance from the exact reference's that the splits' retained fractions
/// allow.
///
/// The bound holds on any backend that keeps the contract. It leaves out
/// what the MPOs' factorisations drop at or below the operator's rank
/// cutoff, which the retained weight does not count and these gates do not
/// reach. It follows in four steps:
///
/// 1. A split of a normalised state `ψ` that drops a share `ε` of its
///    squared Schmidt values and renormalises maps it to `Pψ / ‖Pψ‖`. That
///    is at distance `√(2 − 2√(1 − ε)) ≤ √(2ε)`.
/// 2. Gates and moves of the centre keep distances. So after splits
///    dropping `ε₁, ε₂, …`, up to a global phase, the state is within
///    `Σ √(2εᵢ)` of the exact one.
/// 3. The total variation distance of its distribution is at most that.
///    It is half the sum, over the outcomes, of the sizes of the
///    differences of their probabilities.
/// 4. The retained weight is `W = Π (1 − εᵢ)`, so `Σ εᵢ ≤ −ln W`. Over at
///    most `k` splits, the statistics' count,
///    `Σ √(2εᵢ) ≤ √(2k · (−ln W))`.
///
/// The circuit projects nothing. A projection divides by a probability,
/// which can enlarge the distance. The engine has a cutoff of 0, so the
/// bond alone truncates and the distance is large enough to compare.
///
/// # Panics
///
/// When the distance exceeds the bound, or when a call fails. Also when the
/// distance is too small, or the bound too large, for the check to mean
/// anything.
pub fn assert_truncated_circuit_stays_within_its_truncation_bound<A: SiteAlgebra>(algebra: A) {
    let engine = &CanonicalMpsEngine::new(algebra).with_cutoff(0.0);
    let mut states = ComparedStates::prepare(engine, 4);
    let mut draws = SiteDraws::new(SEED);
    for step in 0..20 {
        let [a, b] = draws.distinct_pair();
        states.apply(&[a, b], &truncated_circuit_gate(step));
        states.apply(&[a], &one_site_rotation(step));
    }
    let bound = states.truncation_bound();
    let tvd = states.total_variation_distance();
    assert!(
        tvd > 1e-5,
        "circuit seed {SEED:#x}: precondition: the cap truncates, so the \
         distributions differ (TVD {tvd:e})"
    );
    assert!(
        bound < 0.2,
        "circuit seed {SEED:#x}: precondition: the bound is far below 1, so \
         the check is not vacuous (bound {bound:e})"
    );
    assert!(
        tvd <= bound,
        "circuit seed {SEED:#x}: TVD {tvd:e} above the bound {bound:e}"
    );
}

/// The seed of every circuit's choice of sites.
const SEED: u64 = 0x2545_F491_4F6C_DD1D;

/// The sites of both circuits' chains.
const SITES: usize = 8;

/// The engine's state and the exact reference's, under the same circuit,
/// so that their reads compare. A failed check names [`SEED`].
struct ComparedStates<'e, A: SiteAlgebra> {
    engine: &'e CanonicalMpsEngine<A>,
    /// The engine's capacity: [`SITES`] qubits under a bond cap.
    capacity: MpsShape,
    state: CanonicalMpsState<A>,
    reference: ExactReference,
    exact_state: ReferenceState,
}

impl<'e, A: SiteAlgebra> ComparedStates<'e, A> {
    /// Both states at `|0…0⟩`: the engine's with every bond at most
    /// `bond_cap`, the reference's exact.
    fn prepare(engine: &'e CanonicalMpsEngine<A>, bond_cap: usize) -> Self {
        let capacity = capacity(SITES, bond_cap);
        let state = engine.prepare(&capacity).expect("prepare");
        let reference = ExactReference::new();
        let exact_state = reference.prepare(&full_capacity(SITES)).expect("prepare");
        Self {
            engine,
            capacity,
            state,
            reference,
            exact_state,
        }
    }

    /// Applies `gate` on `qubits` to both states.
    fn apply(&mut self, qubits: &[usize], gate: &[Complex64]) {
        self.engine
            .apply(&mut self.state, qubits, gate)
            .expect("apply");
        self.reference
            .apply(&mut self.exact_state, qubits, gate)
            .expect("apply");
    }

    /// Asserts that every site's P(One), and the value of
    /// [`mixed_string`], agree within `1e-10`.
    fn assert_agree(&self) {
        let (engine, state, exact_state) = (self.engine, &self.state, &self.exact_state);
        if let Some(disagreement) = disagreeing_read(engine, state, exact_state, 1e-10) {
            panic!("circuit seed {SEED:#x}: {disagreement}");
        }
        let value = engine.expectation(state, &mixed_string()).expect("value");
        let expected = self
            .reference
            .expectation(exact_state, &mixed_string())
            .expect("value");
        assert!(
            (value - expected).norm() < 1e-10,
            "circuit seed {SEED:#x}: expectation {value} vs {expected}"
        );
    }

    /// Projects both states' `site` on its likelier value, as the exact
    /// reference reads it.
    fn project_on_likelier_value(&mut self, site: usize) {
        let p_one = self
            .reference
            .probability_of_one(&self.exact_state, site)
            .expect("P(One)");
        let one = p_one > 0.5;
        self.engine
            .project(&mut self.state, site, one)
            .expect("project");
        self.reference
            .project(&mut self.exact_state, site, one)
            .expect("project");
    }

    /// Asserts that the engine's chain still fits the capacity it was
    /// prepared with.
    fn assert_within_capacity(&self) {
        let shape = self.engine.shape(&self.state).expect("shape");
        assert!(
            shape.fits_within(&self.capacity),
            "circuit seed {SEED:#x}: {shape:?}"
        );
    }

    /// The bound `√(2k · (−ln W))` on the distance from the exact state,
    /// from the engine's retained weight `W` over its `k` splits. The
    /// weight is read first: that read applies the gates the engine left
    /// pending, and the count then includes their splits.
    fn truncation_bound(&self) -> f64 {
        let weight = self.engine.retained_weight(&self.state).expect("weight");
        let splits = self.state.statistics().splits();
        let splits = f64::from(u32::try_from(splits).expect("few splits"));
        (2.0 * splits * -weight.ln()).sqrt()
    }

    /// The total variation distance between the two states' distributions
    /// of all [`SITES`] qubits.
    fn total_variation_distance(&self) -> f64 {
        let engine_distribution = distribution(self.engine, &self.state, SITES);
        let exact_distribution = distribution(&self.reference, &self.exact_state, SITES);
        distance_between_distributions(&engine_distribution, &exact_distribution)
    }
}

/// `Σ ½·|p(x) − q(x)|` over the outcomes `x`.
fn distance_between_distributions(p: &[f64], q: &[f64]) -> f64 {
    p.iter().zip(q).map(|(p, q)| 0.5 * (p - q).abs()).sum()
}

/// The circuits' choice of sites, drawn from `tensornet`'s [`SplitMix64`]:
/// the same sequence on every run of a seed.
struct SiteDraws(SplitMix64);

impl SiteDraws {
    fn new(seed: u64) -> Self {
        Self(SplitMix64::new(seed))
    }

    /// One of the [`SITES`] sites.
    fn site(&mut self) -> usize {
        self.below(SITES)
    }

    /// Two distinct sites: the second at a random distance after the
    /// first, wrapping around the chain, so mostly far apart.
    fn distinct_pair(&mut self) -> [usize; 2] {
        let first = self.site();
        let second = (first + 1 + self.below(SITES - 1)) % SITES;
        [first, second]
    }

    /// A number below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).expect("a count fits a u64");
        usize::try_from(self.0.next_u64() % bound).expect("below a usize")
    }
}

/// Each site's P(One) on `state` and on the exact reference's `exact_state`
/// agree within `tolerance`.
#[cfg(test)]
pub(super) fn assert_reads_agree<A: SiteAlgebra>(
    engine: &CanonicalMpsEngine<A>,
    state: &CanonicalMpsState<A>,
    exact_state: &ReferenceState,
    tolerance: f64,
) {
    if let Some(disagreement) = disagreeing_read(engine, state, exact_state, tolerance) {
        panic!("{disagreement}");
    }
}

/// The first site whose P(One) on `state` and on the exact reference's
/// `exact_state` differ by `tolerance` or more, with both values, or `None`.
fn disagreeing_read<A: SiteAlgebra>(
    engine: &CanonicalMpsEngine<A>,
    state: &CanonicalMpsState<A>,
    exact_state: &ReferenceState,
    tolerance: f64,
) -> Option<String> {
    let reference = ExactReference::new();
    (0..exact_state.site_count()).find_map(|site| {
        let engine_p = engine.probability_of_one(state, site).expect("P(One)");
        let exact_p = reference
            .probability_of_one(exact_state, site)
            .expect("P(One)");
        let agree = (engine_p - exact_p).abs() < tolerance;
        (!agree).then(|| format!("site {site}: P(One) {engine_p} vs {exact_p}"))
    })
}

/// The probability of every outcome of `sites` qubits, the outcome's bit
/// `q` the value of qubit `q`. It is the product of the probabilities of
/// its values, each projected on a fork in turn, down a tree of forks:
///
/// ```text
///  2 sites; bit q of an outcome is the value of qubit q
///
///                      state (probability 1)
///             ┌──────────────────┴──────────────────┐
///  site 0:  project on 0 (× P(0))         project on 1 (× P(1))
///          ┌───────┴───────┐               ┌───────┴───────┐
///  site 1: 0               1               0               1
///  outcome: 0b00           0b10            0b01            0b11
/// ```
///
/// A value of probability 0 is not projected on: its subtree's outcomes
/// stay 0.
fn distribution<E: MpsEngine>(engine: &E, state: &E::State, sites: usize) -> Vec<f64> {
    let mut probabilities = vec![0.0; 1 << sites];
    let fork = engine.fork(state).expect("fork");
    branch(engine, &fork, 0, 0, 1.0, &mut probabilities);
    probabilities
}

/// Fills `probabilities` with the outcomes that extend `outcome`, the
/// values of the qubits below `site`, which `state` is projected on with
/// probability `probability`.
fn branch<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    site: usize,
    outcome: usize,
    probability: f64,
    probabilities: &mut [f64],
) {
    if 1 << site == probabilities.len() {
        probabilities[outcome] = probability;
        return;
    }
    let p_one = engine.probability_of_one(state, site).expect("P(One)");
    for (one, possible) in [(false, p_one < 1.0), (true, p_one > 0.0)] {
        if possible {
            let mut fork = engine.fork(state).expect("fork");
            let p_value = engine.project(&mut fork, site, one).expect("project");
            branch(
                engine,
                &fork,
                site + 1,
                outcome | usize::from(one) << site,
                probability * p_value,
                probabilities,
            );
        }
    }
}

/// The complex number `re + 0i`.
pub(super) fn real(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

/// The turn about Y by `theta`, row-major.
pub(super) fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![real(k), real(-s), real(s), real(k)]
}

/// The CNOT on (control, target), row-major.
pub(super) fn cnot() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
        m[4 * row + col] = real(1.0);
    }
    m
}

/// A complex entangling gate, symmetric under the exchange of its operands
/// and equal to its transpose. It is a partial iSWAP on |01⟩, |10⟩ and a
/// phase on |11⟩.
pub(super) fn entangler(theta: f64) -> Vec<Complex64> {
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

/// The capacity of a chain of `sites` qubits whose bonds are at most
/// `bond_cap`.
pub(super) fn capacity(sites: usize, bond_cap: usize) -> MpsShape {
    MpsShape::qubit_chain(sites, bond_cap).expect("a chain of qubits")
}

/// The capacity of a chain of `sites` qubits that holds every state, so
/// nothing is truncated. The exact reference uses it.
pub(super) fn full_capacity(sites: usize) -> MpsShape {
    capacity(sites, 1 << sites)
}

/// `0.25 + (0.5 − i)·X₀Y₃Z₅`: an observable with a constant, a complex
/// coefficient and every Pauli.
pub(super) fn mixed_string() -> PauliSum {
    let factors = vec![(0, Pauli::X), (3, Pauli::Y), (5, Pauli::Z)];
    PauliSum::new(
        real(0.25),
        vec![PauliTerm::new(Complex64::new(0.5, -1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

#[cfg(test)]
mod tests;
