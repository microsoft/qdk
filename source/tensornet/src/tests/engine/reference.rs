// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The exact reference's promises, each pinned by a test: its layout of
//! basis states and operators, its refusals and failures, its exact reads
//! (probabilities, conditionals, expectations, projections) down to the
//! precision its rustdoc states, and its chain, whose bonds are the
//! Schmidt ranks with the threshold its rustdoc states.
//!
//! These tests matter beyond the reference itself: every other engine is
//! validated by comparing its answers with the reference's, so a fault in
//! the reference would let a wrong engine pass, or fail a right one. That
//! is why the reference is also checked against implementations
//! independent of it: its amplitudes against the QDK's sparse state
//! simulator, and its ranks against a singular value decomposition, on
//! random circuits.

use crate::reference::{ExactReference, ReferenceError, ReferenceState};
use crate::sampling::splitmix64::SplitMix64;
use crate::tests::{c, cx, hadamard};
use crate::{MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, PauliTerm, Refusal};
use nalgebra::DMatrix;
use num_complex::Complex64;
use qdk_simulators::SparseStateSim;
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

#[test]
fn a_prepared_state_is_a_product_chain() {
    let shape = ENGINE
        .shape(&prepared(3))
        .expect("the reference reports its chain");
    assert_eq!(
        shape,
        MpsShape::qubit_chain(3, 1).expect("three sites form a chain")
    );
}

/// A one-qubit program's state.
#[test]
fn a_lone_site_is_prepared_as_a_lone_qubit() {
    let shape = ENGINE
        .shape(&prepared(1))
        .expect("the reference reports its chain");
    assert_eq!(
        shape,
        MpsShape::new(vec![vec![2]]).expect("a lone qubit is a chain")
    );
}

#[test]
fn basis_state_i_has_site_k_as_bit_k() {
    let mut state = prepared(3);
    apply(&mut state, &[1], &Pauli::X.matrix());
    assert_eq!(state.amplitude(0b010), c(1.0, 0.0));
}

/// CX on [0, 1] flips site 1 when site 0 is set; on [1, 0], the reverse.
#[test]
fn the_first_site_of_an_operator_is_its_most_significant_bit() {
    let mut state = prepared(2);
    apply(&mut state, &[0], &Pauli::X.matrix());
    apply(&mut state, &[0, 1], &cx());
    assert_eq!(state.amplitude(0b11), c(1.0, 0.0));
    apply(&mut state, &[1, 0], &cx());
    assert_eq!(state.amplitude(0b10), c(1.0, 0.0));
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
fn a_refusal_displays_as_itself() {
    let error = ReferenceError::from(Refusal::NoSites);
    assert_eq!(error.to_string(), Refusal::NoSites.to_string());
}

#[test]
fn a_refused_apply_leaves_the_state_unchanged() {
    let mut state = bell(2);
    let before = state.clone();
    let projector = [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)];
    let refused = ENGINE.apply(&mut state, &[1], &projector);
    assert_eq!(
        refused.map_err(|e| e.refusal().cloned()),
        Err(Some(Refusal::NotUnitary { deviation: 1.0 }))
    );
    assert_eq!(state, before);
}

