// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The exact reference's promises, each pinned by a test: its layout of
//! basis states and operators, its refusals and failures, its exact reads
//! (probabilities, conditionals, expectations, projections) down to the
//! precision its rustdoc states, and its chain, whose bonds are the
//! Schmidt ranks with the threshold its rustdoc states.
//!
//! Every other engine is checked against the reference, so a fault here
//! would pass a wrong engine or fail a right one. Hence its Schmidt ranks
//! are also checked against an independent implementation, a singular value
//! decomposition, on random circuits.

use crate::reference::{ExactReference, ReferenceError, ReferenceState};
use crate::sampling::SplitMix64;
use crate::tests::{c, cx, hadamard};
use crate::{MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, PauliTerm, Refusal};
use nalgebra::DMatrix;
use num_complex::Complex64;
use std::f64::consts::FRAC_PI_4;

const ENGINE: ExactReference = ExactReference;

fn prepared(sites: usize) -> ReferenceState {
    let capacity = MpsShape::qubit_chain(sites, usize::MAX).expect("the test's sites form a chain");
    ENGINE
        .prepare(&capacity)
        .expect("the test's sites are held")
}

fn apply(state: &mut ReferenceState, sites: &[usize], matrix: &[Complex64]) {
    ENGINE
        .apply(state, sites, matrix)
        .expect("the test's gates are unitary, on sites the state has");
}

/// (|00⟩ + |11⟩)/√2 on sites 0 and 1, the others |0⟩.
fn bell(sites: usize) -> ReferenceState {
    let mut state = prepared(sites);
    apply(&mut state, &[0], &hadamard());
    apply(&mut state, &[0, 1], &cx());
    state
}

fn observable(
    identity_coefficient: Complex64,
    terms: &[(Complex64, &[(usize, Pauli)])],
) -> PauliSum {
    let terms = terms
        .iter()
        .map(|(coefficient, factors)| {
            PauliTerm::new(*coefficient, factors.to_vec()).expect("a term on distinct sites")
        })
        .collect();
    PauliSum::new(identity_coefficient, terms).expect("finite coefficients")
}

fn expectation(state: &ReferenceState, observable: &PauliSum) -> Complex64 {
    ENGINE
        .expectation(state, observable)
        .expect("the observable is on the state's sites")
}

#[test]
fn a_prepared_state_is_all_zeros() {
    let state = prepared(3);
    let amplitudes: Vec<Complex64> = (0..8).map(|i| state.amplitude(i)).collect();
    assert_eq!(amplitudes[0], c(1.0, 0.0));
    assert!(amplitudes[1..].iter().all(|&a| a == c(0.0, 0.0)));
}

/// A prepared state is a product state, so its chain has every bond 1: on
/// three sites, and on the one site of a one-qubit program's state.
#[test]
fn a_prepared_state_is_a_product_chain() {
    let rows = [
        (
            3,
            MpsShape::qubit_chain(3, 1).expect("three sites form a chain"),
        ),
        (
            1,
            MpsShape::new(vec![vec![2]]).expect("a lone qubit is a chain"),
        ),
    ];
    for (sites, expected) in rows {
        let shape = ENGINE
            .shape(&prepared(sites))
            .expect("the reference reports its chain");
        assert_eq!(shape, expected, "{sites} sites");
    }
}

#[test]
fn basis_state_i_has_site_k_as_bit_k() {
    let mut state = prepared(3);
    apply(&mut state, &[1], &Pauli::X.matrix());
    assert_eq!(state.amplitude(0b010), c(1.0, 0.0));
}

/// CX on [a, b] flips site b when site a is set; on [b, a], the reverse:
/// on neighbours, and on distant sites, the site between them untouched.
#[test]
fn the_first_site_of_an_operator_is_its_most_significant_bit() {
    // (sites, [a, b], the basis state after CX on [a, b], then on [b, a])
    let rows = [(2, [0, 1], 0b11, 0b10), (3, [0, 2], 0b101, 0b100)];
    for (sites, [a, b], after, after_reversed) in rows {
        let mut state = prepared(sites);
        apply(&mut state, &[a], &Pauli::X.matrix());
        apply(&mut state, &[a, b], &cx());
        assert_eq!(state.amplitude(after), c(1.0, 0.0), "CX on [{a}, {b}]");
        apply(&mut state, &[b, a], &cx());
        assert_eq!(
            state.amplitude(after_reversed),
            c(1.0, 0.0),
            "CX on [{b}, {a}]"
        );
    }
}

#[test]
fn the_reference_holds_its_largest_number_of_sites() {
    let state = prepared(ExactReference::MAX_SITES);
    assert_eq!(state.site_count(), ExactReference::MAX_SITES);
}

