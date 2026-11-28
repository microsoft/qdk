// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's errors, [`CanonicalMpsError`]: its refusals, then its
//! failures, on the fake [`FailingFakeAlgebra`] (the CPU algebra fails only
//! in its SVD).

mod failing_fake_algebra;
mod nan_reading_fake_algebra;
mod zero_uploading_fake_algebra;

use crate::engine::conformance::{capacity, cnot, real, ry};
use crate::engine::tests::spy_algebra::{SpiedCalls, SpyAlgebra};
use crate::engine::tests::{State, both_fusions, z_string};
use crate::site_algebra::SiteAlgebra;
use crate::{
    CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, CpuAlgebra, CpuMpsEngine as Engine,
    DEFAULT_CUTOFF,
};
use dense_linalg::SvdError;
use failing_fake_algebra::{AlgebraFailure, FailingFakeAlgebra, Operation};
use nan_reading_fake_algebra::NanReadingFakeAlgebra;
use num_complex::Complex64;
use tensornet::{MpsEngine, MpsEngineError, MpsShape, Refusal};
use zero_uploading_fake_algebra::ZeroUploadingFakeAlgebra;

/// The contract (`Refusal::ZeroProbabilityOutcome`) refuses a condition or
/// a projection on a value of probability exactly zero, or NaN, a
/// condition on the site read included. Zero: |00⟩'s One, and the One of
/// site 0 turned by 2e-200, whose squared weight underflows to zero. NaN:
/// a backend whose reads come back NaN, as a device's may after an
/// overflow; one that stores zeros, so the state's norm is zero; and one
/// whose reads turn NaN only after a possible condition's projection.
#[test]
fn a_value_of_zero_or_nan_probability_is_refused() {
    for (scene, turn) in [("|00⟩", 0.0), ("a turn by 2e-200", 2e-200)] {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let scene = format!("{scene}, {name}");
            let mut state = site_0_turned_by(&engine, turn);
            assert_one_of_site_0_reads_zero(&engine, &state, &format!("{scene}: before"));
            assert_refused_on_site_0(&engine, &mut state, true, &scene);
            assert_one_of_site_0_reads_zero(&engine, &state, &format!("{scene}: after"));
        }
    }
    let nan_reading = CanonicalMpsEngine::new(NanReadingFakeAlgebra::new());
    assert_every_read_of_site_0_refused(&nan_reading, "reads of NaN");
    let zero_uploading = CanonicalMpsEngine::new(ZeroUploadingFakeAlgebra);
    assert_every_read_of_site_0_refused(&zero_uploading, "a norm of zero");
    let engine = nan_reading_after_a_projection();
    let state = engine.prepare(&capacity(2, 2)).expect("prepare");
    let error = engine
        .conditional_probability_of_one(&state, 1, &[(0, false)])
        .expect_err("an answer of NaN after a possible condition");
    assert_eq!(
        error,
        Refusal::ZeroProbabilityOutcome.into(),
        "after a projection"
    );
}

/// The contract leaves a threshold to the simulation method that needs
/// one. So a condition or a projection on a value of a weight of the size
/// rounding leaves, rare but possible, is accepted, one row per call. The
/// value's weight is about 1e-24, far below any threshold of rounding and
/// far above zero.
#[test]
fn a_value_of_rounding_weight_is_accepted() {
    type Check = fn(&Engine, &str);
    let rows: [(&str, Check); 2] = [
        (
            "a condition on One of rounding weight",
            assert_condition_on_rare_one_accepted,
        ),
        (
            "a projection on One of rounding weight",
            assert_projection_on_rare_one_accepted,
        ),
    ];
    for (row, check) in rows {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            check(&engine, &format!("{row}, {name}"));
        }
    }
}

/// Two sites, site 0 turned by `turn` from Zero.
fn site_0_turned_by(engine: &Engine, turn: f64) -> State {
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(turn)).expect("apply");
    state
}

/// Site 0 turned by 2e-12, so that One has weight `sin²(1e-12)`, about
/// 1e-24. Site 1 turned by 0.8.
fn one_of_rounding_weight(engine: &Engine) -> State {
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    engine.apply(&mut state, &[0], &ry(2e-12)).expect("apply");
    engine.apply(&mut state, &[1], &ry(0.8)).expect("apply");
    state
}

