// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's failures, apart from refusals: a sample too large for
//! memory, and each method of the algebra failing, on the fake
//! [`FailingFakeAlgebra`], since the reference algebra, [`CpuAlgebra`],
//! fails only when its SVD does, which the engine's tests never make it do.

use super::failing_fake_algebra::{AlgebraFailure, Method};
use super::*;

/// The error of an engine on [`FailingFakeAlgebra`].
type Failure = CanonicalMpsError<AlgebraFailure>;

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
    let state = engine.prepare(&chain(2, 2)).expect("prepare");
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

/// A preparation uploads each site; when an upload fails, the preparation
/// fails with the algebra's error.
#[test]
fn a_preparation_fails_when_an_upload_fails() {
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::Upload));
    let Err(error) = engine.prepare(&chain(3, 4)) else {
        panic!("a preparation needs an upload");
    };
    assert_fails_in(&error, Method::Upload, "prepare");
}

/// Without fusion, a two-site gate splits its pair by the algebra's gate
/// split; when the split fails (as a backend's SVD may not converge), the
/// gate fails with the algebra's error, not a refusal.
#[test]
fn a_gate_fails_when_its_split_fails() {
    let engine =
        CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::GateSplit)).with_fusion(false);
    let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
    engine
        .apply(&mut state, &[1], &ry(0.4))
        .expect("a one-site gate needs no split");
    let error = engine
        .apply(&mut state, &[0, 2], &cnot())
        .expect_err("no split");
    assert_fails_in(&error, Method::GateSplit, "a gate on sites 0 and 2");
}

/// Without fusion, a gate on neighbours splits its pair, and a gate on
/// sites 0 and 2 of 3, a matrix product operator, takes two splits, one
/// per cut of its span. When one of them fails, though the algebra answers
/// the others, the gate fails with the algebra's error, whichever split it
/// was, rather than return with the sites it was splitting lost.
#[test]
fn an_unfused_gate_fails_when_one_of_its_splits_fails() {
    for (sites, splits) in [(&[0, 1][..], 1), (&[0, 2], 2)] {
        for failing in 0..splits {
            let (algebra, asked) = FailingFakeAlgebra::on_call(Method::GateSplit, failing);
            let engine = CanonicalMpsEngine::new(algebra).with_fusion(false);
            let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
            let error = engine
                .apply(&mut state, sites, &cnot())
                .expect_err("no split");
            assert!(
                asked.get() > failing,
                "precondition: the gate on {sites:?} asks for split {failing}"
            );
            assert_fails_in(
                &error,
                Method::GateSplit,
                &format!("{sites:?}, split {failing}"),
            );
        }
    }
}

/// A CNOT on sites 0 and 1 of 3, which fusion leaves pending; then a read
/// or a fork, which applies the window first. When its split fails, the
/// read fails with the algebra's error, not a refusal, rather than read
/// the sites the window left stale.
#[test]
fn a_read_fails_when_the_split_of_its_pending_window_fails() {
    type State = CanonicalMpsState<FailingFakeAlgebra>;
    type Read<'a> = &'a dyn Fn(&mut State) -> Result<(), Failure>;
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
        let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
        engine
            .apply(&mut state, &[0, 1], &cnot())
            .expect("a fused gate takes no split");
        let error = read(&mut state).expect_err(name);
        assert_fails_in(&error, Method::GateSplit, name);
    }
}

/// Without fusion, a gate on sites 0 and 2 of 3 is a matrix product
/// operator, which uploads each site of its span multiplied by its factor;
/// when an upload fails, the gate fails with the algebra's error.
#[test]
fn a_gate_on_distant_sites_fails_when_an_upload_fails() {
    let sites = 3;
    let (algebra, asked) = FailingFakeAlgebra::on_call(Method::Upload, sites + 1);
    let engine = CanonicalMpsEngine::new(algebra).with_fusion(false);
    let mut state = engine.prepare(&chain(sites, 4)).expect("prepare");
    let error = engine
        .apply(&mut state, &[0, 2], &cnot())
        .expect_err("the operator's second upload fails");
    assert!(
        asked.get() > sites + 1,
        "precondition: the operator asked for its second upload"
    );
    assert_fails_in(&error, Method::Upload, "a gate on sites 0 and 2");
}

/// A read of a site away from the centre moves the centre there, by one
/// move per site crossed; when a move fails, the read fails with the
/// algebra's error.
#[test]
fn a_read_fails_when_a_move_of_the_centre_fails() {
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::QrMove));
    let state = engine.prepare(&chain(3, 4)).expect("prepare");
    let error = engine
        .probability_of_one(&state, 2)
        .expect_err("the centre starts on site 0");
    assert_fails_in(&error, Method::QrMove, "P(One) of site 2");
}

/// A read copies its sites to the host; when the copy fails, the read
/// fails with the algebra's error: a probability, which reads the centre,
/// and an expectation, which reads every site its terms span.
#[test]
fn a_read_fails_when_the_copy_of_a_site_fails() {
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::Read));
    let state = engine.prepare(&chain(3, 4)).expect("prepare");
    let error = engine
        .probability_of_one(&state, 0)
        .expect_err("a read of the centre");
    assert_fails_in(&error, Method::Read, "P(One)");
    let error = engine
        .expectation(&state, &z_string(&[1, 2]))
        .expect_err("a read of sites 1 and 2");
    assert_fails_in(&error, Method::Read, "expectation");
}

/// A projection uploads the site it projected; when the upload fails, the
/// projection fails with the algebra's error.
#[test]
fn a_projection_fails_when_the_upload_of_its_site_fails() {
    let sites = 3;
    let (algebra, asked) = FailingFakeAlgebra::on_call(Method::Upload, sites);
    let engine = CanonicalMpsEngine::new(algebra);
    let mut state = engine.prepare(&chain(sites, 4)).expect("prepare");
    assert_eq!(
        asked.get(),
        sites,
        "precondition: the preparation uploads one tensor per site"
    );
    let error = engine
        .project(&mut state, 1, false)
        .expect_err("the projection's upload fails");
    assert_fails_in(&error, Method::Upload, "project");
}

/// A fork duplicates each site; when a duplicate fails, the fork fails
/// with the algebra's error.
#[test]
fn a_fork_fails_when_a_duplicate_fails() {
    let engine = CanonicalMpsEngine::new(FailingFakeAlgebra::every(Method::Duplicate));
    let state = engine.prepare(&chain(3, 4)).expect("prepare");
    let Err(error) = engine.fork(&state) else {
        panic!("a fork duplicates its sites");
    };
    assert_fails_in(&error, Method::Duplicate, "fork");
}
