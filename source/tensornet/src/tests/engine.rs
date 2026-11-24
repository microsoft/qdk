// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The trait's default `conditional_probability_of_one`, through an engine
//! that keeps it and samples from it; the contract's other code is tested
//! in its own files.

mod checks;
mod reference;
mod sampling;

use super::{c, cx, hadamard};
use crate::reference::{ExactReference, ReferenceError, ReferenceState};
use crate::{MpsEngine, MpsShape, PauliSum, Refusal};
use num_complex::Complex64;

const REFERENCE: ExactReference = ExactReference;

/// The reference without its own conditional probability, so that the
/// trait's default answers it, and draws its samples from that default: an
/// engine that has no projected contraction.
struct DefaultConditional;

impl MpsEngine for DefaultConditional {
    type State = ReferenceState;
    type Error = ReferenceError;

    fn prepare(&self, capacity: &MpsShape) -> Result<ReferenceState, ReferenceError> {
        REFERENCE.prepare(capacity)
    }
    fn apply(
        &self,
        state: &mut ReferenceState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), ReferenceError> {
        REFERENCE.apply(state, sites, matrix)
    }
    fn probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
    ) -> Result<f64, ReferenceError> {
        REFERENCE.probability_of_one(state, site)
    }
    fn project(
        &self,
        state: &mut ReferenceState,
        site: usize,
        one: bool,
    ) -> Result<f64, ReferenceError> {
        REFERENCE.project(state, site, one)
    }
    fn fork(&self, state: &ReferenceState) -> Result<ReferenceState, ReferenceError> {
        REFERENCE.fork(state)
    }
    /// Draws from this engine's own conditional probabilities, the trait's
    /// default, as an engine with no sampler of its own would.
    fn sample(
        &self,
        state: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: u64,
    ) -> Result<Vec<bool>, ReferenceError> {
        crate::checks::sample(state.site_count(), sites)?;
        crate::sampling::sample_by_conditionals(sites, shots, seed, |site, given| {
            self.conditional_probability_of_one(state, site, given)
        })
    }
    fn expectation(
        &self,
        state: &ReferenceState,
        observable: &PauliSum,
    ) -> Result<Complex64, ReferenceError> {
        REFERENCE.expectation(state, observable)
    }
    fn shape(&self, state: &ReferenceState) -> Result<MpsShape, ReferenceError> {
        REFERENCE.shape(state)
    }
    fn retained_weight(&self, state: &ReferenceState) -> Result<f64, ReferenceError> {
        REFERENCE.retained_weight(state)
    }
}

/// Three entangled sites with no value certain: `Ry(0.7)` on site 0, H on
/// site 1, then CX from 0 to 2 and from 1 to 0.
fn entangled() -> ReferenceState {
    let capacity = MpsShape::qubit_chain(3, usize::MAX).expect("three sites form a chain");
    let mut state = REFERENCE.prepare(&capacity).expect("three sites are held");
    let (s, k) = 0.35f64.sin_cos();
    let ry = [c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)];
    for (sites, matrix) in [
        (&[0][..], &ry[..]),
        (&[1], &hadamard()),
        (&[0, 2], &cx()),
        (&[1, 0], &cx()),
    ] {
        REFERENCE
            .apply(&mut state, sites, matrix)
            .expect("unitary gates on sites in range");
    }
    state
}

fn conditional(
    state: &ReferenceState,
    site: usize,
    given: &[(usize, bool)],
) -> Result<f64, Option<Refusal>> {
    DefaultConditional
        .conditional_probability_of_one(state, site, given)
        .map_err(|e| crate::MpsEngineError::refusal(&e).cloned())
}

/// Every condition on the other two sites, in both orders.
#[test]
fn the_default_conditional_probability_is_the_exact_one() {
    let state = entangled();
    for site in 0..3 {
        let others: Vec<usize> = (0..3).filter(|&at| at != site).collect();
        for values in 0..4 {
            let given: Vec<(usize, bool)> = others
                .iter()
                .enumerate()
                .map(|(k, &at)| (at, values >> k & 1 == 1))
                .collect();
            let reversed: Vec<(usize, bool)> = given.iter().rev().copied().collect();
            let expected = REFERENCE
                .conditional_probability_of_one(&state, site, &given)
                .expect("no value of the state is certain");
            for given in [given, reversed] {
                let actual = conditional(&state, site, &given).expect("a valid condition");
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "site {site} given {given:?}: {actual}, not {expected}"
                );
            }
        }
    }
}