/// An engine whose reads are the stored values until a projection of site
/// 0 uploads what it kept, and NaN after, however many reads come first.
/// So on its prepared state a condition on Zero at site 0 is possible, and
/// only the answer's reads are NaN.
fn nan_reading_after_a_projection() -> CanonicalMpsEngine<NanReadingFakeAlgebra> {
    let (spy, calls) = SpyAlgebra::new();
    CanonicalMpsEngine::new(spy)
        .prepare(&capacity(2, 2))
        .expect("prepare");
    let uploads = calls.uploads.borrow().len() + 1;
    let engine = || CanonicalMpsEngine::new(NanReadingFakeAlgebra::after_uploads(uploads));
    let probe = engine();
    let mut state = probe.prepare(&capacity(2, 2)).expect("prepare");
    let p = probe.project(&mut state, 0, false).expect("stored reads");
    assert_eq!(p, 1.0, "the prepared state is Zero on site 0");
    engine()
}

/// P(One) of site 0 reads exactly zero.
fn assert_one_of_site_0_reads_zero(engine: &Engine, state: &State, scene: &str) {
    let read = engine.probability_of_one(state, 0).expect("P(One)");
    assert_eq!(read, 0.0, "{scene}");
}

/// On a prepared state of `engine`, whose probabilities read NaN, every
/// read of site 0 is refused: with and without conditions, a sample, and
/// a projection.
fn assert_every_read_of_site_0_refused<A: SiteAlgebra<Error = SvdError>>(
    engine: &CanonicalMpsEngine<A>,
    scene: &str,
) {
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    assert_refused_without_conditions(engine, &state, scene);
    assert_refused_on_site_0(engine, &mut state, false, scene);
}

/// A condition on One of rounding weight is accepted.
fn assert_condition_on_rare_one_accepted(engine: &Engine, scene: &str) {
    let state = one_of_rounding_weight(engine);
    let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
    assert!(
        p1 > 0.0 && p1 < 1e-20,
        "{scene}: a weight of rounding, {p1}"
    );
    engine
        .conditional_probability_of_one(&state, 1, &[(0, true)])
        .expect("a condition of nonzero probability");
}

/// A projection on One of rounding weight returns that weight, and the
/// state it leaves keeps its norm.
fn assert_projection_on_rare_one_accepted(engine: &Engine, scene: &str) {
    let mut state = one_of_rounding_weight(engine);
    let p1 = engine.probability_of_one(&state, 0).expect("P(One)");
    let p = engine
        .project(&mut state, 0, true)
        .expect("a value of nonzero probability");
    assert!((p - p1).abs() <= 1e-12 * p1, "{scene}: {p} vs {p1}");
    let weight = engine.retained_weight(&state).expect("weight");
    assert!((weight - 1.0).abs() < 1e-9, "{scene}: {weight}");
}

/// On a state whose norm is zero or NaN, a conditional read of site 0
/// without conditions and a sample of site 0 alone are each refused with
/// [`Refusal::ZeroProbabilityOutcome`]: no projection refuses them first.
fn assert_refused_without_conditions<A: SiteAlgebra<Error = SvdError>>(
    engine: &CanonicalMpsEngine<A>,
    state: &CanonicalMpsState<A>,
    scene: &str,
) {
    let read = engine
        .conditional_probability_of_one(state, 0, &[])
        .expect_err("a read of NaN");
    let sample = engine
        .sample(state, &[0], 4, 7)
        .expect_err("a draw from NaN");
    for (call, error) in [
        ("read without conditions", read),
        ("sample of one site", sample),
    ] {
        assert_eq!(
            error,
            Refusal::ZeroProbabilityOutcome.into(),
            "{scene}: {call}"
        );
    }
}

