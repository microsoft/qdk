// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The contract's refusals: a condition or a projection on a value of
//! probability zero or NaN is refused, leaving what reads see, while a
//! value of rounding weight or rare but possible is accepted; a gate on
//! three sites; a qutrit; and the refusal a refused call reports.

use super::*;
use crate::algebra::{GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use std::borrow::Cow;
use std::convert::Infallible;

/// Turns of 0.3 and 0.4 and back by 0.7: the site is Zero up to rounding,
/// so One keeps a weight of rounding, above zero.
fn one_of_rounding_weight(engine: &Engine) -> State {
    let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
    for angle in [0.3, 0.4, -0.7] {
        engine.apply(&mut state, &[0], &ry(angle)).expect("apply");
    }
    engine.apply(&mut state, &[1], &ry(0.8)).expect("apply");
    state
}

/// A condition on |00⟩'s One: its probability is exactly zero.
#[test]
fn refuses_a_condition_of_probability_zero() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let state = engine.prepare(&chain(2, 2)).expect("prepare");
        let error = engine
            .conditional_probability_of_one(&state, 1, &[(0, true)])
            .expect_err("an impossible condition");
        assert_eq!(error, Refusal::ZeroProbabilityOutcome.into(), "{name}");
    }
}

/// A value of rounding weight is not refused as a condition: the contract
/// (`Refusal::ZeroProbabilityOutcome`) refuses only a probability of exactly
/// zero, and leaves a threshold to the method that needs one.
#[test]
fn accepts_a_condition_on_a_value_of_rounding_weight() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let state = one_of_rounding_weight(&engine);
        let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
        assert!(p1 > 0.0 && p1 < 1e-20, "{name}: a weight of rounding, {p1}");
        engine
            .conditional_probability_of_one(&state, 1, &[(0, true)])
            .expect("a condition of nonzero probability");
    }
}

/// A condition on Zero where P(Zero) is 1e-18: one minus P(One) rounds to
/// zero, but the engine reads P(Zero) from its own weight, so the
/// condition is possible and its answer exact.
#[test]
fn accepts_a_condition_whose_value_is_rare_but_possible() {
    let theta = 2.0 * (1e-9f64).acos();
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(theta)).expect("apply");
        engine.apply(&mut state, &[1], &ry(0.8)).expect("apply");
        let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
        assert!((1.0 - p1).abs() < 1e-15, "{name}: P(Zero) rounds away");
        let given = engine
            .conditional_probability_of_one(&state, 1, &[(0, false)])
            .expect("a possible condition");
        let expected = 0.4f64.sin().powi(2);
        assert!((given - expected).abs() < 1e-12, "{name}: {given}");
    }
}

/// A projection on |00⟩'s One: its probability is exactly zero.
#[test]
fn refuses_a_projection_on_a_value_of_probability_zero() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
        let error = engine
            .project(&mut state, 0, true)
            .expect_err("an impossible value");
        assert_eq!(error, Refusal::ZeroProbabilityOutcome.into(), "{name}");
    }
}

/// `cos 0.4·|00⟩ + sin 0.4·|11⟩`, by Ry(0.8) on site 0 and a CX, on a
/// chain of capacity 1: the CX's split keeps `|00⟩`, dropping the value
/// One of site 0. With fusion the CX's pair is still pending.
fn a_pair_truncated_to_zero_zero(engine: &Engine) -> State {
    let mut state = engine.prepare(&chain(2, 1)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.8)).expect("apply");
    engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
    state
}

/// P(One) of both sites and the retained weight of `state`.
fn reads(engine: &Engine, state: &State) -> [f64; 3] {
    [
        engine.probability_of_one(state, 0).expect("P(One)"),
        engine.probability_of_one(state, 1).expect("P(One)"),
        engine.retained_weight(state).expect("retained weight"),
    ]
}

/// A projection on the value of site 0 that the capacity drops from
/// `a_pair_truncated_to_zero_zero`: with fusion the projection first
/// applies the pending pair, as any read does, so both engines refuse it
/// (`ZeroProbabilityOutcome`), and the reads after it are the reads
/// before it, the same with or without fusion: P(One) 0 on both sites and
/// the retained weight `cos²0.4`.
#[test]
fn a_refused_projection_leaves_what_reads_see_with_or_without_fusion() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let before = reads(&engine, &a_pair_truncated_to_zero_zero(&engine));
        let kept = 0.4f64.cos().powi(2);
        assert!(
            before[0].abs() < 1e-12 && before[1].abs() < 1e-12,
            "{name}: the capacity drops One, {before:?}"
        );
        assert!((before[2] - kept).abs() < 1e-12, "{name}: {before:?}");
        let mut state = a_pair_truncated_to_zero_zero(&engine);
        let error = engine
            .project(&mut state, 0, true)
            .expect_err("a value the capacity dropped");
        assert_eq!(error, Refusal::ZeroProbabilityOutcome.into(), "{name}");
        let after = reads(&engine, &state);
        for (a, b) in after.iter().zip(&before) {
            assert!((a - b).abs() < 1e-12, "{name}: {after:?} vs {before:?}");
        }
    }
}