#[test]
fn the_default_conditional_probability_given_nothing_is_the_probability() {
    let state = entangled();
    let expected = REFERENCE
        .probability_of_one(&state, 2)
        .expect("site 2 exists");
    let actual = conditional(&state, 2, &[]).expect("no condition");
    assert!((actual - expected).abs() < 1e-12, "{actual}");
}

/// The answer of a site among its own conditions is its value: 1 for One,
/// 0 for Zero.
#[test]
fn the_default_conditional_probability_of_a_given_site_is_its_value() {
    let state = entangled();
    for (one, expected) in [(true, 1.0), (false, 0.0)] {
        let actual = conditional(&state, 1, &[(0, false), (1, one)]).expect("a valid condition");
        assert!(
            (actual - expected).abs() < 1e-12,
            "site 1 given {one}: {actual}"
        );
    }
}

#[test]
fn the_default_conditional_probability_runs_the_checks() {
    let state = entangled();
    assert_eq!(
        conditional(&state, 0, &[(1, true), (1, true)]),
        Err(Some(Refusal::RepeatedSite { site: 1 }))
    );
}

/// On a GHZ state, site 0 One and site 1 Zero never happen together: the
/// projection on the second value refuses it, and the default passes its
/// refusal on. On a state whose norm is NaN or zero, every probability is
/// NaN, and with no condition nothing is projected: the default refuses the
/// empty condition itself. A NaN outside the states a condition selects
/// still makes the norm NaN, so the default refuses that condition too, as
/// the reference does.
#[test]
fn the_default_conditional_probability_refuses_a_condition_of_probability_zero_or_nan() {
    let capacity = MpsShape::qubit_chain(3, usize::MAX).expect("three sites form a chain");
    let mut ghz = REFERENCE.prepare(&capacity).expect("three sites are held");
    for (sites, matrix) in [
        (&[0][..], &hadamard()[..]),
        (&[0, 1], &cx()),
        (&[1, 2], &cx()),
    ] {
        REFERENCE
            .apply(&mut ghz, sites, matrix)
            .expect("unitary gates on sites in range");
    }
    assert_eq!(
        conditional(&ghz, 2, &[(0, true), (1, false)]),
        Err(Some(Refusal::ZeroProbabilityOutcome))
    );
    let zero = c(0.0, 0.0);
    for amplitudes in [[c(f64::NAN, 0.0), c(1.0, 0.0)], [zero, zero]] {
        let state = ReferenceState::from_amplitudes(&amplitudes);
        assert_eq!(
            conditional(&state, 0, &[]),
            Err(Some(Refusal::ZeroProbabilityOutcome)),
            "{amplitudes:?}"
        );
    }
    // Site 1 Zero selects basis states 0 and 1, both finite; the NaN is on
    // basis state 2.
    let nan_elsewhere =
        ReferenceState::from_amplitudes(&[c(1.0, 0.0), zero, c(f64::NAN, 0.0), zero]);
    assert_eq!(
        conditional(&nan_elsewhere, 0, &[(1, false)]),
        Err(Some(Refusal::ZeroProbabilityOutcome))
    );
}

/// An engine with no sampler of its own draws from its conditional
/// probabilities, so on a state whose norm is NaN or zero its first draw
/// meets the default's refusal, and `sample` passes it on rather than
/// drawing from a NaN.
#[test]
fn sampling_from_the_default_conditional_probability_passes_its_refusal_on() {
    let zero = c(0.0, 0.0);
    for amplitudes in [[c(f64::NAN, 0.0), c(1.0, 0.0)], [zero, zero]] {
        let state = ReferenceState::from_amplitudes(&amplitudes);
        let refusal = DefaultConditional
            .sample(&state, &[0], 1, 7)
            .map_err(|e| crate::MpsEngineError::refusal(&e).cloned());
        assert_eq!(
            refusal,
            Err(Some(Refusal::ZeroProbabilityOutcome)),
            "{amplitudes:?}"
        );
    }
}
