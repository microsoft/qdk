// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The case of refusals: each call with one fault refused exactly, with
//! the state unchanged.
//!
//! Exactly means the [`Refusal`] the reference makes, displayed word for
//! word. The faults include both sides of the unitarity tolerance, a
//! matrix with a NaN or an infinite entry, and a two-site matrix that is
//! not unitary in its last row only; `apply` of a unitary on three sites,
//! which an engine whose own path fails it, rather than refuse it, misses;
//! an expectation whose site out of range is the second factor of its
//! second term; and a site repeated after another in a condition and in a
//! sample, which an engine that compares only neighbours misses; and a
//! sample of zero shots of a site out of range or repeated, which an engine
//! that returns early on no shots misses. A matrix that is unitary to
//! rounding (a product of many gates) must be accepted.

use super::gates::{c, pauli_x, shear, toffoli};
use super::observables::z_on;
use super::twin::{Twin, describe, refused};
use super::{cnot, controlled_y, fused_gates, ry};
use crate::{MpsEngine, MpsShape, Pauli, PauliSum, PauliTerm, Refusal};

/// The refusal of each call with one fault, of every method, exactly, on a
/// state of two sites, and of `apply` on three sites on a state of three,
/// with the states unchanged after them; and the edges that are accepted.
pub(super) fn refusals<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let qutrit = describe("capacity", MpsShape::new(vec![vec![2, 1], vec![1, 3]]))?;
    refused(
        "a capacity with a qutrit",
        engine.prepare(&qutrit),
        &Refusal::NotTwoLevel { site: 1, levels: 3 },
    )?;

    let mut twin = Twin::new(engine, 2)?;
    twin.apply(&[0], &ry(0.7))?;
    twin.apply(&[0, 1], &controlled_y())?;
    refused_applies(engine, &mut twin.state)?;
    refused_three_site_apply(engine)?;
    let seed = 1;
    let out = Refusal::SiteOutOfRange { site: 2, sites: 2 };
    let repeated = Refusal::RepeatedSite { site: 1 };
    let state = &mut twin.state;
    refused(
        "P(One) of a site out of range",
        engine.probability_of_one(state, 2),
        &out,
    )?;
    refused(
        "P(One) given a site out of range",
        engine.conditional_probability_of_one(state, 0, &[(2, true)]),
        &out,
    )?;
    // A site repeated after another, so that a check that compares
    // neighbours only does not see it.
    refused(
        "P(One) given a repeated site",
        engine.conditional_probability_of_one(state, 0, &[(1, true), (0, false), (1, false)]),
        &repeated,
    )?;
    refused(
        "P(One) of a site out of range, given a site",
        engine.conditional_probability_of_one(state, 2, &[(0, true)]),
        &out,
    )?;
    refused(
        "a projection of a site out of range",
        engine.project(state, 2, false),
        &out,
    )?;
    refused(
        "an expectation on a site out of range",
        engine.expectation(state, &z_on(2)?),
        &out,
    )?;
    let later = vec![
        describe("term", PauliTerm::new(c(1.0, 0.0), vec![(0, Pauli::Z)]))?,
        describe(
            "term",
            PauliTerm::new(c(0.5, 0.0), vec![(1, Pauli::X), (2, Pauli::Z)]),
        )?,
    ];
    refused(
        "an expectation on a site out of range in a later term, as its later factor",
        engine.expectation(
            state,
            &describe("observable", PauliSum::new(c(0.0, 0.0), later))?,
        ),
        &out,
    )?;
    refused(
        "a sample of a site out of range",
        engine.sample(state, &[0, 2], 3, seed),
        &out,
    )?;
    refused(
        "a sample of a repeated site",
        engine.sample(state, &[1, 0, 1], 3, seed),
        &repeated,
    )?;
    refused(
        "no shots of a site out of range",
        engine.sample(state, &[0, 2], 0, seed),
        &out,
    )?;
    refused(
        "no shots of a repeated site",
        engine.sample(state, &[1, 0, 1], 0, seed),
        &repeated,
    )?;
    twin.compare("after the refusals")?;

    twin.apply(&[1, 0], &fused_gates(1000))?;
    twin.compare("after a product of a thousand gates")?;
    twin.apply(&[0], &shear(32))?;
    twin.compare("after a matrix just within the unitarity tolerance")
}

/// The refusal of each `apply` with one fault, exactly, on a state of two
/// sites.
fn refused_applies<E: MpsEngine>(engine: &E, state: &mut E::State) -> Result<(), String> {
    let out = Refusal::SiteOutOfRange { site: 2, sites: 2 };
    let projector = [c(1.0, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)];
    refused(
        "apply on no site",
        engine.apply(state, &[], &[c(1.0, 0.0)]),
        &Refusal::NoSites,
    )?;
    refused(
        "apply on a site out of range",
        engine.apply(state, &[2], &pauli_x()),
        &out,
    )?;
    refused(
        "apply on a repeated site",
        engine.apply(state, &[1, 1], &cnot()),
        &Refusal::RepeatedSite { site: 1 },
    )?;
    refused(
        "apply of a matrix of the wrong length",
        engine.apply(state, &[0], &cnot()),
        &Refusal::MatrixLength {
            sites: 1,
            expected: 4,
            actual: 16,
        },
    )?;
    refused(
        "apply of a projector",
        engine.apply(state, &[0], &projector),
        &Refusal::NotUnitary { deviation: 1.0 },
    )?;
    let one = c(1.0, 0.0);
    let mut two_site_projector = vec![c(0.0, 0.0); 16];
    for at in 0..3 {
        two_site_projector[at * 5] = one;
    }
    refused(
        "apply of a two-site matrix whose last row is zero",
        engine.apply(state, &[0, 1], &two_site_projector),
        &Refusal::NotUnitary { deviation: 1.0 },
    )?;
    refused(
        "apply of a matrix with a NaN entry",
        engine.apply(
            state,
            &[1],
            &[c(f64::NAN, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(1.0, 0.0)],
        ),
        &Refusal::NotUnitary {
            deviation: f64::NAN,
        },
    )?;
    refused(
        "apply of a matrix with an infinite entry",
        engine.apply(
            state,
            &[1],
            &[c(f64::INFINITY, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(1.0, 0.0)],
        ),
        &Refusal::NotUnitary {
            deviation: f64::NAN,
        },
    )?;
    refused(
        "apply of a matrix just beyond the unitarity tolerance",
        engine.apply(state, &[0], &shear(30)),
        &Refusal::NotUnitary {
            deviation: 2f64.powi(-29),
        },
    )
}

/// The refusal of `apply` on three sites, exactly, on a state of three
/// sites, with the state unchanged after it: a Toffoli, unitary and of the
/// right length on distinct sites in range, so that its number of sites is
/// its only fault, as when a method passes a gate it did not decompose.
fn refused_three_site_apply<E: MpsEngine>(engine: &E) -> Result<(), String> {
    let mut twin = Twin::new(engine, 3)?;
    twin.apply(&[0], &ry(0.7))?;
    twin.apply(&[0, 1], &cnot())?;
    twin.apply(&[1, 2], &controlled_y())?;
    refused(
        "apply of a Toffoli on three sites",
        engine.apply(&mut twin.state, &[2, 0, 1], &toffoli()),
        &Refusal::TooManySites { sites: 3 },
    )?;
    twin.compare("after the refusal on three sites")
}
