// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The state an engine that truncates must hold: [`CappedTwin`] holds the
//! reference truncated as the engine must truncate; with the probe that
//! tells whether the engine truncates.
//!
//! The kit assumes the engine keeps every squared Schmidt value of at least
//! 1e-9 of its cut's weight (the case `shape` checks one such value), so
//! that its cutoff drops none of the values the kit's states hold. A cutoff
//! that bounds the weight each split discards instead, its routing's splits
//! included, to at most 10⁻¹⁴ of it (a common default), lowers the retained
//! weight by at most that much a split, far within [`TOLERANCE`] over a
//! case's splits; it moves a read only where it drops a Schmidt value that
//! small, which the kit's states, in the caller's sites, hold only as
//! rounding.

use super::conditionals::compare_conditionals;
use super::sampling::{SHOTS, sampled};
use super::twin::{REFERENCE, close, compare_reads, describe, unsupported};
use super::{TOLERANCE, cnot, normalized, ry, truncated};
use crate::reference::ReferenceState;
use crate::{MpsEngine, MpsShape};
use num_complex::Complex64;

/// The capacity of two sites with a bond of one.
pub(super) fn cap_of_one() -> Result<MpsShape, String> {
    describe("capacity", MpsShape::qubit_chain(2, 1))
}

/// Whether the engine truncates, decided by the same probe for every
/// capped state, not by that state's own chain: by whether its chain still
/// fits a cap of one after an entangling gate on two sites. Decided by
/// each state's own chain, an engine that ignored the cap of one cut,
/// where another cut's cap was larger, would be taken there for one that
/// does not truncate.
fn engine_truncates<E: MpsEngine>(engine: &E) -> Result<bool, String> {
    let capacity = cap_of_one()?;
    let mut state = describe("engine prepare", engine.prepare(&capacity))?;
    describe("engine apply", engine.apply(&mut state, &[0], &ry(1.6)))?;
    describe("engine apply", engine.apply(&mut state, &[0, 1], &cnot()))?;
    let shape = describe("engine shape", engine.shape(&state))?;
    Ok(shape.fits_within(&capacity))
}

/// `state` truncated, at every cut whose Schmidt rank exceeds `capacity`'s
/// bond, to that bond ([`truncated`]), cut after cut in increasing order,
/// or in decreasing order when `decreasing`: `None` when every rank fits.
/// A truncation never raises the rank at another cut, so the result fits.
fn truncated_to_capacity(
    state: &ReferenceState,
    capacity: &MpsShape,
    decreasing: bool,
) -> Result<Option<ReferenceState>, String> {
    let cuts = capacity.site_count().saturating_sub(1);
    let order: Vec<usize> = if decreasing {
        (0..cuts).rev().collect()
    } else {
        (0..cuts).collect()
    };
    let mut kept: Option<ReferenceState> = None;
    for cut in order {
        let current = kept.as_ref().unwrap_or(state);
        let ranks = describe("reference shape", REFERENCE.shape(current))?;
        if let (Some(rank), Some(bond)) = (ranks.bond_dim(cut), capacity.bond_dim(cut))
            && rank > bond
        {
            kept = Some(truncated(current, cut, bond)?);
        }
    }
    Ok(kept)
}

/// Whether two states of the kit agree, amplitude by amplitude, to
/// rounding.
fn same_state(a: &ReferenceState, b: &ReferenceState) -> bool {
    a.site_count() == b.site_count()
        && (0..1 << a.site_count()).all(|i| (a.amplitude(i) - b.amplitude(i)).norm() < 1e-12)
}

/// An engine state under a capacity that truncates, and `held`, the
/// reference state the engine must hold: after each gate, at every cut
/// whose Schmidt rank then exceeds the capacity's bond, in increasing cut
/// order, its largest Schmidt pairs at that cut, as many as the bond, not
/// renormalized ([`truncated`]); or the exact state, on an engine that
/// does not truncate ([`engine_truncates`]).
///
/// After a gate on one site or on two neighbours, only the gate's cut can
/// exceed its bond, so the state the engine must keep is unique. After a
/// gate on distant sites or on three, the kit asks a state only where the
/// truncations at the cuts over their bonds give the same state in
/// increasing and in decreasing cut order, so that an engine's routing
/// cannot change it ([`CappedTwin::apply`] asserts it); elsewhere it checks
/// only what every routing keeps ([`CappedTwin::apply_beyond_capacity`]).
///
/// Every gate is read after, so that every engine, whatever it fuses or
/// defers, truncates at the cut and the moment the kit does. Every engine
/// the kit checks truncates at a cut as a singular value decomposition of
/// the chain in canonical form there does, keeping the largest values: so
/// does every engine that keeps its chain canonical around the gate it
/// applies.
pub(super) struct CappedTwin<'e, E: MpsEngine> {
    engine: &'e E,
    state: E::State,
    pub(super) held: ReferenceState,
    pub(super) capacity: MpsShape,
    /// Whether the engine truncates ([`engine_truncates`]).
    truncates: bool,
    /// Whether the engine's chain must be `held`'s Schmidt ranks: until a
    /// truncation or a projection, after which an engine may hold more.
    exact: bool,
    /// The retained weight the engine last reported.
    weight: f64,
}

