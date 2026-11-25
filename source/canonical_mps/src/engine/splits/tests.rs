// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the engine's splits discard, and the weight a truncated state
//! keeps.
//!
//! - A split discards the tail below the engine's cutoff, a fraction of the
//!   split's own weight, which leaves the retained weight. It also discards
//!   what exceeds the capacity's bond.
//! - A fork keeps its parent's retained weight.
//! - Reads keep their value however little weight many truncations kept.
//! - The retained weight fails below the smallest normal `f64`, and a split
//!   that would take it there fails the operation that applies it.
//!
//! The CPU algebra's rule of truncation itself is
//! `Truncation`'s (`src/cpu_algebra/gate_split/truncation.rs`),
//! tested through its splits beside it.

mod fixed_fraction_fake_algebra;

use super::retained_weight_after;
use crate::engine::conformance::{capacity, cnot, entangler, real, ry};
use crate::engine::tests::both_fusions;
use crate::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, CpuAlgebra, CpuMpsEngine as Engine,
    DEFAULT_CUTOFF,
};
use dense_linalg::SvdError;
use fixed_fraction_fake_algebra::FixedFractionFakeAlgebra;
use tensornet::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm};

/// Entangling gates, distant ones included, on a chain whose bond is 2:
/// once each call returns, every bond is at most the capacity's, and the
/// weight the splits discard shows in the retained weight.
#[test]
fn truncates_to_the_capacity() {
    let capacity = capacity(6, 2);
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity).expect("prepare");
        for site in 0..6 {
            engine.apply(&mut state, &[site], &ry(1.1)).expect("apply");
        }
        for (a, b) in [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (4, 5),
            (0, 5),
            (2, 4),
            (1, 3),
        ] {
            engine
                .apply(&mut state, &[a, b], &entangler(1.2))
                .expect("apply");
        }
        let shape = engine.shape(&state).expect("shape");
        assert!(shape.fits_within(&capacity), "{name}: {shape:?}");
        assert_eq!(shape.largest_bond_dimension(), 2, "{name}");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!(weight < 1.0 - 1e-6 && weight > 0.0, "{name}: {weight}");
    }
}

/// Schmidt weights between two sites: the engine's cutoff, the default or
/// one set `with_cutoff`, reaches the gate's split, and the weight it drops
/// leaves the retained weight. Each row: the smaller weight (the tail), the
/// cutoff (`None` for an engine taken by `new`), and the bond and retained
/// weight expected.
///
/// - An engine taken by `new`: tails of 0.9 and 1.1 times `DEFAULT_CUTOFF`,
///   on either side of it; the split drops the first and keeps the second.
/// - With weights 0.8 and 0.2, a cutoff of 0.25 set `with_cutoff` keeps one
///   value and a weight of 0.8.
///
/// Where the tail lies against other cutoffs is `Truncation`'s rule,
/// tested beside it.
#[test]
fn the_engines_cutoff_drops_a_splits_tail_from_the_retained_weight() {
    let rows = [
        (0.9 * DEFAULT_CUTOFF, None, 1, 1.0 - 0.9 * DEFAULT_CUTOFF),
        (1.1 * DEFAULT_CUTOFF, None, 2, 1.0),
        (0.2f64, Some(0.25), 1, 0.8),
    ];
    for (tail, cutoff, bond, weight) in rows {
        let theta = 2.0 * tail.sqrt().asin();
        let engine = cutoff.map_or_else(
            || Engine::new(CpuAlgebra),
            |cutoff| Engine::new(CpuAlgebra).with_cutoff(cutoff),
        );
        for (engine, name) in [
            (engine.clone(), "fused"),
            (engine.with_fusion(false), "unfused"),
        ] {
            let case = format!("{name}, tail {tail:e}, cutoff {cutoff:?}");
            let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
            engine.apply(&mut state, &[0], &ry(theta)).expect("apply");
            engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
            assert_eq!(
                engine
                    .shape(&state)
                    .expect("shape")
                    .largest_bond_dimension(),
                bond,
                "{case}"
            );
            let kept = engine.retained_weight(&state).expect("weight");
            assert!((kept - weight).abs() < 1e-12, "{case}: {kept} vs {weight}");
        }
    }
}

