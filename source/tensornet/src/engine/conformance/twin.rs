// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine beside the reference: [`Twin`] runs each operation on both
//! and compares what they read; with the comparisons and the scrambled
//! state the cases share.

use super::observables::{name, pauli_strings, weighted_sum};
use super::{TOLERANCE, cnot, controlled_y, ry, s_h};
use crate::reference::{ExactReference, ReferenceState};
use crate::{MpsEngine, MpsEngineError, MpsShape, Refusal};
use num_complex::Complex64;
use std::fmt;

/// One engine state and the reference state it must match.
pub(super) struct Twin<'e, E: MpsEngine> {
    pub(super) engine: &'e E,
    pub(super) state: E::State,
    pub(super) reference: ReferenceState,
    capacity: MpsShape,
}

pub(super) const REFERENCE: ExactReference = ExactReference;

pub(super) fn describe<T>(what: &str, result: Result<T, impl fmt::Display>) -> Result<T, String> {
    result.map_err(|e| format!("{what}: {e}"))
}

impl<'e, E: MpsEngine> Twin<'e, E> {
    /// A twin of `sites` sites whose capacity never truncates.
    pub(super) fn new(engine: &'e E, sites: usize) -> Result<Self, String> {
        Self::with_capacity(
            engine,
            &describe("capacity", MpsShape::qubit_chain(sites, usize::MAX))?,
        )
    }

    fn with_capacity(engine: &'e E, capacity: &MpsShape) -> Result<Self, String> {
        Ok(Self {
            engine,
            state: describe("engine prepare", engine.prepare(capacity))?,
            reference: describe("reference prepare", REFERENCE.prepare(capacity))?,
            capacity: capacity.clone(),
        })
    }

    pub(super) fn apply(&mut self, sites: &[usize], matrix: &[Complex64]) -> Result<(), String> {
        describe(
            "reference apply",
            REFERENCE.apply(&mut self.reference, sites, matrix),
        )?;
        describe(
            &format!("engine apply on {sites:?}"),
            self.engine.apply(&mut self.state, sites, matrix),
        )
    }

    pub(super) fn fork(&self) -> Result<Self, String> {
        Ok(Self {
            engine: self.engine,
            state: describe("engine fork", self.engine.fork(&self.state))?,
            reference: describe("reference fork", REFERENCE.fork(&self.reference))?,
            capacity: self.capacity.clone(),
        })
    }

    /// Projects both states, and checks the probability the engine
    /// returns.
    pub(super) fn project(&mut self, site: usize, one: bool) -> Result<(), String> {
        let expected = describe(
            "reference project",
            REFERENCE.project(&mut self.reference, site, one),
        )?;
        let what = format!("the projection of site {site} on {one}");
        let actual = describe(&what, self.engine.project(&mut self.state, site, one))?;
        close(
            &format!("the probability {what} returned"),
            actual,
            expected,
        )
    }

    /// Compares P(One) of every site and every Pauli string's expectation.
    pub(super) fn compare(&self, when: &str) -> Result<(), String> {
        compare_reads(self.engine, &self.state, &self.reference, when)
    }

    /// Compares the engine's chain with the reference's, whose bonds are
    /// the Schmidt ranks: after gates only.
    pub(super) fn same_shape(&self, when: &str) -> Result<(), String> {
        let actual = describe("engine shape", self.engine.shape(&self.state))?;
        let expected = describe("reference shape", REFERENCE.shape(&self.reference))?;
        if actual == expected {
            Ok(())
        } else {
            Err(format!("{when}: the chain is {actual:?}, not {expected:?}"))
        }
    }

    /// Checks that the engine's chain fits the capacity and holds, at each
    /// cut, at least the reference's bond, the Schmidt rank: after a
    /// projection, where an engine that does not compress again may hold
    /// more than the rank, but never less, on a capacity that caps no bond.
    pub(super) fn bonds_between_rank_and_capacity(&self, when: &str) -> Result<(), String> {
        let actual = describe("engine shape", self.engine.shape(&self.state))?;
        if !actual.fits_within(&self.capacity) {
            return Err(format!(
                "{when}: the chain {actual:?} does not fit the capacity {:?}",
                self.capacity
            ));
        }
        let exact = describe("reference shape", REFERENCE.shape(&self.reference))?;
        for cut in 0..exact.site_count().saturating_sub(1) {
            let (held, rank) = (actual.bond_dim(cut), exact.bond_dim(cut));
            if held < rank {
                return Err(format!(
                    "{when}: the bond at cut {cut} is {held:?}, below the Schmidt rank {rank:?}"
                ));
            }
        }
        Ok(())
    }
}

