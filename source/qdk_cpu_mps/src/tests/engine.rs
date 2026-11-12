// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the engine's rules, one file per contract area. This file
//! holds what several areas share: the engine with and without fusion,
//! a circuit run on the engine and on the exact reference, and
//! `Counting`, an algebra that counts the engine's QRs and products, for
//! the tests that pin where the engine moves the centre.

mod conformance;
mod failures;
mod fusion;
mod reads;
mod refusals;
mod retained_weight;
mod routing;
mod sampling;
mod statistics;
mod truncation;

use super::{c, chain, cnot, cz, entangler, exact, mixed_string, phase, ry, swap, z_string};
use crate::{
    CanonicalMps, CpuMpsEngine, CpuMpsError, DEFAULT_CUTOFF, SplitStatistics,
    engine::Engine,
    linear_algebra::{
        CpuLinearAlgebra, FactorizationError, LinearAlgebra, Matrix, MatrixRef, Qr, Svd,
    },
};
use num_complex::Complex64;
use std::{cell::Cell, rc::Rc};
use tensornet::{
    MpsEngine, MpsEngineError, MpsShape, Pauli, PauliSum, PauliTerm, Refusal,
    reference::{ExactReference, ReferenceState},
};

/// The engine with and without fusion, each with its setting's name.
fn both_fusions(cutoff: f64) -> [(CpuMpsEngine, &'static str); 2] {
    [
        (CpuMpsEngine::with_cutoff(cutoff), "fused"),
        (
            CpuMpsEngine::with_cutoff(cutoff).with_fusion(false),
            "unfused",
        ),
    ]
}

/// Applies the same circuit, with gates on sites far apart and in both
/// operand orders, to `engine` and to the reference, and returns both.
fn circuit(engine: &CpuMpsEngine, sites: usize) -> (CanonicalMps, ReferenceState) {
    let reference = ExactReference::new();
    let mut state = engine.prepare(&chain(sites, 64)).expect("prepare");
    let mut exact_state = reference.prepare(&exact(sites)).expect("prepare");
    let mut both = |on: &[usize], m: &[Complex64]| {
        engine.apply(&mut state, on, m).expect("apply");
        reference.apply(&mut exact_state, on, m).expect("apply");
    };
    for site in 0..sites {
        let x = f64::from(u32::try_from(site).expect("a small site"));
        both(&[site], &ry(0.3 + 0.2 * x));
        both(&[site], &phase(0.1 * x));
    }
    both(&[0, sites - 1], &cnot());
    both(&[sites - 2, 1], &entangler(0.9));
    both(&[2, 3], &entangler(-0.4));
    both(&[4, 2], &cnot());
    both(&[1], &ry(1.3));
    both(&[sites - 1, 0], &entangler(0.6));
    (state, exact_state)
}

/// Each site's P(One) on `state` and on the reference's `exact_state`
/// agree within `tolerance`.
fn assert_reads_agree(
    engine: &CpuMpsEngine,
    state: &CanonicalMps,
    exact_state: &ReferenceState,
    tolerance: f64,
) {
    let reference = ExactReference::new();
    for site in 0..exact_state.site_count() {
        let p = engine.probability_of_one(state, site).expect("P(One)");
        let q = reference
            .probability_of_one(exact_state, site)
            .expect("P(One)");
        assert!((p - q).abs() < tolerance, "site {site}: {p} vs {q}");
    }
}

/// The engines of both fusion settings at cutoffs 0 and the default.
fn both_fusions_and_cutoffs() -> impl Iterator<Item = (CpuMpsEngine, String)> {
    [0.0, DEFAULT_CUTOFF].into_iter().flat_map(|cutoff| {
        both_fusions(cutoff).map(|(engine, name)| (engine, format!("{name}, cutoff {cutoff:e}")))
    })
}

/// One round of a Bell pair on sites 0 and 1 truncated to a bond of 1: a
/// turn by π/2, a read of `centre` (0 or 1), which moves the centre there,
/// and a CNOT, whose split keeps half the weight and leaves the centre on
/// that side, forced by a read of the retained weight, which it returns.
fn halve_the_weight(
    engine: &CpuMpsEngine,
    state: &mut CanonicalMps,
    centre: usize,
) -> Result<f64, CpuMpsError> {
    engine.apply(state, &[0], &ry(std::f64::consts::FRAC_PI_2))?;
    engine.probability_of_one(state, centre)?;
    engine.apply(state, &[0, 1], &cnot())?;
    engine.retained_weight(state)
}

/// The Z⊗Z gate, which commutes with X on every site.
fn zz() -> Vec<Complex64> {
    let mut m = vec![c(0.0); 16];
    for (at, value) in [1.0, -1.0, -1.0, 1.0].into_iter().enumerate() {
        m[5 * at] = c(value);
    }
    m
}

/// How many products and QR decompositions a [`Counting`] algebra took.
#[derive(Default)]
struct Counts {
    products: Cell<usize>,
    qrs: Cell<usize>,
}

/// `CpuLinearAlgebra`, counting its products and QR decompositions.
struct Counting(Rc<Counts>);

impl LinearAlgebra for Counting {
    fn multiply(&self, a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
        self.0.products.set(self.0.products.get() + 1);
        CpuLinearAlgebra.multiply(a, b)
    }

    fn qr(&self, a: MatrixRef<'_>) -> Result<Qr, FactorizationError> {
        self.0.qrs.set(self.0.qrs.get() + 1);
        CpuLinearAlgebra.qr(a)
    }

    fn svd(&self, a: MatrixRef<'_>) -> Result<Svd, FactorizationError> {
        CpuLinearAlgebra.svd(a)
    }
}
