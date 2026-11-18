// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The contract's refusals: a condition or a projection on a value of
//! probability zero or NaN is refused, leaving what reads see, while a
//! value of rounding weight or rare but possible is accepted; a gate on
//! three sites; and each checked method's refusal (a qutrit, a site out
//! of range or named twice), reported before any call of the algebra.

use super::*;
use crate::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use std::borrow::Cow;

/// Turns of 0.3 and 0.4 and back by 0.7: the site is Zero up to rounding,
/// so One keeps a weight of rounding, above zero.
fn one_of_rounding_weight(engine: &Engine) -> State {
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    for angle in [0.3, 0.4, -0.7] {
        engine.apply(&mut state, &[0], &ry(angle)).expect("apply");
    }
    engine.apply(&mut state, &[1], &ry(0.8)).expect("apply");
    state
}

/// A condition or a projection on a value of probability exactly zero,
/// |00⟩'s One, is refused with the contract's refusal.
#[test]
fn a_value_of_probability_zero_is_refused() {
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
        let condition = engine
            .conditional_probability_of_one(&state, 1, &[(0, true)])
            .expect_err("an impossible condition");
        let projection = engine
            .project(&mut state, 0, true)
            .expect_err("an impossible value");
        for (call, error) in [("condition", condition), ("projection", projection)] {
            assert_eq!(
                error,
                Refusal::ZeroProbabilityOutcome.into(),
                "{name}: {call}"
            );
        }
    }
}

/// A value of rounding weight, or rare but possible, is not refused: the
/// contract (`Refusal::ZeroProbabilityOutcome`) refuses only a probability
/// of exactly zero, and leaves a threshold to the method that needs one.
/// One row per call: a condition and a projection on One of rounding
/// weight, and a condition on Zero of weight 1e-18.
#[test]
fn a_value_of_rounding_weight_is_accepted() {
    type Row = fn(&Engine, &str);
    let rows: [(&str, Row); 3] = [
        ("a condition on One of rounding weight", |engine, name| {
            let state = one_of_rounding_weight(engine);
            let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
            assert!(p1 > 0.0 && p1 < 1e-20, "{name}: a weight of rounding, {p1}");
            engine
                .conditional_probability_of_one(&state, 1, &[(0, true)])
                .expect("a condition of nonzero probability");
        }),
        // The projection returns that weight, and the state it leaves keeps
        // its norm.
        ("a projection on One of rounding weight", |engine, name| {
            let mut state = one_of_rounding_weight(engine);
            let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
            let p = engine
                .project(&mut state, 0, true)
                .expect("a value of nonzero probability");
            assert!((p - p1).abs() <= 1e-12 * p1, "{name}: {p} vs {p1}");
            let weight = engine.retained_weight(&state).expect("weight");
            assert!((weight - 1.0).abs() < 1e-9, "{name}: {weight}");
        }),
        // One minus P(One) rounds to zero, but the engine reads P(Zero)
        // from its own weight, so the condition is possible and its answer
        // exact.
        ("a condition on Zero of weight 1e-18", |engine, name| {
            let theta = 2.0 * (1e-9f64).acos();
            let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
            engine.apply(&mut state, &[0], &ry(theta)).expect("apply");
            engine.apply(&mut state, &[1], &ry(0.8)).expect("apply");
            let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
            assert!((1.0 - p1).abs() < 1e-15, "{name}: P(Zero) rounds away");
            let given = engine
                .conditional_probability_of_one(&state, 1, &[(0, false)])
                .expect("a possible condition");
            let expected = 0.4f64.sin().powi(2);
            assert!((given - expected).abs() < 1e-12, "{name}: {given}");
        }),
    ];
    for (row, check) in rows {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            check(&engine, &format!("{row}, {name}"));
        }
    }
}

