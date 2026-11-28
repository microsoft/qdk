// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's tests that span its parts, and what the tests of several
//! parts share. Each part's own rules are tested beside it
//! (`<part>/tests.rs`). Here, on the CPU algebra, [`CpuAlgebra`]: the
//! engine against the exact reference on every intrinsic gate and
//! projection, and on circuits of known outcome. Shared: the engine with
//! and without fusion, gates and observables (with those of
//! `engine::conformance`), a circuit run on the engine and on the exact
//! reference, and the spy [`SpyAlgebra`], which wraps [`CpuAlgebra`],
//! counts the engine's calls and records its uploads.

mod circuits_of_known_outcome;
mod exact_gates_and_projections;
pub(super) mod spy_algebra;

use crate::engine::conformance::{capacity, cnot, entangler, full_capacity, real, ry};
use crate::site_algebra::SiteAlgebra;
use crate::{
    CanonicalMpsEngine, CanonicalMpsState, CpuAlgebra, CpuMpsEngine as Engine, DEFAULT_CUTOFF,
};
use num_complex::Complex64;
use spy_algebra::{SpiedCalls, SpyAlgebra};
use std::rc::Rc;
use tensornet::reference::{ExactReference, ReferenceState};
use tensornet::{MpsEngine, Pauli, PauliSum, PauliTerm};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// The same engine, fusing when `fuses` (Fusion on
    /// [`CanonicalMpsEngine`]). The crate's users always get fusion. The
    /// tests compare with `with_fusion(false)`. It applies each two-site
    /// gate at once, as a window of its own. It leaves the centre on the
    /// side of its sites nearer where the centre was.
    #[must_use]
    pub(super) fn with_fusion(self, fuses: bool) -> Self {
        Self { fuses, ..self }
    }
}

/// A state of [`Engine`], the engine the tests run, on the CPU
/// algebra.
pub(super) type State = CanonicalMpsState<CpuAlgebra>;

/// Gates in the order they are applied, each with the sites it acts on.
pub(super) type Gates = Vec<(&'static [usize], Vec<Complex64>)>;

pub(super) fn phase(theta: f64) -> Vec<Complex64> {
    vec![
        real(1.0),
        real(0.0),
        real(0.0),
        Complex64::from_polar(1.0, theta),
    ]
}

pub(super) fn cz() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (at, value) in [1.0, 1.0, 1.0, -1.0].into_iter().enumerate() {
        m[5 * at] = real(value);
    }
    m
}

/// The Z⊗Z gate, which commutes with X⊗X.
pub(super) fn zz() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (at, value) in [1.0, -1.0, -1.0, 1.0].into_iter().enumerate() {
        m[5 * at] = real(value);
    }
    m
}

pub(super) fn z_string(sites: &[usize]) -> PauliSum {
    let factors = sites.iter().map(|&s| (s, Pauli::Z)).collect();
    PauliSum::new(
        real(0.0),
        vec![PauliTerm::new(real(1.0), factors).expect("a term")],
    )
    .expect("an observable")
}

/// The engine with and without fusion, each with its setting's name.
pub(super) fn both_fusions(cutoff: f64) -> [(Engine, &'static str); 2] {
    let engine = Engine::new(CpuAlgebra).with_cutoff(cutoff);
    [
        (engine.clone(), "fused"),
        (engine.with_fusion(false), "unfused"),
    ]
}

/// The engines of both fusion settings at cutoffs 0 and the default.
pub(super) fn both_fusions_and_cutoffs() -> impl Iterator<Item = (Engine, String)> {
    [0.0, DEFAULT_CUTOFF].into_iter().flat_map(|cutoff| {
        both_fusions(cutoff).map(|(engine, name)| (engine, format!("{name}, cutoff {cutoff:e}")))
    })
}

/// Applies the same circuit, with gates on sites far apart and in both
/// operand orders, to `engine` and to the exact reference, and returns
/// both.
pub(super) fn circuit(engine: &Engine, sites: usize) -> (State, ReferenceState) {
    let reference = ExactReference::new();
    let mut state = engine.prepare(&capacity(sites, 64)).expect("prepare");
    let mut exact_state = reference.prepare(&full_capacity(sites)).expect("prepare");
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

/// An engine on a spy algebra, a chain of `sites` sites prepared on it,
/// each turned and entangled with the next, and the spy's record of calls.
pub(super) fn spied_entangled_chain(
    sites: usize,
) -> (
    CanonicalMpsEngine<SpyAlgebra>,
    CanonicalMpsState<SpyAlgebra>,
    Rc<SpiedCalls>,
) {
    let (algebra, calls) = SpyAlgebra::new();
    let engine = CanonicalMpsEngine::new(algebra);
    let mut state = engine.prepare(&capacity(sites, 4)).expect("prepare");
    for site in 0..sites {
        engine.apply(&mut state, &[site], &ry(0.7)).expect("apply");
    }
    for cut in 0..sites - 1 {
        engine
            .apply(&mut state, &[cut, cut + 1], &cnot())
            .expect("apply");
    }
    (engine, state, calls)
}
