// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's errors, [`CanonicalMpsError`]: its refusals, then its
//! failures, on the fake [`FailingFakeAlgebra`] (the CPU algebra fails only
//! in its SVD).

use super::stand_ins::failing_fake_algebra::{AlgebraFailure, Operation};
use super::*;
use crate::site_algebra::SiteAlgebra;

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

/// A condition or a projection on a value of probability exactly zero, or
/// NaN, is refused with the contract's refusal, a condition on the site read
/// included. Zero: |00⟩'s One, and the One of site 0 turned by 2e-200,
/// whose elements are not zero but whose squared weight underflows to zero,
/// so it reads exactly 0, before and after the refusal. NaN: Zero on a
/// backend whose reads come back NaN, as a device's may after an overflow.
#[test]
fn a_value_of_zero_or_nan_probability_is_refused() {
    for (turn, scene) in [(0.0, "|00⟩"), (2e-200, "a turn by 2e-200")] {
        for (engine, name) in both_fusions(DEFAULT_CUTOFF) {
            let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
            engine.apply(&mut state, &[0], &ry(turn)).expect("apply");
            let read = engine.probability_of_one(&state, 0).expect("P(One)");
            assert_eq!(read, 0.0, "{scene}, {name}: before");
            assert_refused_on_site_0(&engine, &mut state, true, &format!("{scene}, {name}"));
            let read = engine.probability_of_one(&state, 0).expect("P(One)");
            assert_eq!(read, 0.0, "{scene}, {name}: after");
        }
    }
    let engine = CanonicalMpsEngine::new(NanReadingFakeAlgebra);
    let mut state = engine.prepare(&capacity(2, 2)).expect("prepare");
    assert_refused_on_site_0(&engine, &mut state, false, "reads of NaN");
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

/// A value of rounding weight, or rare but possible, is not refused: the
/// contract (`Refusal::ZeroProbabilityOutcome`) refuses only a probability
/// of exactly zero, and leaves a threshold to the simulation method that
/// needs one.
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
/// [`a_refused_call_reports_its_refusal_before_any_call_of_the_algebra`],
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
/// panic: `usize::MAX` shots of two sites overflow the count, and so do
/// `2^(BITS − 1)` shots of two sites, whose count would wrap to zero; and
/// `isize::MAX` shots of one site cannot be allocated.
#[test]
fn a_sample_too_large_for_memory_fails() {
    let engine = Engine::new(CpuAlgebra);
    let state = engine.prepare(&capacity(2, 2)).expect("prepare");
    for (sites, shots) in [
        (&[0, 1][..], usize::MAX),
        (&[0, 1], 1 << (usize::BITS - 1)),
        (&[1], usize::MAX / 2),
    ] {
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
}

/// When a call to the algebra fails (as a backend's SVD may not
/// converge), the engine's method that made it fails with the algebra's
/// error, not a refusal, rather than return with sites lost or read stale
/// ones: one row per engine call and the algebra operation it makes, each
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

/// A CNOT on sites 0 and 2 of 3, applied by the retained weight, whose
/// read or upload of the span's second site fails, after the first site
/// was read, multiplied and uploaded: the chain keeps the span's sites
/// unmultiplied, so its neighbours still share each bond and its shape is
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