impl<'e, E: MpsEngine> CappedTwin<'e, E> {
    pub(super) fn new(engine: &'e E, capacity: MpsShape) -> Result<Self, String> {
        Ok(Self {
            engine,
            state: describe("engine prepare", engine.prepare(&capacity))?,
            held: describe("reference prepare", REFERENCE.prepare(&capacity))?,
            capacity,
            truncates: engine_truncates(engine)?,
            exact: true,
            weight: 1.0,
        })
    }

    /// Applies `matrix` to `sites` of both states, truncates `held` as the
    /// engine must, and [`check`](Self::check)s the engine.
    pub(super) fn apply(
        &mut self,
        sites: &[usize],
        matrix: &[Complex64],
        when: &str,
    ) -> Result<(), String> {
        describe(
            &format!("{when}: engine apply on {sites:?}"),
            self.engine.apply(&mut self.state, sites, matrix),
        )?;
        self.follow(sites, matrix, when)
    }

    /// As [`apply`](Self::apply), but an engine may fail the gate, with no
    /// refusal, when it acts on more than two sites ([`unsupported`]):
    /// `false` then, and the engine's state, which a failure leaves
    /// unspecified, must not be used again.
    pub(super) fn apply_if_supported(
        &mut self,
        sites: &[usize],
        matrix: &[Complex64],
        when: &str,
    ) -> Result<bool, String> {
        match self.engine.apply(&mut self.state, sites, matrix) {
            Ok(()) => self.follow(sites, matrix, when).map(|()| true),
            Err(error) => unsupported(&error, sites, when).map(|()| false),
        }
    }

    /// Applies to `held` the gate the engine applied, truncates it as the
    /// engine must, and [`check`](Self::check)s the engine.
    ///
    /// # Panics
    ///
    /// When the gate acts on distant sites or on three, and the truncations
    /// its result needs give one state in increasing cut order and another
    /// in decreasing order: the kit's cases avoid such a gate, since the
    /// state an engine keeps would then depend on its routing.
    fn follow(&mut self, sites: &[usize], matrix: &[Complex64], when: &str) -> Result<(), String> {
        describe(
            "reference apply",
            REFERENCE.apply(&mut self.held, sites, matrix),
        )?;
        if self.truncates
            && let Some(kept) = truncated_to_capacity(&self.held, &self.capacity, false)?
        {
            let neighbours = match *sites {
                [_] => true,
                [a, b] => a.abs_diff(b) == 1,
                _ => false,
            };
            if !neighbours {
                let other = truncated_to_capacity(&self.held, &self.capacity, true)?
                    .expect("a state that does not fit truncates in either order");
                assert!(
                    same_state(&kept, &other),
                    "the kit asks the state a gate on {sites:?} keeps only where the order of \
                     its truncations does not change it"
                );
            }
            self.held = kept;
            self.exact = false;
        }
        self.check(when)
    }

    /// Applies a gate whose result does not fit the capacity, where the
    /// state an engine keeps may depend on its routing, so the kit asks no
    /// state; and checks what every engine that truncates keeps: a chain
    /// that fits the capacity, and a retained weight above zero and below
    /// the last one by more than [`TOLERANCE`]. An engine that does not
    /// truncate is [`check`](Self::check)ed against the exact state. An
    /// engine may fail a gate on more than two sites ([`unsupported`]).
    ///
    /// The weight must fall because a chain that fits the capacity holds,
    /// at a cut whose Schmidt rank exceeds its bond, a state at a squared
    /// distance of at least the tail the bond drops from the exact one
    /// (Eckart and Young); and truncations, orthogonal projections
    /// interleaved with the swaps of a routing, that drop a fraction
    /// `1 − w` of the weight in all, in `m` steps, leave it within a squared
    /// distance of `m·(1 − w)`. The kit's gates drop a tail of more than
    /// 7·10⁻³ (7.5·10⁻³ at the inner cut after the gate on two sites), so
    /// an engine would need over 10⁶ truncations to keep the weight within
    /// [`TOLERANCE`].
    ///
    /// # Panics
    ///
    /// When the gate leaves a state that fits the capacity: the kit's cases
    /// use this only for gates that do not.
    pub(super) fn apply_beyond_capacity(
        mut self,
        sites: &[usize],
        matrix: &[Complex64],
        when: &str,
    ) -> Result<(), String> {
        if let Err(error) = self.engine.apply(&mut self.state, sites, matrix) {
            return unsupported(&error, sites, when);
        }
        describe(
            "reference apply",
            REFERENCE.apply(&mut self.held, sites, matrix),
        )?;
        let ranks = describe("reference shape", REFERENCE.shape(&self.held))?;
        assert!(
            !ranks.fits_within(&self.capacity),
            "the kit's gates beyond the capacity leave a state that does not fit it"
        );
        if !self.truncates {
            return self.check(when);
        }
        let weight = self.engine_weight(when)?;
        if weight > self.weight - TOLERANCE {
            return Err(format!(
                "{when}: the retained weight is {weight}, as before the gate ({}), though \
                 no state the capacity holds keeps all of the state the gate leaves",
                self.weight
            ));
        }
        let actual = describe("engine shape", self.engine.shape(&self.state))?;
        self.fits(&actual, when)
    }

