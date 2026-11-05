// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's failures, apart from refusals: a sample too large for
//! memory, and a split whose SVD fails, on the `FailingSvds` algebra, since
//! no real matrix is known to make the SVD fail there.

use super::*;

/// A sample of more values than memory holds fails, not a refusal and not a
/// panic: `usize::MAX` shots of two sites overflow the count, and so do
/// `2^(BITS − 1)` shots of two sites, whose count would wrap to zero; and
/// `isize::MAX` shots of one site cannot be allocated.
#[test]
fn a_sample_too_large_for_memory_fails() {
    let engine = CpuMpsEngine::default();
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
            CpuMpsError::TooManyValues {
                shots,
                sites: sites.len()
            }
        );
        assert!(error.refusal().is_none());
    }
}

/// `CpuLinearAlgebra`, except that the SVD numbered `failing`, counting
/// from 0, does not converge, or none does when `failing` is `None`;
/// `asked` counts the SVDs asked of it.
struct FailingSvds {
    failing: Option<usize>,
    asked: Rc<Cell<usize>>,
}

impl FailingSvds {
    /// An algebra on which no SVD converges.
    fn all() -> Self {
        Self {
            failing: None,
            asked: Rc::default(),
        }
    }
}

impl LinearAlgebra for FailingSvds {
    fn multiply(&self, a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
        CpuLinearAlgebra.multiply(a, b)
    }

    fn qr(&self, a: MatrixRef<'_>) -> Result<Qr, FactorizationError> {
        CpuLinearAlgebra.qr(a)
    }

    fn svd(&self, a: MatrixRef<'_>) -> Result<Svd, FactorizationError> {
        let number = self.asked.get();
        self.asked.set(number + 1);
        if self.failing.is_some_and(|failing| failing != number) {
            return CpuLinearAlgebra.svd(a);
        }
        Err(FactorizationError {
            reason: "did not converge",
            factorization: "SVD",
            rows: a.rows(),
            cols: a.cols(),
        })
    }
}

/// A two-site gate splits its pair by an SVD; when the SVD fails, the
/// gate fails with the SVD's error, not a refusal.
#[test]
fn fails_a_gate_whose_split_does_not_converge() {
    let engine = Engine::new(FailingSvds::all());
    let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
    engine
        .apply(&mut state, &[1], &ry(0.4))
        .expect("a one-site gate needs no SVD");
    let error = engine
        .apply(&mut state, &[0, 2], &cnot())
        .expect_err("no SVD");
    assert!(
        matches!(
            &error,
            CpuMpsError::Factorization(FactorizationError {
                reason: "did not converge",
                ..
            })
        ),
        "{error:?}"
    );
    assert!(error.refusal().is_none());
}

/// Without fusion, a gate on neighbours splits its pair, and a gate on
/// sites 0 and 2 of 3 takes three splits (the outbound SWAP's, the gate's
/// and the return's). When the SVD fails on one of them, though it
/// answers the others, the gate fails with the SVD's error,
/// whichever split it was, rather than return with the pair it was
/// splitting lost.
#[test]
fn fails_an_unfused_gate_whose_split_does_not_converge() {
    for (sites, splits) in [(&[0, 1][..], 1), (&[0, 2], 3)] {
        for failing in 0..splits {
            let asked = Rc::new(Cell::new(0));
            let algebra = FailingSvds {
                failing: Some(failing),
                asked: Rc::clone(&asked),
            };
            let engine = Engine::new(algebra).with_fusion(false);
            let mut state = engine.prepare(&chain(3, 4)).expect("prepare");
            let error = engine
                .apply(&mut state, sites, &cnot())
                .expect_err("no SVD");
            assert!(
                asked.get() > failing,
                "precondition: the gate on {sites:?} asks for SVD {failing}"
            );
            assert!(
                matches!(&error, CpuMpsError::Factorization(_)),
                "{sites:?}, SVD {failing}: {error:?}"
            );
            assert!(error.refusal().is_none(), "{sites:?}, SVD {failing}");
        }
    }
}

/// A CNOT on sites 0 and 1 of 3, which fusion leaves unsplit; then a read
/// or a fork, which splits the pair first. When the SVD of that split
/// fails, the read fails with the SVD's error, not a
/// refusal, rather than read the sites the pair left stale.
#[test]
fn a_read_fails_when_the_split_of_its_pending_pair_fails() {
    type Read<'a> = &'a dyn Fn(&mut CanonicalMps) -> Result<(), CpuMpsError>;
    let engine = Engine::new(FailingSvds::all());
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
            .expect("a fused gate takes no SVD");
        let error = read(&mut state).expect_err(name);
        assert!(
            matches!(&error, CpuMpsError::Factorization(_)),
            "{name}: {error:?}"
        );
        assert!(error.refusal().is_none(), "{name}");
    }
}