/// The cutoff is a fraction of the split's own weight, neither of the
/// exact state's nor of the weight the state kept. A state that kept 0.6
/// of its weight through a bond of 1, then a pair whose smaller Schmidt
/// weight is a tail of what is left, at a cutoff of 1e-3. Each row: the
/// tail and the bond.
///
/// - A tail of 1.2e-3 is kept, a bond of 2. It is above the cutoff as a
///   fraction of the split's weight, though below it as a fraction of the
///   exact state's (7.2e-4).
/// - A tail of 8e-4 is dropped, a bond of 1. A cutoff scaled by the weight
///   kept, 6e-4, would keep it.
#[test]
fn the_cutoff_is_a_fraction_of_the_splits_own_weight() {
    let capacity = MpsShape::new(vec![vec![2, 1], vec![1, 2, 2], vec![2, 2]]).expect("a shape");
    let first = 2.0 * 0.6f64.sqrt().acos();
    for (tail, bond) in [(1.2e-3f64, 2), (8e-4, 1)] {
        let second = 2.0 * tail.sqrt().asin();
        for (engine, name) in both_fusions(1e-3) {
            let mut state = engine.prepare(&capacity).expect("prepare");
            engine.apply(&mut state, &[0], &ry(first)).expect("apply");
            engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
            let weight = engine.retained_weight(&state).expect("weight");
            assert!((weight - 0.6).abs() < 1e-12, "{name}: {weight}");
            engine.apply(&mut state, &[2], &ry(second)).expect("apply");
            engine.apply(&mut state, &[2, 1], &cnot()).expect("apply");
            let shape = engine.shape(&state).expect("shape");
            assert_eq!(shape.bond_dimension(1), Some(bond), "{name}, {tail:e}");
        }
    }
}

/// A cutoff is a fraction of a split's weight: one below 0, of 1 or more,
/// or NaN is a caller's error, and the engine panics on it, while both
/// zeros and the largest value below 1 are accepted.
#[test]
fn a_cutoff_must_lie_in_zero_to_one() {
    for cutoff in [0.0, -0.0, f64::next_down(1.0)] {
        let _ = Engine::new(CpuAlgebra).with_cutoff(cutoff);
    }
    for cutoff in [-1e-300, 1.0, 2.0, f64::INFINITY, f64::NAN] {
        let payload = std::panic::catch_unwind(|| Engine::new(CpuAlgebra).with_cutoff(cutoff))
            .expect_err("a cutoff outside [0, 1)");
        let message = payload
            .downcast_ref::<String>()
            .expect("a formatted panic message");
        assert!(message.contains("in [0, 1)"), "{cutoff}: {message}");
    }
}

/// Grouped shots fork a state whose pair was truncated to a bond of 1,
/// keeping half the weight: the fork's retained weight is its parent's,
/// 0.5, as a fork is the same state.
#[test]
fn a_fork_keeps_its_parents_retained_weight() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
        engine
            .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
            .expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        let fork = engine.fork(&state).expect("fork");
        let weight = engine.retained_weight(&fork).expect("weight");
        assert!((weight - 0.5).abs() < 1e-12, "{name}: {weight}");
    }
}

/// A backend whose split reports keeping 2⁻⁶⁰⁰ or 2⁻¹⁰²⁰ of the weight, as
/// a long run of heavy truncations would, just above the smallest normal
/// `f64` for the second. The reads keep their value however little weight
/// the splits kept: the retained weight is kept apart from the state, whose
/// norm stays about 1. One row per read.
#[test]
fn reads_after_many_truncations_keep_their_value() {
    type Row = fn(&FixedFractionEngine, &str);
    let rows: [(&str, f64, Row); 2] = [
        // A CNOT on |00⟩, its split leaving the centre on the left site,
        // then on the right, then a turn of site 0 to a P(One) of 1e-200:
        // the read gives 1e-200, and a projection on it is accepted.
        (
            "a rare value's probability",
            2f64.powi(-600),
            |engine, name| {
                for centre in [0, 1] {
                    let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
                    engine.probability_of_one(&state, centre).expect("P(One)");
                    let weight = split_and_read_the_weight(engine, &mut state);
                    assert_eq!(weight, Ok(2f64.powi(-600)), "{name}, {centre}");
                    engine.apply(&mut state, &[0], &ry(2e-100)).expect("apply");
                    let p = engine.probability_of_one(&state, 0).expect("P(One)");
                    assert!((p / 1e-200 - 1.0).abs() < 1e-9, "{name}, {centre}: {p:e}");
                    let projected = engine.project(&mut state, 0, true);
                    assert!(projected.is_ok(), "{name}, {centre}: {projected:?}");
                }
            },
        ),
        // A Bell pair truncated to a bond of 1, then ⟨1000·Z₀⟩: the read
        // gives ±1000.
        ("an expectation", 2f64.powi(-1020), |engine, name| {
            let z = PauliSum::new(
                real(0.0),
                vec![PauliTerm::new(real(1000.0), vec![(0, Pauli::Z)]).expect("a term")],
            )
            .expect("an observable");
            let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
            engine
                .apply(&mut state, &[0], &ry(std::f64::consts::FRAC_PI_2))
                .expect("apply");
            let weight = split_and_read_the_weight(engine, &mut state);
            assert_eq!(weight, Ok(2f64.powi(-1020)), "{name}");
            let value = engine.expectation(&state, &z).expect("expectation");
            assert!((value.re.abs() - 1000.0).abs() < 1e-6, "{name}: {value}");
        }),
    ];
    for (row, fraction, check) in rows {
        for (engine, name) in both_fusions_of_a_fixed_fraction(fraction) {
            check(&engine, &format!("{row}, {name}"));
        }
    }
}