/// Another engine holds such a state, so the reference fails rather than
/// refuses.
#[test]
fn more_sites_than_the_reference_holds_is_a_failure_not_a_refusal() {
    let sites = ExactReference::MAX_SITES + 1;
    let capacity = MpsShape::qubit_chain(sites, 2).expect("the sites form a chain");
    let error = ENGINE
        .prepare(&capacity)
        .expect_err("the reference holds at most MAX_SITES sites");
    assert_eq!(
        error,
        ReferenceError::ChainTooLong {
            requested: sites,
            max: ExactReference::MAX_SITES
        }
    );
    assert_eq!(error.refusal(), None);
}

/// A capacity no engine takes is refused, whatever its size.
#[test]
fn a_refused_capacity_is_refused_before_its_size_is_checked() {
    let mut sites = vec![vec![2, 1]];
    sites.extend((0..ExactReference::MAX_SITES - 1).map(|_| vec![1, 2, 1]));
    sites.push(vec![1, 3]);
    let capacity = MpsShape::new(sites).expect("a chain of bond one");
    assert_eq!(
        ENGINE.prepare(&capacity).map_err(|e| e.refusal().cloned()),
        Err(Some(Refusal::NotTwoLevel {
            site: ExactReference::MAX_SITES,
            levels: 3
        }))
    );
}

#[test]
fn an_error_holding_a_refusal_displays_as_the_refusal() {
    let error = ReferenceError::from(Refusal::NoSites);
    assert_eq!(error.to_string(), Refusal::NoSites.to_string());
}

/// A refused apply leaves the state as it was: a measurement's projector
/// passed as a gate, refused only once its deviation from unitary is
/// measured; and the identity on three sites, unitary and of the right
/// length, as a method would pass a Toffoli it did not decompose, refused
/// as too many sites through the contract's check.
#[test]
fn a_refused_apply_leaves_the_state_unchanged() {
    let projector = vec![c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)];
    let identity: Vec<Complex64> = (0..64)
        .map(|at| c(if at % 9 == 0 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    // (scene, state, sites, matrix, refusal)
    let rows = [
        (
            "a projector",
            bell(2),
            vec![1],
            projector,
            Refusal::NotUnitary { deviation: 1.0 },
        ),
        (
            "three sites",
            bell(3),
            vec![0, 1, 2],
            identity,
            Refusal::TooManySites { sites: 3 },
        ),
    ];
    for (scene, mut state, sites, matrix, refusal) in rows {
        let before = state.clone();
        let refused = ENGINE.apply(&mut state, &sites, &matrix);
        assert_eq!(
            refused.map_err(|e| e.refusal().cloned()),
            Err(Some(refusal)),
            "{scene}"
        );
        assert_eq!(state, before, "{scene}: the state changed");
    }
}

/// On (|00⟩ + |11⟩)/√2, site 1 is One only on |11⟩, half the weight.
#[test]
fn the_probability_of_one_is_the_weight_of_the_states_where_the_site_is_one() {
    let p1 = ENGINE
        .probability_of_one(&bell(2), 1)
        .expect("site 1 exists");
    assert!((p1 - 0.5).abs() < 1e-12, "{p1}");
}

/// On (|00⟩ + |11⟩)/√2 projected on site 0's value, site 1 takes that
/// value.
#[test]
fn a_conditional_probability_is_that_of_the_state_projected_on_the_condition() {
    let state = bell(3);
    assert_eq!(
        ENGINE.conditional_probability_of_one(&state, 1, &[(0, true)]),
        Ok(1.0)
    );
    assert_eq!(
        ENGINE.conditional_probability_of_one(&state, 1, &[(0, false)]),
        Ok(0.0)
    );
}

/// On a Bell pair ⟨XX⟩ = ⟨ZZ⟩ = 1, ⟨YY⟩ = −1 and ⟨Z₀⟩ = 0; the identity's
/// coefficient adds, and each term's multiplies.
#[test]
fn an_expectation_adds_the_identity_and_weights_each_term() {
    let state = bell(2);
    let (x, y, z) = (Pauli::X, Pauli::Y, Pauli::Z);
    let sum = observable(
        c(0.5, 0.0),
        &[
            (c(1.0, 0.0), &[(0, x), (1, x)]),
            (c(2.0, 0.0), &[(1, z), (0, z)]),
            (c(4.0, 0.0), &[(0, y), (1, y)]),
            (c(8.0, 0.0), &[(0, z)]),
        ],
    );
    let value = expectation(&state, &sum);
    assert!(
        (value - c(0.5 + 1.0 + 2.0 - 4.0, 0.0)).norm() < 1e-12,
        "{value}"
    );
}

/// σ⁺ = (X + iY)/2 on (|0⟩ + i|1⟩)/√2, where ⟨X⟩ = 0 and ⟨Y⟩ = 1: a
/// non-Hermitian observable has a complex expectation.
#[test]
fn a_complex_coefficient_gives_a_complex_expectation() {
    let mut state = prepared(1);
    apply(&mut state, &[0], &hadamard());
    let s = [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, 1.0)];
    apply(&mut state, &[0], &s);
    let raising = observable(
        c(0.0, 0.0),
        &[
            (c(0.5, 0.0), &[(0, Pauli::X)]),
            (c(0.0, 0.5), &[(0, Pauli::Y)]),
        ],
    );
    let value = expectation(&state, &raising);
    assert!((value - c(0.0, 0.5)).norm() < 1e-12, "{value}");
}