    /// The engine's retained weight, refused when it is NaN or not above
    /// zero, or when it rose above the last one by more than [`TOLERANCE`].
    fn engine_weight(&self, when: &str) -> Result<f64, String> {
        let weight = describe(
            "engine retained weight",
            self.engine.retained_weight(&self.state),
        )?;
        if weight.is_nan() || weight <= 0.0 {
            return Err(format!("{when}: the retained weight fell to {weight}"));
        }
        if weight > self.weight + TOLERANCE {
            return Err(format!(
                "{when}: the retained weight rose from {} to {weight}",
                self.weight
            ));
        }
        Ok(weight)
    }

    /// Refuses the engine's chain `actual` when the engine truncates and
    /// the chain does not fit the capacity.
    fn fits(&self, actual: &MpsShape, when: &str) -> Result<(), String> {
        if self.truncates && !actual.fits_within(&self.capacity) {
            return Err(format!(
                "{when}: the chain {actual:?} does not fit the capacity {:?}",
                self.capacity
            ));
        }
        Ok(())
    }

    /// A fork of both states, [`check`](Self::check)ed: the fork of a
    /// truncated state holds the same state, with the same retained weight.
    pub(super) fn fork(&self, when: &str) -> Result<Self, String> {
        let mut copy = Self {
            engine: self.engine,
            state: describe(
                &format!("{when}: engine fork"),
                self.engine.fork(&self.state),
            )?,
            held: self.held.clone(),
            capacity: self.capacity.clone(),
            truncates: self.truncates,
            exact: self.exact,
            weight: self.weight,
        };
        copy.check(when)?;
        Ok(copy)
    }

    /// Projects both states, checks the probability the engine returns,
    /// and [`check`](Self::check)s the engine.
    pub(super) fn project(&mut self, site: usize, one: bool, when: &str) -> Result<(), String> {
        let expected = describe(
            "reference project",
            REFERENCE.project(&mut self.held, site, one),
        )?;
        let actual = describe(
            &format!("{when}: engine project"),
            self.engine.project(&mut self.state, site, one),
        )?;
        close(
            &format!("{when}: the probability the projection returned"),
            actual,
            expected,
        )?;
        self.exact = false;
        self.check(when)
    }

    /// Checks, in this order, the retained weight (above zero, not above
    /// the last one, and the weight of `held`), the chain, and P(One) of
    /// every site, which must be `held`'s normalized.
    ///
    /// The chain is `held`'s Schmidt ranks while it must be exact; then, at
    /// each cut, at least `held`'s rank, and within the capacity on an
    /// engine that truncates.
    pub(super) fn check(&mut self, when: &str) -> Result<(), String> {
        let weight = self.engine_weight(when)?;
        let kept = describe(
            "reference retained weight",
            REFERENCE.retained_weight(&self.held),
        )?;
        close(&format!("{when}: the retained weight"), weight, kept)?;
        self.weight = weight;
        let actual = describe("engine shape", self.engine.shape(&self.state))?;
        let ranks = describe("reference shape", REFERENCE.shape(&self.held))?;
        if self.exact {
            if actual != ranks {
                return Err(format!("{when}: the chain is {actual:?}, not {ranks:?}"));
            }
        } else {
            self.fits(&actual, when)?;
            for cut in 0..ranks.site_count().saturating_sub(1) {
                let (bond, rank) = (actual.bond_dim(cut), ranks.bond_dim(cut));
                if bond < rank {
                    return Err(format!(
                        "{when}: the bond at cut {cut} is {bond:?}, below the Schmidt rank \
                         {rank:?}"
                    ));
                }
            }
        }
        let expected = normalized(&self.held);
        for site in 0..expected.site_count() {
            let wanted = describe("reference", REFERENCE.probability_of_one(&expected, site))?;
            let actual = describe(
                &format!("engine P(One) of site {site}"),
                self.engine.probability_of_one(&self.state, site),
            )?;
            close(&format!("{when}: P(One) of site {site}"), actual, wanted)?;
        }
        Ok(())
    }

    /// Every read of the state: P(One) of each site, every Pauli string's
    /// expectation and their weighted sum, every conditional probability,
    /// and the frequencies of [`SHOTS`] samples of every site, all of
    /// `held` normalized.
    pub(super) fn compare(&self, when: &str) -> Result<(), String> {
        let expected = normalized(&self.held);
        compare_reads(self.engine, &self.state, &expected, when)?;
        compare_conditionals(self.engine, &self.state, &expected, when)?;
        let sites: Vec<usize> = (0..expected.site_count()).rev().collect();
        sampled(self.engine, &self.state, &expected, &sites, SHOTS, 5, when).map(drop)
    }
}