/// A split's retained fraction times the state's weight, at the smallest
/// normal `f64` and on either side of it. The product is the new weight
/// when it is at least the smallest normal, and `WeightUnderflow`
/// otherwise, as floats lose digits below it. Each row: the weight, the
/// fraction, and the result. Every product is exact.
#[test]
fn the_retained_weight_fails_below_the_smallest_normal() {
    let smallest = f64::MIN_POSITIVE;
    let underflow = Err(CanonicalMpsError::WeightUnderflow);
    let rows: [(f64, f64, Result<f64, CanonicalMpsError<SvdError>>); 6] = [
        // A Bell pair truncated to a bond of 1 halves the weight.
        (0.5, 0.5, Ok(0.25)),
        // At the smallest normal.
        (1.0, smallest, Ok(smallest)),
        (2.0 * smallest, 0.5, Ok(smallest)),
        // Just above it.
        (1.0, smallest.next_up(), Ok(smallest.next_up())),
        // Just below it.
        (1.0, smallest.next_down(), underflow.clone()),
        (smallest, 0.5, underflow),
    ];
    for (weight, fraction, expected) in rows {
        assert_eq!(
            retained_weight_after(weight, fraction),
            expected,
            "{weight:e} × {fraction:e}"
        );
    }
}

/// An engine on [`FixedFractionFakeAlgebra`].
type FixedFractionEngine = CanonicalMpsEngine<FixedFractionFakeAlgebra>;

/// The engine on a backend whose every split reports keeping `fraction`,
/// with and without fusion, each with its setting's name.
fn both_fusions_of_a_fixed_fraction(fraction: f64) -> [(FixedFractionEngine, &'static str); 2] {
    let engine = CanonicalMpsEngine::new(FixedFractionFakeAlgebra(fraction));
    [
        (engine.clone(), "fused"),
        (engine.with_fusion(false), "unfused"),
    ]
}

/// One split on a pair, then a read of the retained weight, which applies
/// a window left pending.
fn split_and_read_the_weight(
    engine: &FixedFractionEngine,
    state: &mut CanonicalMpsState<FixedFractionFakeAlgebra>,
) -> Result<f64, CanonicalMpsError<SvdError>> {
    engine.apply(state, &[0, 1], &cnot())?;
    engine.retained_weight(state)
}

/// A backend whose every split reports keeping 2⁻⁶⁰⁰ of the weight, as a
/// long run of heavy truncations would. After one CNOT, the retained
/// weight reads 2⁻⁶⁰⁰. A second CNOT's split would take it to 2⁻¹²⁰⁰,
/// below the smallest normal `f64`. So the operation that applies that
/// split fails with `WeightUnderflow`: the gate unfused, the read of the
/// weight fused.
#[test]
fn a_split_that_would_underflow_the_retained_weight_fails_its_operation() {
    let fraction = 2f64.powi(-600);
    for (engine, name) in both_fusions_of_a_fixed_fraction(fraction) {
        let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
        let first = split_and_read_the_weight(&engine, &mut state);
        assert_eq!(first, Ok(fraction), "{name}: precondition, one split");
        let second = split_and_read_the_weight(&engine, &mut state);
        assert_eq!(second, Err(CanonicalMpsError::WeightUnderflow), "{name}");
    }
}