/// On (|000⟩ + i|111⟩)/√2, ⟨YYY⟩ = −1: YYY maps |000⟩ to −i|111⟩, so the
/// expectation is the imaginary part of the amplitudes' product, with the
/// sign three Y factors give.
#[test]
fn each_y_factor_of_a_term_contributes_its_phase() {
    let mut state = prepared(3);
    apply(&mut state, &[0], &hadamard());
    let s = [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, 1.0)];
    apply(&mut state, &[0], &s);
    apply(&mut state, &[0, 1], &cx());
    apply(&mut state, &[1, 2], &cx());
    let y = Pauli::Y;
    let yyy = observable(c(0.0, 0.0), &[(c(1.0, 0.0), &[(0, y), (1, y), (2, y)])]);
    let value = expectation(&state, &yyy);
    assert!((value - c(-1.0, 0.0)).norm() < 1e-12, "{value}");
}

/// The value's amplitudes are kept, divided by the square root of its
/// probability: the state is normalized again.
#[test]
fn a_projection_keeps_the_value_and_divides_by_the_root_of_its_probability() {
    let mut state = bell(2);
    ENGINE
        .project(&mut state, 0, true)
        .expect("One has probability 1/2");
    assert!((state.amplitude(0b11) - c(1.0, 0.0)).norm() < 1e-12);
    assert_eq!(state.amplitude(0b00), c(0.0, 0.0));
}

/// `Ry(0.7)` gives One the probability sin²(0.35), and Zero the rest.
#[test]
fn a_projection_returns_the_probability_of_its_value() {
    let (s, k) = 0.35f64.sin_cos();
    let ry = [c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)];
    for (one, expected) in [(true, s * s), (false, k * k)] {
        let mut state = prepared(2);
        apply(&mut state, &[1], &ry);
        let p = ENGINE
            .project(&mut state, 1, one)
            .expect("both values have probability above zero");
        assert!((p - expected).abs() < 1e-12, "{one}: {p}, not {expected}");
    }
}

/// A row of the next test: its scene, the state, the site, the condition,
/// and the answer, or `None` for a refusal.
type ConditionRow = (
    &'static str,
    ReferenceState,
    usize,
    &'static [(usize, bool)],
    Option<f64>,
);

/// The probability of a condition, over the state's norm, decides it:
/// zero or NaN is refused, anything above zero is answered, however small.
/// A NaN makes the norm NaN wherever it is, so it is refused on a state the
/// condition selects and on one it does not; with no condition, the answer
/// itself is NaN.
#[test]
fn a_condition_is_refused_exactly_when_its_probability_is_zero_or_nan() {
    let (zero, one, nan) = (c(0.0, 0.0), c(1.0, 0.0), c(f64::NAN, 0.0));
    let (s, k) = 1e-9f64.sin_cos();
    let mut tiny = prepared(2);
    apply(
        &mut tiny,
        &[0],
        &[c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)],
    );
    apply(&mut tiny, &[0, 1], &cx());
    let from = ReferenceState::from_amplitudes;
    // Site 1 Zero selects basis states 0 and 1.
    let rows: [ConditionRow; 5] = [
        ("probability zero", bell(3), 0, &[(2, true)], None),
        ("a NaN, no condition", from(&[nan, one]), 0, &[], None),
        (
            "a NaN on a state the condition selects",
            from(&[nan, one, zero, zero]),
            0,
            &[(1, false)],
            None,
        ),
        (
            "a NaN on a state the condition does not select",
            from(&[one, zero, nan, zero]),
            0,
            &[(1, false)],
            None,
        ),
        ("probability 1e-18", tiny, 1, &[(0, true)], Some(1.0)),
    ];
    for (scene, state, site, given, answer) in rows {
        let result = ENGINE.conditional_probability_of_one(&state, site, given);
        match answer {
            None => assert_eq!(
                result,
                Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome)),
                "{scene}"
            ),
            Some(expected) => {
                let p1 = result.expect("a condition of probability above zero is answered");
                assert!((p1 - expected).abs() < 1e-9, "{scene}: {p1}");
            }
        }
    }
}

/// Whether two states hold the same amplitudes, bit for bit, NaN included.
fn same_amplitudes(a: &ReferenceState, b: &ReferenceState) -> bool {
    let bits = |z: Complex64| (z.re.to_bits(), z.im.to_bits());
    a.site_count() == b.site_count()
        && (0..1 << a.site_count()).all(|i| bits(a.amplitude(i)) == bits(b.amplitude(i)))
}