/// The identity on three sites, unitary and of the right length, as a
/// method would pass a Toffoli it did not decompose: refused as too many
/// sites, through the contract's check, with the state unchanged.
#[test]
fn an_operator_on_three_sites_is_refused_and_leaves_the_state_unchanged() {
    let mut state = bell(3);
    let before = state.clone();
    let identity: Vec<Complex64> = (0..64)
        .map(|at| c(if at % 9 == 0 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    let refused = ENGINE.apply(&mut state, &[0, 1, 2], &identity);
    assert_eq!(
        refused.map_err(|e| e.refusal().cloned()),
        Err(Some(Refusal::TooManySites { sites: 3 }))
    );
    assert_eq!(state, before);
}

#[test]
fn the_probability_of_one_of_a_bell_site_is_one_half() {
    let p1 = ENGINE
        .probability_of_one(&bell(2), 1)
        .expect("site 1 exists");
    assert!((p1 - 0.5).abs() < 1e-12, "{p1}");
}

#[test]
fn a_bell_site_given_its_partner_is_its_partner() {
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

#[test]
fn a_condition_of_probability_zero_is_refused() {
    assert_eq!(
        ENGINE.conditional_probability_of_one(&bell(3), 0, &[(2, true)]),
        Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
    );
}

/// On a Bell pair ⟨XX⟩ = ⟨ZZ⟩ = 1, ⟨YY⟩ = −1 and ⟨Z₀⟩ = 0; the identity's
/// coefficient adds, and each term's multiplies.
#[test]
fn the_expectation_of_a_pauli_sum_on_a_bell_pair() {
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
fn the_expectation_of_three_y_factors_on_a_phased_ghz_state() {
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

/// One never happens on |0⟩.
#[test]
fn a_projection_on_a_value_of_probability_zero_is_refused_leaving_the_state() {
    let mut state = prepared(2);
    let before = state.clone();
    assert_eq!(
        ENGINE.project(&mut state, 1, true),
        Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
    );
    assert_eq!(state, before);
}

/// Only exactly zero is refused: `Ry(2·10⁻⁹)` leaves One a probability of
/// 1e-18, which is projected on.
#[test]
fn a_projection_on_a_tiny_probability_is_accepted() {
    let (s, k) = 1e-9f64.sin_cos();
    let ry = [c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)];
    let mut state = prepared(2);
    apply(&mut state, &[0], &ry);
    let p = ENGINE
        .project(&mut state, 0, true)
        .expect("a probability above zero is projected on");
    assert!((p - s * s).abs() < 1e-30, "{p}");
    assert!((state.amplitude(0b01) - c(1.0, 0.0)).norm() < 1e-12);
}

/// Only a condition of probability exactly zero is refused: one of
/// probability 1e-18 is answered.
#[test]
fn a_condition_of_tiny_probability_is_answered() {
    let (s, k) = 1e-9f64.sin_cos();
    let ry = [c(k, 0.0), c(-s, 0.0), c(s, 0.0), c(k, 0.0)];
    let mut state = prepared(2);
    apply(&mut state, &[0], &ry);
    apply(&mut state, &[0, 1], &cx());
    let p1 = ENGINE
        .conditional_probability_of_one(&state, 1, &[(0, true)])
        .expect("a condition of probability above zero is answered");
    assert!((p1 - 1.0).abs() < 1e-9, "{p1}");
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
/// normal `f64`, returns that probability to rounding.
#[test]
fn a_projection_on_an_outcome_of_small_normal_probability_returns_it_to_rounding() {
    let mut state = state_with_an_outcome_of_small_normal_probability();
    let p = ENGINE
        .project(&mut state, 0, true)
        .expect("an outcome of probability above zero is projected on");
    let expected = 1e-150f64.sin().powi(2);
    assert!(
        ((p - expected) / expected).abs() < 1e-12,
        "{p}, not {expected}"
    );
}

/// A projection on an outcome of probability about 10⁻³⁰⁰, small but a
/// normal `f64`, keeps the state's weight.
#[test]
fn a_projection_on_an_outcome_of_small_normal_probability_keeps_the_weight() {
    let mut state = state_with_an_outcome_of_small_normal_probability();
    ENGINE
        .project(&mut state, 0, true)
        .expect("an outcome of probability above zero is projected on");
    let weight = ENGINE.retained_weight(&state).expect("a read");
    assert!((weight - 1.0).abs() < 1e-12, "{weight}");
}

/// A state whose amplitudes hold a NaN has no probability to divide by.
#[test]
fn a_projection_whose_probability_is_nan_is_refused() {
    let mut state = ReferenceState::from_amplitudes(&[c(f64::NAN, 0.0), c(1.0, 0.0)]);
    assert_eq!(
        ENGINE.project(&mut state, 0, true),
        Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
    );
}

/// A state whose amplitudes hold a NaN has no probability to condition
/// on.
#[test]
fn a_condition_whose_probability_is_nan_is_refused() {
    let state = ReferenceState::from_amplitudes(&[c(f64::NAN, 0.0), c(1.0, 0.0)]);
    assert_eq!(
        ENGINE.conditional_probability_of_one(&state, 0, &[]),
        Err(ReferenceError::Refused(Refusal::ZeroProbabilityOutcome))
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
/// sixteen times on site 1: each is within the unitarity tolerance, so
/// `apply` accepts it, yet together they raise ⟨ψ|ψ⟩ to about 1 + 1.3e-8,
/// while the normalized state stays the same.
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

/// P(One) of the site the gates grew is still 1, and that of its partner
/// still sin²(0.35), though ⟨ψ|ψ⟩ is about 1 + 1.3e-8.
#[test]
fn a_probability_is_of_the_normalized_state_when_gates_drift_the_norm() {
    let state = state_whose_norm_drifted();
    let p = |site| {
        ENGINE
            .probability_of_one(&state, site)
            .expect("the site is in range")
    };
    let expected = [0.35f64.sin().powi(2), 1.0];
    for (site, expected) in expected.into_iter().enumerate() {
        let actual = p(site);
        assert!(
            (actual - expected).abs() < 1e-12,
            "site {site}: {actual}, not {expected}"
        );
    }
}

/// ⟨2 + Z₀ + Z₁⟩ is 2 + cos(0.7) − 1, though ⟨ψ|ψ⟩ is about 1 + 1.3e-8:
/// the Pauli terms are of the normalized state, and the identity's
/// coefficient is added as it is.
#[test]
fn an_expectation_is_of_the_normalized_state_when_gates_drift_the_norm() {
    let state = state_whose_norm_drifted();
    let one = c(1.0, 0.0);
    let value = expectation(
        &state,
        &observable(
            c(2.0, 0.0),
            &[(one, &[(0, Pauli::Z)]), (one, &[(1, Pauli::Z)])],
        ),
    );
    let expected = c(2.0 + 0.7f64.cos() - 1.0, 0.0);
    assert!((value - expected).norm() < 1e-12, "{value}, not {expected}");
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

/// `values[s * sites.len() + t]` is shot `s`'s value of `sites[t]`: with
/// site 2 always Zero and the Bell pair equal, every shot reads `[b, b,
/// false]` for the sites `[1, 0, 2]`.
#[test]
fn samples_are_shot_major_in_the_order_of_the_sites() {
    let values = ENGINE
        .sample(&bell(3), &[1, 0, 2], 1000, 7)
        .expect("distinct sites in range");
    assert_eq!(values.len(), 3000);
    assert!(values.chunks_exact(3).all(|s| s[0] == s[1] && !s[2]));
}

#[test]
fn samples_follow_the_probabilities() {
    let values = ENGINE
        .sample(&bell(2), &[0], 1000, 7)
        .expect("a site in range");
    let ones = values.iter().filter(|&&one| one).count();
    assert!((400..=600).contains(&ones), "{ones} of 1000");
}

#[test]
fn another_seed_gives_other_samples() {
    let state = bell(2);
    assert_ne!(
        ENGINE.sample(&state, &[0], 64, 7),
        ENGINE.sample(&state, &[0], 64, 8)
    );
}

/// More values than a `usize` counts is a failure, not a refusal and not
/// a panic.
#[test]
fn a_sample_of_more_values_than_memory_holds_fails() {
    let error = ENGINE
        .sample(&bell(2), &[0, 1], usize::MAX, 7)
        .expect_err("no memory holds the values");
    assert_eq!(
        error,
        ReferenceError::TooManyValues {
            shots: usize::MAX,
            sites: 2
        }
    );
    assert_eq!(error.refusal(), None);
}

/// Zero shots are no fault, but they do not excuse a site out of range.
#[test]
fn zero_shots_of_a_site_out_of_range_are_refused() {
    assert_eq!(
        ENGINE.sample(&bell(2), &[0, 2], 0, 1),
        Err(ReferenceError::Refused(Refusal::SiteOutOfRange {
            site: 2,
            sites: 2
        }))
    );
}

/// Zero shots are no fault, but they do not excuse a repeated site, even
/// one that does not repeat its neighbour.
#[test]
fn zero_shots_of_a_repeated_site_are_refused() {
    assert_eq!(
        ENGINE.sample(&bell(2), &[1, 0, 1], 0, 1),
        Err(ReferenceError::Refused(Refusal::RepeatedSite { site: 1 }))
    );
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

#[test]
fn the_exact_chain_has_the_schmidt_rank_at_each_cut() {
    let mut ghz = prepared(5);
    apply(&mut ghz, &[0], &hadamard());
    for s in 0..4 {
        apply(&mut ghz, &[s, s + 1], &cx());
    }
    let shape = ENGINE.shape(&ghz).expect("the reference reports its chain");
    assert_eq!(
        (0..4).map(|cut| shape.bond_dim(cut)).collect::<Vec<_>>(),
        [Some(2); 4]
    );
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
    assert_eq!(shape.bond_dim(1), Some(4));
    assert!(!shape.fits_within(&capacity));
}

/// A rank that elimination along the shorter side could miss: 3 sites,
/// cut 0 has 2 rows and 4 columns, cut 1 has 4 rows and 2 columns.
#[test]
fn the_schmidt_rank_is_found_along_either_side_of_a_cut() {
    let mut state = prepared(3);
    apply(&mut state, &[0], &hadamard());
    apply(&mut state, &[0, 2], &cx());
    let shape = ENGINE
        .shape(&state)
        .expect("the reference reports its chain");
    assert_eq!((shape.bond_dim(0), shape.bond_dim(1)), (Some(2), Some(2)));
}

/// A controlled rotation by 1e-6 entangles its sites only slightly: the
/// smaller Schmidt value is about 2.5e-7, far above rounding, so the rank
/// at the cut is 2.
#[test]
fn a_slight_entanglement_is_counted_in_the_rank() {
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
    let mut state = prepared(2);
    apply(&mut state, &[0], &hadamard());
    apply(&mut state, &[0, 1], &controlled_ry);
    let shape = ENGINE
        .shape(&state)
        .expect("the reference reports its chain");
    assert_eq!(shape.bond_dim(0), Some(2));
}

/// The product state of site 0 in 0.6|0⟩ + 0.8|1⟩ and site 1 in
/// 0.8|0⟩ + 0.6|1⟩, with `delta` added to the amplitude of |11⟩:
/// elimination at the cut leaves a second pivot of 0.75 `delta`, and the
/// rank there counts the pivots at or above the reference's threshold of
/// 1e-10.
fn rank_of_product_state_off_by(delta: f64) -> Option<usize> {
    let (a, b) = ([0.6, 0.8], [0.8, 0.6]);
    let amplitudes: Vec<Complex64> = (0..4)
        .map(|i: usize| c(a[i & 1] * b[i >> 1] + if i == 3 { delta } else { 0.0 }, 0.0))
        .collect();
    let state = ReferenceState::from_amplitudes(&amplitudes);
    let shape = ENGINE
        .shape(&state)
        .expect("the reference reports its chain");
    shape.bond_dim(0)
}

/// A product state whose amplitudes carry an error of rounding's size, as
/// gates that entangle and then disentangle leave: the pivot it leaves is
/// about 1e-12, below 1e-10, so the rank at the cut stays 1.
#[test]
fn a_rounding_error_does_not_raise_the_rank() {
    assert_eq!(rank_of_product_state_off_by(1e-12), Some(1));
}

/// A pivot of 0.9e-10, just below the threshold of 1e-10, counts as zero.
#[test]
fn a_pivot_just_below_the_rank_threshold_does_not_raise_the_rank() {
    assert_eq!(rank_of_product_state_off_by(0.9e-10 / 0.75), Some(1));
}

/// A pivot of 1.1e-10, just above the threshold of 1e-10, counts.
#[test]
fn a_pivot_just_above_the_rank_threshold_raises_the_rank() {
    assert_eq!(rank_of_product_state_off_by(1.1e-10 / 0.75), Some(2));
}

/// Amplitudes 1 on |00⟩ and 1e-10 on |11⟩: elimination at the cut leaves a
/// pivot of exactly 1e-10, the threshold, which counts, since only a pivot
/// below it counts as zero.
#[test]
fn a_pivot_at_the_rank_threshold_raises_the_rank() {
    let state =
        ReferenceState::from_amplitudes(&[c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(1e-10, 0.0)]);
    let shape = ENGINE
        .shape(&state)
        .expect("the reference reports its chain");
    assert_eq!(shape.bond_dim(0), Some(2));
}

/// A gate of a random circuit, applied alike to the reference and to the
/// QDK's sparse simulator: one-site gates on a site, and controlled gates
/// on a control and a target.
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
    /// `exp(−iθP/2)`, as the simulator's.
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

    fn apply_to_simulator(self, simulator: &mut SparseStateSim) {
        match self {
            Self::H(q) => simulator.h(q),
            Self::S(q) => simulator.s(q),
            Self::T(q) => simulator.t(q),
            Self::X(q) => simulator.x(q),
            Self::Rx(theta, q) => simulator.rx(theta, q),
            Self::Ry(theta, q) => simulator.ry(theta, q),
            Self::Rz(theta, q) => simulator.rz(theta, q),
            Self::Cx(control, target) => simulator.mcx(&[control], target),
            Self::Cz(control, target) => simulator.mcz(&[control], target),
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

/// The reference's amplitudes are those of the QDK's sparse state
/// simulator, an independent implementation, on random circuits of five
/// sites: forty one-site gates (H, S, T, X and rotations by random angles)
/// and twelve controlled X and Z gates, on neighbours and distant sites
/// alike, in either order, twenty circuits from fixed seeds.
#[test]
fn the_amplitudes_are_the_sparse_simulator_s_on_random_circuits() {
    const SITES: usize = 5;
    let mut stream = Stream(SplitMix64::new(0x00C0_FFEE));
    for round in 0..20 {
        let circuit = random_circuit(&mut stream, SITES, 40, 12);
        let state = reference_of(SITES, &circuit);
        let mut simulator = SparseStateSim::new(None);
        for _ in 0..SITES {
            let _ = simulator.allocate();
        }
        for &gate in &circuit {
            gate.apply_to_simulator(&mut simulator);
        }
        let (entries, qubits) = simulator.get_state();
        assert_eq!(qubits, SITES, "the simulator holds every site");
        let mut amplitudes = vec![c(0.0, 0.0); 1 << SITES];
        for (index, amplitude) in entries {
            let index = index.to_u64_digits().first().copied().unwrap_or(0);
            amplitudes[usize::try_from(index).expect("five bits")] = amplitude;
        }
        for (index, expected) in amplitudes.into_iter().enumerate() {
            let actual = state.amplitude(index);
            assert!(
                (actual - expected).norm() < 1e-12,
                "circuit {round}, amplitude {index}: {actual}, not {expected}: {circuit:?}"
            );
        }
    }
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

/// The reference's chain, whose bonds it computes as Schmidt ranks by
/// elimination, has at every cut the rank a singular value decomposition
/// gives, on random circuits of five sites with zero to four controlled
/// gates, so that the ranks are one, two and four (a rank of three needs
/// a tuned state, which random gates almost never give: the next test has
/// one): twenty circuits of each count, from fixed seeds.
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
                    shape.bond_dim(cut),
                    Some(rank),
                    "cut {cut} of circuit {round} of {entangling} controlled gates: {circuit:?}"
                );
            }
        }
    }
    assert!(seen[1] && seen[2] && seen[4], "the ranks seen: {seen:?}");
}

/// √0.5|0000⟩ + i√0.3|0101⟩ + e^{0.7i}√0.2|1010⟩ (basis index `i`, site `k`
/// as bit `k`), set by its amplitudes: three basis states that differ on
/// both sides of the middle cut, two on either side of the others, so its
/// bonds are [2, 3, 2], a rank of three that random gates almost never
/// give, and a singular value decomposition gives the same.
#[test]
fn a_tuned_state_of_schmidt_rank_three_has_a_bond_of_three() {
    let mut amplitudes = vec![c(0.0, 0.0); 16];
    amplitudes[0b0000] = c(0.5f64.sqrt(), 0.0);
    amplitudes[0b0101] = c(0.0, 0.3f64.sqrt());
    amplitudes[0b1010] = Complex64::from_polar(0.2f64.sqrt(), 0.7);
    let state = ReferenceState::from_amplitudes(&amplitudes);
    let shape = ENGINE
        .shape(&state)
        .expect("the reference reports its chain");
    let bonds: Vec<Option<usize>> = (0..3).map(|cut| shape.bond_dim(cut)).collect();
    assert_eq!(bonds, [Some(2), Some(3), Some(2)]);
    let by_svd: Vec<Option<usize>> = (0..3).map(|cut| Some(rank_by_svd(&state, cut))).collect();
    assert_eq!(bonds, by_svd);
}