/// A projection on a value of rounding weight is not refused: it returns
/// that weight, and the state it leaves keeps its norm.
#[test]
fn projects_on_a_value_of_rounding_weight() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = one_of_rounding_weight(&engine);
        let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
        let p = engine
            .project(&mut state, 0, true)
            .expect("a value of nonzero probability");
        assert!((p - p1).abs() <= 1e-12 * p1, "{name}: {p} vs {p1}");
        let weight = engine.retained_weight(&state).expect("weight");
        assert!((weight - 1.0).abs() < 1e-9, "{name}: {weight}");
    }
}

/// A Toffoli the method did not decompose, after a pair that fusion keeps
/// pending: the engine refuses it with the contract's refusal, with or
/// without fusion, and the reads after it are the reads before it.
#[test]
fn refuses_a_gate_on_three_sites_leaving_what_reads_see() {
    let identity: Vec<Complex64> = (0..64)
        .map(|at| real(if at % 9 == 0 { 1.0 } else { 0.0 }))
        .collect();
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
        engine.apply(&mut state, &[0], &ry(0.8)).expect("apply");
        engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
        let p1 = |state: &State| -> Vec<f64> {
            (0..3)
                .map(|site| engine.probability_of_one(state, site).expect("P(One)"))
                .collect()
        };
        let before = p1(&state);
        let error = engine
            .apply(&mut state, &[0, 1, 2], &identity)
            .expect_err("a gate on three sites");
        assert_eq!(error, Refusal::TooManySites { sites: 3 }.into(), "{name}");
        let after = p1(&state);
        for (a, b) in after.iter().zip(&before) {
            assert!((a - b).abs() < 1e-12, "{name}: {after:?} vs {before:?}");
        }
    }
}

/// A capacity whose first site has three levels: the engine holds qubits
/// only, and refuses it with the contract's refusal.
#[test]
fn refuses_a_qutrit() {
    let qutrit = MpsShape::new(vec![vec![3, 1], vec![1, 2]]).expect("a shape");
    let error = Engine::new(FakeAlgebra)
        .prepare(&qutrit)
        .expect_err("a qutrit");
    assert_eq!(error, Refusal::NotTwoLevel { site: 0, levels: 3 }.into());
}

/// A simulator that gets an error asks it whether it was a refusal, to
/// keep the state: a refused call answers with the contract's refusal.
#[test]
fn a_refused_call_reports_its_refusal() {
    let engine = Engine::new(FakeAlgebra);
    let state = engine.prepare(&chain(2, 2)).expect("prepare");
    let error = engine
        .probability_of_one(&state, 2)
        .expect_err("site 2 of 2");
    assert_eq!(
        error.refusal(),
        Some(&Refusal::SiteOutOfRange { site: 2, sites: 2 })
    );
}

/// A stub of a backend whose reads come back NaN, as a device's may after
/// an overflow: [`FakeAlgebra`], except that `read` returns NaN elements.
#[derive(Clone)]
struct NanReadsStub;

impl SiteAlgebra for NanReadsStub {
    type Tensor = Vec<Complex64>;
    type Error = Infallible;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Infallible> {
        FakeAlgebra.upload(values)
    }

    fn read<'t>(
        &self,
        _: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Infallible> {
        Ok(Cow::Owned(vec![Complex64::new(f64::NAN, f64::NAN); len]))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Infallible> {
        FakeAlgebra.duplicate(tensor, len)
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Infallible> {
        FakeAlgebra.gate_split(call)
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Infallible> {
        FakeAlgebra.qr_move(call)
    }
}

/// A projection on a site whose read comes back NaN: its probability is
/// NaN, and the contract refuses it.
#[test]
fn refuses_a_projection_whose_probability_is_nan() {
    let engine = CanonicalMpsEngine::new(NanReadsStub);
    let mut state = engine.prepare(&chain(2, 2)).expect("prepare");
    let error = engine
        .project(&mut state, 0, false)
        .expect_err("a probability of NaN");
    assert_eq!(error, Refusal::ZeroProbabilityOutcome.into());
}

/// A condition on a site whose read comes back NaN: its probability is
/// NaN, and the contract refuses it.
#[test]
fn refuses_a_condition_whose_probability_is_nan() {
    let engine = CanonicalMpsEngine::new(NanReadsStub);
    let state = engine.prepare(&chain(2, 2)).expect("prepare");
    let error = engine
        .conditional_probability_of_one(&state, 1, &[(0, false)])
        .expect_err("a condition of probability NaN");
    assert_eq!(error, Refusal::ZeroProbabilityOutcome.into());
}