/// The probability of the value, over the state's norm, decides a
/// projection: zero (One on |0⟩) or NaN is refused, leaving the state
/// unchanged; anything above zero is projected on, however small, such as
/// the 1e-18 that `Ry(2·10⁻⁹)` leaves One.
#[test]
fn a_projection_is_refused_exactly_when_its_probability_is_zero_or_nan() {
    let (s, k) = 1e-9f64.sin_cos();
    let mut tiny = prepared(2);
    apply(
        &mut tiny,
        &[0],
        &[c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)],
    );
    let nan = ReferenceState::from_amplitudes(&[c(f64::NAN, 0.0), c(1.0, 0.0)]);
    // (scene, state, site, value, `None` for a refusal, or the probability
    // returned and the basis state the projection leaves of amplitude 1)
    let rows = [
        ("probability zero", prepared(2), 1, true, None),
        ("a NaN", nan, 0, true, None),
        ("probability 1e-18", tiny, 0, true, Some((s * s, 0b01))),
    ];
    for (scene, mut state, site, one, projected) in rows {
        let before = state.clone();
        let result = ENGINE.project(&mut state, site, one);
        match projected {
            None => {
                assert_eq!(
                    result,
                    Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome)),
                    "{scene}"
                );
                assert!(
                    same_amplitudes(&state, &before),
                    "{scene}: the state changed"
                );
            }
            Some((expected, kept)) => {
                let p = result.expect("a probability above zero is projected on");
                assert!((p - expected).abs() < 1e-30, "{scene}: {p}");
                assert!(
                    (state.amplitude(kept) - c(1.0, 0.0)).norm() < 1e-12,
                    "{scene}"
                );
            }
        }
    }
}

/// Site 0 One with probability sin²(10⁻¹⁵⁰) (about 10⁻³⁰⁰, a normal `f64`,
/// whose amplitudes' squares are normal too), site 1 One with probability
/// sin²(0.35); the two are independent.
fn state_with_an_outcome_of_small_normal_probability() -> ReferenceState {
    let mut state = prepared(2);
    for (site, angle) in [(0, 1e-150f64), (1, 0.35)] {
        let (s, k) = angle.sin_cos();
        apply(
            &mut state,
            &[site],
            &[c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)],
        );
    }
    state
}

/// A condition of probability about 10⁻³⁰⁰, small but a normal `f64`, gives
/// the other site's conditional probability to rounding.
#[test]
fn a_condition_of_small_normal_probability_is_answered_to_rounding() {
    let state = state_with_an_outcome_of_small_normal_probability();
    let p1 = ENGINE
        .conditional_probability_of_one(&state, 1, &[(0, true)])
        .expect("a condition of probability above zero is answered");
    let expected = 0.35f64.sin().powi(2);
    assert!((p1 - expected).abs() < 1e-12, "{p1}, not {expected}");
}

/// A projection on an outcome of probability about 10⁻³⁰⁰, small but a
/// normal `f64`, returns that probability to rounding and keeps the
/// state's weight.
#[test]
fn a_projection_on_an_outcome_of_small_normal_probability_returns_it_and_keeps_the_weight() {
    let mut state = state_with_an_outcome_of_small_normal_probability();
    let p = ENGINE
        .project(&mut state, 0, true)
        .expect("an outcome of probability above zero is projected on");
    let expected = 1e-150f64.sin().powi(2);
    assert!(
        ((p - expected) / expected).abs() < 1e-12,
        "the probability returned: {p}, not {expected}"
    );
    let weight = ENGINE.retained_weight(&state).expect("a read");
    assert!(
        (weight - 1.0).abs() < 1e-12,
        "the weight kept: {weight}, not 1"
    );
}

/// On a state of weight 0.64, such as a truncation leaves, One has
/// probability 0.4096 / 0.64 = 0.64 in the normalized state, and the
/// projected state keeps the weight 0.64.
#[test]
fn a_projection_keeps_the_weight_of_a_state_not_normalized() {
    let mut state = ReferenceState::from_amplitudes(&[c(0.48, 0.0), c(0.64, 0.0)]);
    let p = ENGINE
        .project(&mut state, 0, true)
        .expect("One has probability above zero");
    assert!((p - 0.64).abs() < 1e-12, "{p}");
    let weight = ENGINE.retained_weight(&state).expect("a read");
    assert!((weight - 0.64).abs() < 1e-12, "{weight}");
}

#[test]
fn the_retained_weight_stays_one_under_gates_and_projections() {
    let mut state = bell(3);
    apply(&mut state, &[2], &hadamard());
    apply(&mut state, &[2, 0], &cx());
    ENGINE
        .project(&mut state, 2, true)
        .expect("One has probability above zero");
    let weight = ENGINE.retained_weight(&state).expect("a read");
    assert!((weight - 1.0).abs() < 1e-12, "{weight}");
}