/// `cos 0.4·|00⟩ + sin 0.4·|11⟩`, by Ry(0.8) on site 0 and a CX, on a
/// chain of capacity 1: the CX's split keeps `|00⟩`, dropping the value
/// One of site 0. With fusion the CX's window is still pending.
fn a_pair_truncated_to_zero_zero(engine: &Engine) -> State {
    let mut state = engine.prepare(&capacity(2, 1)).expect("prepare");
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
/// applies the pending window, as a read does, so both engines refuse it
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

/// A Toffoli the method did not decompose, after a gate that fusion keeps
/// pending: the engine refuses it with the contract's refusal, with or
/// without fusion, and the reads after it are the reads before it.
#[test]
fn refuses_a_gate_on_three_sites_leaving_what_reads_see() {
    let identity: Vec<Complex64> = (0..64)
        .map(|at| real(if at % 9 == 0 { 1.0 } else { 0.0 }))
        .collect();
    for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
        let mut state = engine.prepare(&capacity(3, 4)).expect("prepare");
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

/// A simulator that gets an error asks it whether it was a refusal, to
/// keep the state: each method the contract checks refuses an invalid call
/// with the contract's refusal, before any call of the algebra, even with
/// a window pending (a CX on two sites, under fusion). One row per checked
/// method and kind of fault; `sample` also with no shot, which draws
/// nothing yet still checks its sites.
#[test]
fn a_refused_call_reports_its_refusal_before_any_call_of_the_algebra() {
    type SpiedEngine = CanonicalMpsEngine<SpyAlgebra>;
    type SpiedState = CanonicalMpsState<SpyAlgebra>;
    type Call = fn(&SpiedEngine, &mut SpiedState) -> CanonicalMpsError<SvdError>;
    let out_of_range = Refusal::SiteOutOfRange { site: 2, sites: 2 };
    let repeated = Refusal::RepeatedSite { site: 0 };
    let rows: [(&str, Call, Refusal); 11] = [
        // The engine holds qubits only.
        (
            "prepare a qutrit",
            |engine, _| {
                let qutrit = MpsShape::new(vec![vec![3, 1], vec![1, 2]]).expect("a shape");
                engine.prepare(&qutrit).expect_err("refused")
            },
            Refusal::NotTwoLevel { site: 0, levels: 3 },
        ),
        (
            "apply on site 2",
            |engine, state| engine.apply(state, &[2], &ry(0.3)).expect_err("refused"),
            out_of_range.clone(),
        ),
        (
            "apply on site 0 twice",
            |engine, state| engine.apply(state, &[0, 0], &cnot()).expect_err("refused"),
            repeated.clone(),
        ),
        (
            "probability_of_one of site 2",
            |engine, state| engine.probability_of_one(state, 2).expect_err("refused"),
            out_of_range.clone(),
        ),
        (
            "conditional_probability_of_one given site 2",
            |engine, state| {
                engine
                    .conditional_probability_of_one(state, 1, &[(2, true)])
                    .expect_err("refused")
            },
            out_of_range.clone(),
        ),
        (
            "project site 2",
            |engine, state| engine.project(state, 2, true).expect_err("refused"),
            out_of_range.clone(),
        ),
        (
            "sample site 2",
            |engine, state| engine.sample(state, &[2], 1, 5).expect_err("refused"),
            out_of_range.clone(),
        ),
        (
            "sample site 2, no shot",
            |engine, state| engine.sample(state, &[2], 0, 5).expect_err("refused"),
            out_of_range.clone(),
        ),
        (
            "sample site 0 twice",
            |engine, state| engine.sample(state, &[0, 0], 1, 5).expect_err("refused"),
            repeated.clone(),
        ),
        (
            "sample site 0 twice, no shot",
            |engine, state| engine.sample(state, &[0, 0], 0, 5).expect_err("refused"),
            repeated,
        ),
        (
            "expectation on site 2",
            |engine, state| {
                engine
                    .expectation(state, &z_string(&[2]))
                    .expect_err("refused")
            },
            out_of_range,
        ),
    ];
    let (algebra, calls) = SpyAlgebra::new();
    let engine = CanonicalMpsEngine::new(algebra);
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(0.8)).expect("apply");
    engine
        .apply(&mut state, &[0, 1], &cnot())
        .expect("a pending window");
    let tally = |calls: &SpiedCalls| {
        [
            calls.gate_splits.get(),
            calls.qr_moves.get(),
            calls.reads.get(),
            calls.duplicates.get(),
            calls.uploads.borrow().len(),
        ]
    };
    for (row, call, refusal) in rows {
        let before = tally(&calls);
        let error = call(&engine, &mut state);
        assert_eq!(error.refusal(), Some(&refusal), "{row}");
        assert_eq!(tally(&calls), before, "{row}: calls of the algebra");
    }
}

/// A stub of a backend whose reads come back NaN, as a device's may after
/// an overflow: [`CpuAlgebra`], except that `read` returns NaN elements.
#[derive(Clone)]
struct NanReadsStub;

impl SiteAlgebra for NanReadsStub {
    type Tensor = Vec<Complex64>;
    type Error = SvdError;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        CpuAlgebra.upload(values)
    }

    fn read<'t>(
        &self,
        _: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        Ok(Cow::Owned(vec![Complex64::new(f64::NAN, f64::NAN); len]))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        CpuAlgebra.duplicate(tensor, len)
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        CpuAlgebra.gate_split(call)
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        CpuAlgebra.qr_move(call)
    }
}

/// A condition or a projection on a site whose read comes back NaN: its
/// probability is NaN, and the contract refuses it.
#[test]
fn a_value_of_nan_probability_is_refused() {
    let engine = CanonicalMpsEngine::new(NanReadsStub);
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    let condition = engine
        .conditional_probability_of_one(&state, 1, &[(0, false)])
        .expect_err("a condition of probability NaN");
    let projection = engine
        .project(&mut state, 0, false)
        .expect_err("a probability of NaN");
    for (call, error) in [("condition", condition), ("projection", projection)] {
        assert_eq!(error, Refusal::ZeroProbabilityOutcome.into(), "{call}");
    }
}