/// A condition of site 1 on `one` at site 0, a condition of site 0 on it,
/// and a projection of site 0 on it are each refused with
/// [`Refusal::ZeroProbabilityOutcome`].
fn assert_refused_on_site_0<A: SiteAlgebra<Error = SvdError>>(
    engine: &CanonicalMpsEngine<A>,
    state: &mut CanonicalMpsState<A>,
    one: bool,
    scene: &str,
) {
    let condition = engine
        .conditional_probability_of_one(state, 1, &[(0, one)])
        .expect_err("an impossible condition");
    let on_the_site_read = engine
        .conditional_probability_of_one(state, 0, &[(0, one)])
        .expect_err("an impossible condition on the site read");
    let projection = engine
        .project(state, 0, one)
        .expect_err("an impossible value");
    for (call, error) in [
        ("condition", condition),
        ("condition on the site read", on_the_site_read),
        ("projection", projection),
    ] {
        assert_eq!(
            error,
            Refusal::ZeroProbabilityOutcome.into(),
            "{scene}: {call}"
        );
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
/// `a_pair_truncated_to_zero_zero`. With fusion, the projection first
/// applies the pending window, as a read does. So both engines refuse it
/// (`ZeroProbabilityOutcome`). The reads after it are the reads before it,
/// the same with or without fusion: P(One) 0 on both sites and the
/// retained weight `cos²0.4`.
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

/// A gate the contract refuses, after a gate that fusion keeps pending: the
/// engine refuses it with the contract's refusal, with or without fusion,
/// and the reads after it are the reads before it, so it neither changed
/// the state nor joined the pending window. One row per refusal: a Toffoli
/// the simulation method did not decompose, refused as such whatever its
/// matrix's length; a matrix of the wrong length; and matrices that are not
/// unitary, on one site or on the pending gate's two.
#[test]
fn a_refused_gate_leaves_what_reads_see() {
    type Row = (
        &'static str,
        &'static [usize],
        Vec<Complex64>,
        fn(&Refusal) -> bool,
    );
    let identity: Vec<Complex64> = (0..64)
        .map(|at| real(if at % 9 == 0 { 1.0 } else { 0.0 }))
        .collect();
    let too_many: fn(&Refusal) -> bool = |r| *r == Refusal::TooManySites { sites: 3 };
    let not_unitary: fn(&Refusal) -> bool = |r| matches!(r, Refusal::NotUnitary { .. });
    let rows: [Row; 7] = [
        ("a Toffoli", &[0, 1, 2], identity, too_many),
        (
            "a short matrix on three sites",
            &[0, 1, 2],
            vec![real(1.0); 3],
            too_many,
        ),
        ("a short matrix", &[0], vec![real(1.0)], |r| {
            *r == Refusal::MatrixLength {
                sites: 1,
                expected: 4,
                actual: 1,
            }
        }),
        ("a NaN matrix", &[0], vec![real(f64::NAN); 4], not_unitary),
        (
            "an infinite matrix",
            &[0],
            vec![real(f64::INFINITY); 4],
            not_unitary,
        ),
        (
            "2·I",
            &[0],
            vec![real(2.0), real(0.0), real(0.0), real(2.0)],
            not_unitary,
        ),
        (
            "a non-unitary gate on the pending pair",
            &[0, 1],
            vec![real(2.0); 16],
            not_unitary,
        ),
    ];
    for (row, sites, matrix, refusal) in &rows {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let pending = || {
                let mut state = engine.prepare(&capacity(3, 4)).expect("prepare");
                engine.apply(&mut state, &[0], &ry(0.8)).expect("apply");
                engine.apply(&mut state, &[0, 1], &cnot()).expect("apply");
                state
            };
            let p1 = |state: &State| -> Vec<f64> {
                (0..3)
                    .map(|site| engine.probability_of_one(state, site).expect("P(One)"))
                    .collect()
            };
            // Read on a state of its own, since a read applies the window.
            let before = p1(&pending());
            let mut state = pending();
            let error = engine
                .apply(&mut state, sites, matrix)
                .expect_err("a refused gate");
            assert!(
                error.refusal().is_some_and(refusal),
                "{row}, {name}: {error:?}"
            );
            let after = p1(&state);
            for (a, b) in after.iter().zip(&before) {
                assert!(
                    (a - b).abs() < 1e-12,
                    "{row}, {name}: {after:?} vs {before:?}"
                );
            }
        }
    }
}

type SpiedEngine = CanonicalMpsEngine<SpyAlgebra>;
type SpiedState = CanonicalMpsState<SpyAlgebra>;

/// A call of a checked method on a state of two sites, and the error it
/// returns.
type CheckedCall = fn(&SpiedEngine, &mut SpiedState) -> CanonicalMpsError<SvdError>;

/// A simulator that gets an error asks it whether it was a refusal, to
/// keep the state: each method the contract checks refuses an invalid call
/// with the contract's refusal, before any call of the algebra, even with
/// a window pending (a CX on two sites, under fusion). One row per checked
/// method and kind of fault, in [`invalid_calls`].
#[test]
fn a_refused_call_reports_its_refusal_before_any_call_of_the_algebra() {
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
    for (row, call, refusal) in invalid_calls() {
        let before = tally(&calls);
        let error = call(&engine, &mut state);
        assert_eq!(error.refusal(), Some(&refusal), "{row}");
        assert_eq!(tally(&calls), before, "{row}: calls of the algebra");
    }
}

/// The invalid calls of
/// `a_refused_call_reports_its_refusal_before_any_call_of_the_algebra`,
/// with the refusal each gets: `apply` on three sites is refused as such
/// before its sites or its matrix are checked; `sample` also with no shot,
/// which draws nothing yet still checks its sites.
fn invalid_calls() -> [(&'static str, CheckedCall, Refusal); 14] {
    let out_of_range = Refusal::SiteOutOfRange { site: 2, sites: 2 };
    let repeated = Refusal::RepeatedSite { site: 0 };
    [
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
            "apply a short matrix on three sites",
            |engine, state| {
                engine
                    .apply(state, &[0, 1, 2], &[real(1.0); 3])
                    .expect_err("refused")
            },
            Refusal::TooManySites { sites: 3 },
        ),
        (
            "apply a short matrix",
            |engine, state| {
                engine
                    .apply(state, &[0], &[real(1.0)])
                    .expect_err("refused")
            },
            Refusal::MatrixLength {
                sites: 1,
                expected: 4,
                actual: 1,
            },
        ),
        // `M·M† − I` is `3·I`, whose entries sum to 6 in magnitude.
        (
            "apply 2·I",
            |engine, state| {
                let twice = [real(2.0), real(0.0), real(0.0), real(2.0)];
                engine.apply(state, &[0], &twice).expect_err("refused")
            },
            Refusal::NotUnitary { deviation: 6.0 },
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
    ]
}

/// The error of an engine on [`FailingFakeAlgebra`].
type Failure = CanonicalMpsError<AlgebraFailure>;

/// A state of an engine on [`FailingFakeAlgebra`].
type FakeState = CanonicalMpsState<FailingFakeAlgebra>;

/// `error` is the failure of the algebra's `operation`, not a refusal.
fn assert_fails_in(error: &Failure, operation: Operation, context: &str) {
    assert_eq!(
        error,
        &CanonicalMpsError::Algebra(AlgebraFailure(operation)),
        "{context}"
    );
    assert!(error.refusal().is_none(), "{context}");
}

/// A sample of more values than memory holds fails, not a refusal and not a
/// panic: `usize::MAX` shots of two sites overflow the count. The count's
/// other edges are the shared sampler's, tested in `tensornet`.
#[test]
fn a_sample_too_large_for_memory_fails() {
    let engine = Engine::new(CpuAlgebra);
    let state = engine.prepare(&capacity(2, 2)).expect("prepare");
    let (sites, shots) = (&[0, 1][..], usize::MAX);
    let error = engine
        .sample(&state, sites, shots, 0)
        .expect_err("too many values");
    assert_eq!(
        error,
        CanonicalMpsError::<SvdError>::TooManyValues {
            shots,
            sites: sites.len()
        }
    );
    assert!(error.refusal().is_none());
}

/// When a call to the algebra fails (as a backend's SVD may not
/// converge), the engine's method that made it fails with the algebra's
/// error, not a refusal. It does not return with sites lost, or read stale
/// ones. One row per engine call and the algebra operation it makes, each
/// with the algebra failing that operation.
#[test]
fn every_algebra_failure_reaches_the_caller() {
    let failures = [
        failures_of_preparations_and_gates(),
        failures_after_a_pending_window(),
        failures_of_reads_projections_and_forks(),
    ];
    for (name, operation, error) in failures.iter().flatten() {
        assert_fails_in(error, *operation, name);
    }
}

/// A row of `every_algebra_failure_reaches_the_caller`: the engine's call
/// that failed, named; the algebra's operation that failed in it; and the
/// error the engine's call returned.
type FailureRow = (String, Operation, Failure);

/// The chain most rows prepare: 3 sites of bond 4; a row that needs
/// another says so.
const SITES: usize = 3;

/// The rows of preparations and gates.
fn failures_of_preparations_and_gates() -> Vec<FailureRow> {
    let sites = SITES;
    let mut failures = Vec::new();

    // A preparation uploads each site.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Operation::Upload));
    let Err(error) = engine.prepare(&capacity(sites, 4)) else {
        panic!("a preparation needs an upload");
    };
    failures.push(("prepare".into(), Operation::Upload, error));

    // A gate on neighbours splits its pair, and a gate on sites 0 and 2 of
    // 3, a matrix product operator, takes two splits, one per cut of its
    // span, once a read applies its window: whichever fails, though the
    // algebra answers the others.
    for (on, splits) in [(&[0, 1][..], 1), (&[0, 2], 2)] {
        for failing in 0..splits {
            let (algebra, asked) = FailingFakeAlgebra::on_call(Operation::GateSplit, failing);
            let engine = CanonicalMpsEngine::new(algebra);
            let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
            engine
                .apply(&mut state, on, &cnot())
                .expect("a fused gate takes no split");
            let error = engine.retained_weight(&state).expect_err("no split");
            assert!(
                asked.get() > failing,
                "precondition: the gate on {on:?} asks for split {failing}"
            );
            let name = format!("a gate on {on:?}, split {failing}");
            failures.push((name, Operation::GateSplit, error));
        }
    }

    // A gate whose sites, with the pending window's, number more than a
    // window holds applies the window first: the gate itself fails as that
    // application does. On 4 sites, since on 3 every pair of gates fits one
    // window.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Operation::GateSplit));
    let mut state = engine.prepare(&capacity(4, 4)).expect("prepare");
    engine
        .apply(&mut state, &[0, 1], &cnot())
        .expect("a fused gate takes no split");
    let error = engine
        .apply(&mut state, &[2, 3], &cnot())
        .expect_err("the gate on sites 2 and 3 applies the window on 0 and 1");
    let name = "a gate that does not fit the pending window".into();
    failures.push((name, Operation::GateSplit, error));

    // A gate on sites 0 and 2 of 3 is a matrix product operator, which,
    // once a read applies its window, uploads each site of its span
    // multiplied by its factor.
    let (algebra, asked) = FailingFakeAlgebra::on_call(Operation::Upload, sites + 1);
    let engine = CanonicalMpsEngine::new(algebra);
    let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    engine
        .apply(&mut state, &[0, 2], &cnot())
        .expect("a fused gate takes no upload");
    let error = engine
        .retained_weight(&state)
        .expect_err("the operator's second upload fails");
    assert!(
        asked.get() > sites + 1,
        "precondition: the operator asked for its second upload"
    );
    failures.push((
        "an operator on sites 0 and 2".into(),
        Operation::Upload,
        error,
    ));

    failures
}

/// The rows of a CNOT on sites 0 and 1, which fusion leaves pending; then
/// a read or a fork, which applies the window first.
fn failures_after_a_pending_window() -> Vec<FailureRow> {
    type Read<'a> = &'a dyn Fn(&mut FakeState) -> Result<(), Failure>;
    let sites = SITES;
    let mut failures = Vec::new();
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Operation::GateSplit));
    let z = z_string(&[2]);
    let reads: [(&str, Read<'_>); 9] = [
        ("P(One)", &|state| {
            engine.probability_of_one(state, 2).map(drop)
        }),
        ("P(One) given nothing", &|state| {
            engine
                .conditional_probability_of_one(state, 2, &[])
                .map(drop)
        }),
        ("P(One) given a value", &|state| {
            engine
                .conditional_probability_of_one(state, 2, &[(0, false)])
                .map(drop)
        }),
        ("project", &|state| {
            engine.project(state, 2, false).map(drop)
        }),
        ("sample", &|state| {
            engine.sample(state, &[2], 1, 0).map(drop)
        }),
        ("expectation", &|state| {
            engine.expectation(state, &z).map(drop)
        }),
        ("shape", &|state| engine.shape(state).map(drop)),
        ("retained_weight", &|state| {
            engine.retained_weight(state).map(drop)
        }),
        ("fork", &|state| engine.fork(state).map(drop)),
    ];
    for (name, read) in reads {
        let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
        engine
            .apply(&mut state, &[0, 1], &cnot())
            .expect("a fused gate takes no split");
        let error = read(&mut state).expect_err(name);
        let name = format!("{name} after a pending window");
        failures.push((name, Operation::GateSplit, error));
    }
    failures
}

/// The rows of reads, projections and forks.
fn failures_of_reads_projections_and_forks() -> Vec<FailureRow> {
    let sites = SITES;
    let mut failures = Vec::new();

    // A read of a site away from the centre moves the centre there, by one
    // move per site crossed.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Operation::QrMove));
    let state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    let error = engine
        .probability_of_one(&state, 2)
        .expect_err("the centre starts on site 0");
    failures.push(("P(One) of site 2".into(), Operation::QrMove, error));

    // A read copies its sites to the host: a probability, which reads the
    // centre, and an expectation, which reads every site its terms span.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Operation::Read));
    let state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    let error = engine
        .probability_of_one(&state, 0)
        .expect_err("a read of the centre");
    failures.push(("P(One) of the centre".into(), Operation::Read, error));
    let error = engine
        .expectation(&state, &z_string(&[1, 2]))
        .expect_err("a read of sites 1 and 2");
    failures.push(("an expectation".into(), Operation::Read, error));

    // A projection uploads the site it projected.
    let (algebra, asked) = FailingFakeAlgebra::on_call(Operation::Upload, sites);
    let engine = CanonicalMpsEngine::new(algebra);
    let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    assert_eq!(
        asked.get(),
        sites,
        "precondition: the preparation uploads one tensor per site"
    );
    let error = engine
        .project(&mut state, 1, false)
        .expect_err("the projection's upload fails");
    failures.push(("project".into(), Operation::Upload, error));

    // A fork duplicates each site.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Operation::Duplicate));
    let state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    let Err(error) = engine.fork(&state) else {
        panic!("a fork duplicates its sites");
    };
    failures.push(("fork".into(), Operation::Duplicate, error));
    failures
}

/// A CNOT on sites 0 and 2 of 3, applied by the retained weight. The read
/// or upload of the span's second site fails, after the first site was
/// read, multiplied and uploaded. The chain keeps the span's sites
/// unmultiplied. So its neighbours still share each bond, and its shape is
/// the one before the gate, not a site of bond 2 beside one of bond 1.
/// The preparation uploads each site and reads none, so the operator's
/// calls are numbered after it.
#[test]
fn a_failed_read_or_upload_leaves_the_span_unmultiplied() {
    let sites = 3;
    for (operation, failing) in [(Operation::Upload, sites + 1), (Operation::Read, 1)] {
        let (algebra, asked) = FailingFakeAlgebra::on_call(operation, failing);
        let engine = CanonicalMpsEngine::new(algebra);
        let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
        let before = engine.shape(&state).expect("shape before the gate");
        engine
            .apply(&mut state, &[0, 2], &cnot())
            .expect("a fused gate waits for a read");
        let error = engine
            .retained_weight(&state)
            .expect_err("the operator's call on the second site fails");
        assert!(
            asked.get() > failing,
            "precondition: the operator called {operation:?} on the second site"
        );
        assert_fails_in(&error, operation, "a gate on sites 0 and 2");
        let after = engine.shape(&state).expect("shape after the failed gate");
        assert_eq!(after, before, "a failed {operation:?}");
    }
}