/// Site 0 in `Ry(0.7)|0⟩`, site 1 in One, then `diag(1, 1 + 4e-10)`
/// sixteen times on site 1: each deviates from unitary by
/// (1 + 4e-10)² − 1 ≈ 8e-10, within the tolerance of 1e-9, so `apply`
/// accepts it, yet together they scale ⟨ψ|ψ⟩ by (1 + 4e-10)³² ≈
/// 1 + 1.3e-8, while the normalized state stays the same.
fn state_whose_norm_drifted() -> ReferenceState {
    let (s, k) = 0.35f64.sin_cos();
    let mut state = prepared(2);
    apply(
        &mut state,
        &[0],
        &[c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)],
    );
    apply(&mut state, &[1], &Pauli::X.matrix());
    let grows = [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(1.0 + 4e-10, 0.0)];
    for _ in 0..16 {
        apply(&mut state, &[1], &grows);
    }
    let weight = ENGINE.retained_weight(&state).expect("a read");
    assert!(weight > 1.0 + 1e-8, "the gates raise the norm: {weight}");
    state
}

/// The reads are of the normalized state though ⟨ψ|ψ⟩ is about
/// 1 + 1.3e-8: P(One) of the site the gates grew is still 1, and that of
/// its partner still sin²(0.35); ⟨2 + Z₀ + Z₁⟩ is 2 + cos(0.7) − 1, with
/// ⟨Z₀⟩ = 1 − 2 sin²(0.35) = cos(0.7) and ⟨Z₁⟩ = −1, its Pauli terms of
/// the normalized state and the identity's coefficient added as it is.
#[test]
fn the_reads_are_of_the_normalized_state_when_gates_drift_the_norm() {
    let state = state_whose_norm_drifted();
    let probability_of_one = |site| {
        let p = ENGINE
            .probability_of_one(&state, site)
            .expect("the site is in range");
        c(p, 0.0)
    };
    let one = c(1.0, 0.0);
    let sum = observable(
        c(2.0, 0.0),
        &[(one, &[(0, Pauli::Z)]), (one, &[(1, Pauli::Z)])],
    );
    let rows = [
        (
            "probability_of_one, site 0",
            probability_of_one(0),
            c(0.35f64.sin().powi(2), 0.0),
        ),
        ("probability_of_one, site 1", probability_of_one(1), one),
        (
            "expectation",
            expectation(&state, &sum),
            c(2.0 + 0.7f64.cos() - 1.0, 0.0),
        ),
    ];
    for (read, actual, expected) in rows {
        assert!(
            (actual - expected).norm() < 1e-12,
            "{read}: {actual}, not {expected}"
        );
    }
}

#[test]
fn a_fork_is_independent_of_its_original() {
    let state = bell(2);
    let before = state.clone();
    let mut fork = ENGINE.fork(&state).expect("the reference forks");
    assert_eq!(fork, state);
    apply(&mut fork, &[1], &Pauli::X.matrix());
    assert_eq!(state, before);
}

/// A Bell pair's first qubit, of probability one half, reads One in
/// about half of 1000 shots.
#[test]
fn samples_follow_the_probabilities() {
    let values = ENGINE
        .sample(&bell(2), &[0], 1000, 7)
        .expect("a site in range");
    let ones = values.iter().filter(|&&one| one).count();
    assert!((400..=600).contains(&ones), "{ones} of 1000");
}

/// `sample` asks that every bit of a seed select the values: seeds that
/// differ in their lowest bit, or in their highest, give other values. An
/// engine that keeps only some bits of the seed (its low 32, or all but
/// the lowest) fails one of the pairs.
#[test]
fn seeds_that_differ_in_one_bit_give_other_samples() {
    let state = bell(2);
    for (seed, other) in [(0, 1), (0, 1 << 63)] {
        assert_ne!(
            ENGINE.sample(&state, &[0], 64, seed),
            ENGINE.sample(&state, &[0], 64, other),
            "seeds {seed} and {other}"
        );
    }
}

/// A sample whose values no memory holds breaks no rule of the contract,
/// so the reference reports the sampler's `TooManyValues` as its own
/// failure, which a caller does not take for a refusal.
#[test]
fn a_sample_too_large_to_hold_is_a_failure_not_a_refusal() {
    let error = ENGINE
        .sample(&bell(2), &[0, 1], usize::MAX, 7)
        .expect_err("no memory holds the values");
    assert!(
        matches!(error, ReferenceError::TooManyValues(_)),
        "{error:?}"
    );
    assert_eq!(error.refusal(), None);
}