/// Compares the engine's P(One) of every site, every Pauli string's
/// expectation, and the expectation of their [`weighted_sum`], with the
/// reference's, on a normalized `expected`.
pub(super) fn compare_reads<E: MpsEngine>(
    engine: &E,
    state: &E::State,
    expected: &ReferenceState,
    when: &str,
) -> Result<(), String> {
    for site in 0..expected.site_count() {
        let wanted = describe("reference", REFERENCE.probability_of_one(expected, site))?;
        let actual = describe(
            &format!("engine P(One) of site {site}"),
            engine.probability_of_one(state, site),
        )?;
        close(&format!("{when}: P(One) of site {site}"), actual, wanted)?;
    }
    for observable in pauli_strings(expected.site_count()) {
        let wanted = describe("reference", REFERENCE.expectation(expected, &observable))?;
        let actual = describe("engine expectation", engine.expectation(state, &observable))?;
        close_complex(&format!("{when}: ⟨{}⟩", name(&observable)), actual, wanted)?;
    }
    let observable = describe("observable", weighted_sum(expected.site_count()))?;
    let wanted = describe("reference", REFERENCE.expectation(expected, &observable))?;
    let actual = describe("engine expectation", engine.expectation(state, &observable))?;
    close_complex(
        &format!("{when}: the weighted sum of every Pauli string"),
        actual,
        wanted,
    )
}

/// `actual` within [`TOLERANCE`] of `expected`; written so that a NaN is
/// not.
pub(super) fn close(what: &str, actual: f64, expected: f64) -> Result<(), String> {
    if (actual - expected).abs() <= TOLERANCE {
        Ok(())
    } else {
        Err(format!("{what} is {actual}, not {expected}"))
    }
}

/// [`close`] for a complex read: within [`TOLERANCE`] in modulus, and not
/// NaN.
pub(super) fn close_complex(
    what: &str,
    actual: Complex64,
    expected: Complex64,
) -> Result<(), String> {
    if (actual - expected).norm() <= TOLERANCE {
        Ok(())
    } else {
        Err(format!("{what} is {actual}, not {expected}"))
    }
}

/// Checks that `result` is refused with exactly `expected`: an error that
/// exposes it, and displays as it.
pub(super) fn refused<T, X: MpsEngineError>(
    what: &str,
    result: Result<T, X>,
    expected: &Refusal,
) -> Result<(), String> {
    let Err(error) = result else {
        return Err(format!(
            "{what} was accepted, not refused with `{expected}`"
        ));
    };
    match error.refusal() {
        None => Err(format!(
            "{what} failed (`{error}`) instead of being refused with `{expected}`"
        )),
        // Compared by their debug text, so that a NaN equals a NaN.
        Some(refusal) if format!("{refusal:?}") != format!("{expected:?}") => Err(format!(
            "{what} was refused with `{refusal}`, not `{expected}`"
        )),
        Some(_) if error.to_string() != expected.to_string() => Err(format!(
            "{what} displays as `{error}`, not as its refusal `{expected}`"
        )),
        Some(_) => Ok(()),
    }
}

/// Three sites in a state with no symmetry to hide a layout error: complex
/// one-site gates, a gate on non-adjacent sites, and gates whose first site
/// is the higher one.
pub(super) fn scrambled<E: MpsEngine>(engine: &E) -> Result<Twin<'_, E>, String> {
    let mut twin = Twin::new(engine, 3)?;
    twin.apply(&[0], &ry(0.7))?;
    twin.apply(&[1], &s_h())?;
    twin.apply(&[0, 2], &cnot())?;
    twin.apply(&[2, 1], &controlled_y())?;
    twin.apply(&[2], &ry(-1.1))?;
    twin.apply(&[1, 0], &cnot())?;
    Ok(twin)
}
