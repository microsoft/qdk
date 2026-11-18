// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's failures, apart from refusals: a sample too large for
//! memory, and each method of the algebra failing, on the fake
//! [`FailingFakeAlgebra`], since the CPU algebra, [`CpuAlgebra`],
//! fails only when its SVD does, which the engine's tests never make it do.

use super::stand_ins::failing_fake_algebra::{AlgebraFailure, Method};
use super::*;

/// The error of an engine on [`FailingFakeAlgebra`].
type Failure = CanonicalMpsError<AlgebraFailure>;

/// A state of an engine on [`FailingFakeAlgebra`].
type FakeState = CanonicalMpsState<FailingFakeAlgebra>;

/// `error` is the failure of the algebra's `method`, not a refusal.
fn assert_fails_in(error: &Failure, method: Method, context: &str) {
    assert_eq!(
        error,
        &CanonicalMpsError::Algebra(AlgebraFailure(method)),
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
/// ones: one row per operation and the call it makes, each with the
/// algebra failing that call.
#[test]
fn every_algebra_failure_reaches_the_caller() {
    let failures = [
        failures_of_preparations_and_gates(),
        failures_after_a_pending_window(),
        failures_of_reads_projections_and_forks(),
    ];
    for (name, method, error) in failures.iter().flatten() {
        assert_fails_in(error, *method, name);
    }
}

/// A row of `every_algebra_failure_reaches_the_caller`: the operation, the
/// call that failed, and the error the operation returned.
type FailureRow = (String, Method, Failure);

/// The chain the rows prepare: 3 sites of bond 4.
const SITES: usize = 3;

/// The rows of preparations and gates, fused or not.
fn failures_of_preparations_and_gates() -> Vec<FailureRow> {
    let sites = SITES;
    let mut failures = Vec::new();

    // A preparation uploads each site.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::Upload));
    let Err(error) = engine.prepare(&capacity(sites, 4)) else {
        panic!("a preparation needs an upload");
    };
    failures.push(("prepare".into(), Method::Upload, error));

    // Without fusion, a two-site gate splits its pair by the algebra's
    // gate split.
    let engine =
        CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::GateSplit)).with_fusion(false);
    let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    engine
        .apply(&mut state, &[1], &ry(0.4))
        .expect("a one-site gate needs no split");
    let error = engine
        .apply(&mut state, &[0, 2], &cnot())
        .expect_err("no split");
    failures.push(("a gate on sites 0 and 2".into(), Method::GateSplit, error));

    // Without fusion, a gate on neighbours splits its pair, and a gate on
    // sites 0 and 2 of 3, a matrix product operator, takes two splits, one
    // per cut of its span: whichever fails, though the algebra answers the
    // others.
    for (on, splits) in [(&[0, 1][..], 1), (&[0, 2], 2)] {
        for failing in 0..splits {
            let (algebra, asked) = FailingFakeAlgebra::on_call(Method::GateSplit, failing);
            let engine = CanonicalMpsEngine::new(algebra).with_fusion(false);
            let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
            let error = engine.apply(&mut state, on, &cnot()).expect_err("no split");
            assert!(
                asked.get() > failing,
                "precondition: the gate on {on:?} asks for split {failing}"
            );
            let name = format!("a gate on {on:?}, split {failing}");
            failures.push((name, Method::GateSplit, error));
        }
    }

    // Without fusion, a gate on sites 0 and 2 of 3 is a matrix product
    // operator, which uploads each site of its span multiplied by its
    // factor.
    let (algebra, asked) = FailingFakeAlgebra::on_call(Method::Upload, sites + 1);
    let engine = CanonicalMpsEngine::new(algebra).with_fusion(false);
    let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    let error = engine
        .apply(&mut state, &[0, 2], &cnot())
        .expect_err("the operator's second upload fails");
    assert!(
        asked.get() > sites + 1,
        "precondition: the operator asked for its second upload"
    );
    failures.push(("an operator on sites 0 and 2".into(), Method::Upload, error));

    failures
}

/// The rows of a CNOT on sites 0 and 1, which fusion leaves pending; then
/// a read or a fork, which applies the window first.
fn failures_after_a_pending_window() -> Vec<FailureRow> {
    type Read<'a> = &'a dyn Fn(&mut FakeState) -> Result<(), Failure>;
    let sites = SITES;
    let mut failures = Vec::new();
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::GateSplit));
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
        failures.push((name, Method::GateSplit, error));
    }
    failures
}

/// The rows of reads, projections and forks.
fn failures_of_reads_projections_and_forks() -> Vec<FailureRow> {
    let sites = SITES;
    let mut failures = Vec::new();

    // A read of a site away from the centre moves the centre there, by one
    // move per site crossed.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::QrMove));
    let state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    let error = engine
        .probability_of_one(&state, 2)
        .expect_err("the centre starts on site 0");
    failures.push(("P(One) of site 2".into(), Method::QrMove, error));

    // A read copies its sites to the host: a probability, which reads the
    // centre, and an expectation, which reads every site its terms span.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::Read));
    let state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    let error = engine
        .probability_of_one(&state, 0)
        .expect_err("a read of the centre");
    failures.push(("P(One) of the centre".into(), Method::Read, error));
    let error = engine
        .expectation(&state, &z_string(&[1, 2]))
        .expect_err("a read of sites 1 and 2");
    failures.push(("an expectation".into(), Method::Read, error));

    // A projection uploads the site it projected.
    let (algebra, asked) = FailingFakeAlgebra::on_call(Method::Upload, sites);
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
    failures.push(("project".into(), Method::Upload, error));

    // A fork duplicates each site.
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::Duplicate));
    let state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    let Err(error) = engine.fork(&state) else {
        panic!("a fork duplicates its sites");
    };
    failures.push(("fork".into(), Method::Duplicate, error));
    failures
}

/// Without fusion, a CNOT on sites 0 and 2 of 3 whose read or upload of
/// the span's second site fails, after the first site was read, multiplied
/// and uploaded: the chain keeps the sites it had, so its neighbours still
/// share each bond and its shape is the one before the gate, not a site of
/// bond 2 beside one of bond 1. The preparation uploads each site and
/// reads none, so the operator's calls are numbered after it.
#[test]
fn a_failed_read_or_upload_leaves_the_span_as_it_was() {
    let sites = 3;
    for (method, failing) in [(Method::Upload, sites + 1), (Method::Read, 1)] {
        let (algebra, asked) = FailingFakeAlgebra::on_call(method, failing);
        let engine = CanonicalMpsEngine::new(algebra).with_fusion(false);
        let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
        let before = engine.shape(&state).expect("shape before the gate");
        let error = engine
            .apply(&mut state, &[0, 2], &cnot())
            .expect_err("the operator's call on the second site fails");
        assert!(
            asked.get() > failing,
            "precondition: the operator called {method:?} on the second site"
        );
        assert_fails_in(&error, method, "a gate on sites 0 and 2");
        let after = engine.shape(&state).expect("shape after the failed gate");
        assert_eq!(after, before, "a failed {method:?}");
    }
}