/// Zero shots are no fault, but they do not excuse a site out of range, nor
/// a repeated site, even one that does not repeat its neighbour.
#[test]
fn zero_shots_do_not_excuse_a_site_out_of_range_or_repeated() {
    let cases = [
        (vec![0, 2], Refusal::SiteOutOfRange { site: 2, sites: 2 }),
        (vec![1, 0, 1], Refusal::RepeatedSite { site: 1 }),
    ];
    for (sites, refusal) in cases {
        assert_eq!(
            ENGINE.sample(&bell(2), &sites, 0, 1),
            Err(ReferenceError::Refused(refusal))
        );
    }
}

/// Basis states past the last one, to the largest index, have amplitude
/// zero rather than a panic or a NaN.
#[test]
fn an_amplitude_past_the_last_basis_state_is_zero() {
    let state = bell(2);
    for index in [4, 5, usize::MAX] {
        assert_eq!(state.amplitude(index), c(0.0, 0.0), "index {index}");
    }
}

/// The bond at each cut is the Schmidt rank there, as a singular value
/// decomposition gives it too: on a five-site GHZ state, 2 at every cut;
/// and on √0.5|0000⟩ + i√0.3|0101⟩ + e^{0.7i}√0.2|1010⟩ (basis index `i`,
/// site `k` as bit `k`), set by its amplitudes: at the middle cut its
/// three basis states have three different halves on each side, so rank
/// 3; at the other cuts the lone site on one side takes only two values,
/// so rank 2; [2, 3, 2], a rank of three that random gates almost never
/// give.
#[test]
fn the_exact_chain_has_the_schmidt_rank_at_each_cut() {
    let mut ghz = prepared(5);
    apply(&mut ghz, &[0], &hadamard());
    for s in 0..4 {
        apply(&mut ghz, &[s, s + 1], &cx());
    }
    let mut amplitudes = vec![c(0.0, 0.0); 16];
    amplitudes[0b0000] = c(0.5f64.sqrt(), 0.0);
    amplitudes[0b0101] = c(0.0, 0.3f64.sqrt());
    amplitudes[0b1010] = Complex64::from_polar(0.2f64.sqrt(), 0.7);
    let rank_three = ReferenceState::from_amplitudes(&amplitudes);
    let rows: [(&str, ReferenceState, &[usize]); 2] = [
        ("GHZ", ghz, &[2, 2, 2, 2]),
        ("a rank of three", rank_three, &[2, 3, 2]),
    ];
    for (scene, state, ranks) in rows {
        let shape = ENGINE
            .shape(&state)
            .expect("the reference reports its chain");
        let cuts = 0..state.site_count() - 1;
        let bonds: Vec<Option<usize>> = cuts.clone().map(|cut| shape.bond_dimension(cut)).collect();
        let expected: Vec<Option<usize>> = ranks.iter().copied().map(Some).collect();
        assert_eq!(bonds, expected, "{scene}");
        let by_svd: Vec<Option<usize>> = cuts.map(|cut| Some(rank_by_svd(&state, cut))).collect();
        assert_eq!(bonds, by_svd, "{scene}: by a singular value decomposition");
    }
}

/// Two Bell pairs across the middle cut of four sites: rank 4 there,
/// reported though the capacity's bond is 2, since the reference never
/// truncates.
#[test]
fn the_reference_reports_a_chain_that_outgrows_its_capacity() {
    let capacity = MpsShape::qubit_chain(4, 2).expect("four sites form a chain");
    let mut pairs = ENGINE.prepare(&capacity).expect("four sites are held");
    for (a, b) in [(0, 2), (1, 3)] {
        apply(&mut pairs, &[a], &hadamard());
        apply(&mut pairs, &[a, b], &cx());
    }
    let shape = ENGINE
        .shape(&pairs)
        .expect("the reference reports its chain");
    assert_eq!(shape.bond_dimension(1), Some(4));
    assert!(!shape.fits_within(&capacity));
}

/// A rank that an elimination always along the rows (or always along
/// the columns) would miss: 3 sites, cut 0 has 2 rows and 4 columns,
/// cut 1 has 4 rows and 2 columns.
#[test]
fn the_schmidt_rank_is_found_along_either_side_of_a_cut() {
    let mut state = prepared(3);
    apply(&mut state, &[0], &hadamard());
    apply(&mut state, &[0, 2], &cx());
    let shape = ENGINE
        .shape(&state)
        .expect("the reference reports its chain");
    assert_eq!(
        (shape.bond_dimension(0), shape.bond_dimension(1)),
        (Some(2), Some(2))
    );
}

/// The product state of site 0 in 0.6|0⟩ + 0.8|1⟩ and site 1 in
/// 0.8|0⟩ + 0.6|1⟩, with `delta` added to the amplitude of |11⟩. As a
/// matrix (site 0's value the row, site 1's the column) it is
/// [[0.48, 0.36], [0.64, 0.48 + delta]]: Gaussian elimination pivots on
/// 0.64 and, with 0.48/0.64 = 0.75, leaves a second pivot of
/// 0.36 − 0.75 (0.48 + delta) = −0.75 `delta`, so a row that wants a
/// pivot p passes `delta` = p / 0.75.
fn product_state_off_by(delta: f64) -> ReferenceState {
    let (a, b) = ([0.6, 0.8], [0.8, 0.6]);
    let amplitudes: Vec<Complex64> = (0..4)
        .map(|i: usize| c(a[i & 1] * b[i >> 1] + if i == 3 { delta } else { 0.0 }, 0.0))
        .collect();
    ReferenceState::from_amplitudes(&amplitudes)
}

/// The rank at a cut counts the pivots elimination leaves at or above the
/// threshold of 1e-10. The rows:
///
/// - a controlled rotation by 1e-6 entangles its sites slightly; its
///   smaller Schmidt value, sin(0.5e-6)/2 ≈ 2.5e-7, is far above rounding,
///   so it counts;
/// - on the product state off by `delta`, a pivot of rounding's size
///   (about 1e-12, as gates that entangle then disentangle leave) and one
///   of 0.9e-10, just below the threshold, count as zero; one of 1.1e-10,
///   just above, counts;
/// - amplitudes 1 on |00⟩ and 1e-10 on |11⟩ leave a pivot of exactly
///   1e-10, which counts: only a pivot below the threshold is zero.
#[test]
fn the_rank_counts_the_pivots_at_or_above_its_threshold() {
    let (cos, sin) = ((0.5e-6_f64).cos(), (0.5e-6_f64).sin());
    let (o, l) = (c(0.0, 0.0), c(1.0, 0.0));
    let controlled_ry = [
        l,
        o,
        o,
        o, //
        o,
        l,
        o,
        o, //
        o,
        o,
        c(cos, 0.0),
        c(-sin, 0.0), //
        o,
        o,
        c(sin, 0.0),
        c(cos, 0.0),
    ];
    let mut slightly_entangled = prepared(2);
    apply(&mut slightly_entangled, &[0], &hadamard());
    apply(&mut slightly_entangled, &[0, 1], &controlled_ry);
    let rows = [
        ("a slight entanglement", slightly_entangled, 2),
        ("a pivot of about 1e-12", product_state_off_by(1e-12), 1),
        (
            "a pivot of 0.9e-10",
            product_state_off_by(0.9e-10 / 0.75),
            1,
        ),
        (
            "a pivot of 1.1e-10",
            product_state_off_by(1.1e-10 / 0.75),
            2,
        ),
        (
            "a pivot of exactly 1e-10",
            ReferenceState::from_amplitudes(&[l, o, o, c(1e-10, 0.0)]),
            2,
        ),
    ];
    for (scene, state, rank) in rows {
        let shape = ENGINE
            .shape(&state)
            .expect("the reference reports its chain");
        assert_eq!(shape.bond_dimension(0), Some(rank), "{scene}");
    }
}

/// A state whose amplitudes are all zero has rank 0 at its cut, and the
/// chain still has a bond of 1, the least a bond can be.
#[test]
fn a_bond_is_at_least_one_where_the_schmidt_rank_is_zero() {
    let state = ReferenceState::from_amplitudes(&[c(0.0, 0.0); 4]);
    let shape = ENGINE
        .shape(&state)
        .expect("the reference reports its chain");
    assert_eq!(shape.bond_dimension(0), Some(1));
}

/// A gate of a random circuit: one-site gates on a site, and controlled
/// gates on a control and a target.
#[derive(Clone, Copy, Debug)]
enum Gate {
    H(usize),
    S(usize),
    T(usize),
    X(usize),
    Rx(f64, usize),
    Ry(f64, usize),
    Rz(f64, usize),
    Cx(usize, usize),
    Cz(usize, usize),
}

impl Gate {
    /// The gate's sites, the control first, and its matrix, row-major with
    /// the first site the most significant bit; rotations by `θ` are
    /// `exp(−iθP/2)`.
    fn on_sites(self) -> (Vec<usize>, Vec<Complex64>) {
        let (o, l, i) = (c(0.0, 0.0), c(1.0, 0.0), c(0.0, 1.0));
        let half = |theta: f64| (theta / 2.0).sin_cos();
        match self {
            Self::H(q) => (vec![q], hadamard().to_vec()),
            Self::S(q) => (vec![q], vec![l, o, o, i]),
            Self::T(q) => (
                vec![q],
                vec![l, o, o, Complex64::from_polar(1.0, FRAC_PI_4)],
            ),
            Self::X(q) => (vec![q], vec![o, l, l, o]),
            Self::Rx(theta, q) => {
                let (sin, cos) = half(theta);
                (
                    vec![q],
                    vec![c(cos, 0.0), c(0.0, -sin), c(0.0, -sin), c(cos, 0.0)],
                )
            }
            Self::Ry(theta, q) => {
                let (sin, cos) = half(theta);
                (
                    vec![q],
                    vec![c(cos, 0.0), c(-sin, 0.0), c(sin, 0.0), c(cos, 0.0)],
                )
            }
            Self::Rz(theta, q) => {
                let phase = |sign: f64| Complex64::from_polar(1.0, sign * theta / 2.0);
                (vec![q], vec![phase(-1.0), o, o, phase(1.0)])
            }
            Self::Cx(control, target) => (vec![control, target], cx().to_vec()),
            Self::Cz(control, target) => {
                let mut cz = vec![o; 16];
                for (k, value) in [l, l, l, -l].into_iter().enumerate() {
                    cz[5 * k] = value;
                }
                (vec![control, target], cz)
            }
        }
    }
}

/// A `SplitMix64` stream, so that the random circuits are the same on
/// every run.
struct Stream(SplitMix64);

impl Stream {
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.0.next_u64() % n as u64).expect("below a usize")
    }

    /// An angle in [0, 2π).
    fn angle(&mut self) -> f64 {
        self.0.next_uniform() * std::f64::consts::TAU
    }

    /// Two distinct sites of `sites`, in either order, neighbours or not.
    fn pair(&mut self, sites: usize) -> (usize, usize) {
        let first = self.below(sites);
        let second = (first + 1 + self.below(sites - 1)) % sites;
        (first, second)
    }
}

/// A random circuit of `one_site` one-site gates on `sites` sites, with
/// `entangling` controlled gates among them at random places.
fn random_circuit(
    stream: &mut Stream,
    sites: usize,
    one_site: usize,
    entangling: usize,
) -> Vec<Gate> {
    let mut gates: Vec<Gate> = (0..one_site)
        .map(|_| {
            let q = stream.below(sites);
            match stream.below(7) {
                0 => Gate::H(q),
                1 => Gate::S(q),
                2 => Gate::T(q),
                3 => Gate::X(q),
                4 => Gate::Rx(stream.angle(), q),
                5 => Gate::Ry(stream.angle(), q),
                _ => Gate::Rz(stream.angle(), q),
            }
        })
        .collect();
    for _ in 0..entangling {
        let (control, target) = stream.pair(sites);
        let gate = if stream.below(2) == 0 {
            Gate::Cx(control, target)
        } else {
            Gate::Cz(control, target)
        };
        let at = stream.below(gates.len() + 1);
        gates.insert(at, gate);
    }
    gates
}

fn reference_of(sites: usize, circuit: &[Gate]) -> ReferenceState {
    let mut state = prepared(sites);
    for gate in circuit {
        let (on, matrix) = gate.on_sites();
        apply(&mut state, &on, &matrix);
    }
    state
}

/// The Schmidt rank at `cut` of `state`, by a singular value decomposition
/// of its amplitudes split at the cut: the singular values above 1e-10.
/// The test that calls it checks that none lies near that threshold.
fn rank_by_svd(state: &ReferenceState, cut: usize) -> usize {
    let left = 1 << (cut + 1);
    let right = 1 << (state.site_count() - cut - 1);
    let matrix = DMatrix::from_fn(left, right, |l, r| state.amplitude(l | r << (cut + 1)));
    let values = matrix.svd(false, false).singular_values;
    assert!(
        values.iter().all(|&value| !(1e-13..1e-6).contains(&value)),
        "a singular value lies near the rank threshold: {values}"
    );
    values.iter().filter(|&&value| value > 1e-10).count()
}

/// The reference computes its bonds as Schmidt ranks by elimination. At
/// every cut they equal the ranks a singular value decomposition gives, on
/// random circuits of five sites with zero to four controlled gates,
/// twenty circuits of each count from fixed seeds. The ranks seen are one,
/// two and four. A rank of three needs a tuned state, which random gates
/// almost never give; `the_exact_chain_has_the_schmidt_rank_at_each_cut`
/// has one.
#[test]
fn the_schmidt_ranks_are_those_a_singular_value_decomposition_gives() {
    const SITES: usize = 5;
    let mut stream = Stream(SplitMix64::new(0x5EED_F5A1));
    let mut seen = [false; 5];
    for entangling in 0..=4 {
        for round in 0..20 {
            let circuit = random_circuit(&mut stream, SITES, 20, entangling);
            let state = reference_of(SITES, &circuit);
            let shape = ENGINE
                .shape(&state)
                .expect("the reference reports its chain");
            for cut in 0..SITES - 1 {
                let rank = rank_by_svd(&state, cut);
                seen[rank] = true;
                assert_eq!(
                    shape.bond_dimension(cut),
                    Some(rank),
                    "cut {cut} of circuit {round} of {entangling} controlled gates: {circuit:?}"
                );
            }
        }
    }
    assert!(seen[1] && seen[2] && seen[4], "the ranks seen: {seen:?}");
}
