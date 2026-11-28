// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The site steps of a grouped run: how a group's state takes each step of
//! a batch, through the engine.
//!
//! [`GroupedRun`]'s loop runs each [`SiteStep`] here: a gate is applied, a
//! measurement or a reset drawn, and the group split where its shots draw
//! both values. The group's state is prepared at its first step, and the
//! run's report keeps the largest bond dimension and the least retained
//! weight.

use crate::qir::{QirGate, QirOutcome, QirReadout};
use tensornet::MpsEngine;

use crate::mps_simulator::lowering::SiteGate;
#[cfg(doc)]
use crate::mps_simulator::lowering::SiteStep;

use super::GroupedRun;
use super::first_failure::{GroupFailure, engine_failure};
use super::shot_group::{ShotGroup, value_going_on};

impl<E: MpsEngine> GroupedRun<'_, E> {
    /// Applies `gate` to `group`'s state, prepared at the group's first
    /// step: one engine call per gate; an engine that gains by fusing gates
    /// fuses them itself.
    pub(super) fn apply(
        &self,
        group: &mut ShotGroup<E::State>,
        gate: &SiteGate,
    ) -> Result<(), GroupFailure<E::Error>> {
        let state = self.prepare(&mut group.state)?;
        group.bond_unread = true;
        self.engine
            .apply(state, gate.sites(), gate.matrix.row_major())
            .map_err(engine_failure)
    }

    /// Draws the step's site for each shot of `group`, against the
    /// probability of One read once for the group, and takes the draws
    /// ([`Self::take_draws`]). It reads the largest bond dimension first
    /// ([`Self::read_bond`]), as projecting may shrink it.
    ///
    /// # Errors
    ///
    /// The engine's refusal or failure on `group`'s state, which fails
    /// `group`.
    pub(super) fn draw(
        &mut self,
        group: &mut ShotGroup<E::State>,
        step: DrawStep,
    ) -> Result<(), GroupFailure<E::Error>> {
        self.read_bond(group)?;
        let probability_of_one = self.probability_of_one(group, step.site)?;
        let draws = group.draw_each_shot(probability_of_one);
        self.take_draws(group, &draws, step)
    }

    /// Keeps the value `group`'s shots drew at the step's site, one draw
    /// per shot in `draws` (One when true), in the group's state, prepared
    /// by then ([`Self::draw`] reads P(One) from it). The step's result, if
    /// any, is answered with that value. When the shots drew both values,
    /// the group splits first ([`Self::split`]): the fewer value's shots go
    /// on as `group`.
    ///
    /// For instance, five shots that drew One, Zero, Zero, One, Zero:
    ///
    /// ```text
    ///  shots     s0 s1 s2 s3 s4      P(One) read from ψ
    ///  draws      1  0  0  1  0      One is the fewer: 2 of 5
    ///  group     s0 s3, on ψ projected on One, goes on
    ///  waiting   s1 s2 s4, on a fork of ψ projected on Zero, pushed
    /// ```
    ///
    /// A reset returns the site to |0⟩ after the projection.
    ///
    /// # Errors
    ///
    /// The engine's refusal or failure on `group`'s state, which fails
    /// `group`; the failure to settle the fork fails only the shots that
    /// wait ([`Self::split`]).
    fn take_draws(
        &mut self,
        group: &mut ShotGroup<E::State>,
        draws: &[bool],
        step: DrawStep,
    ) -> Result<(), GroupFailure<E::Error>> {
        let one = value_going_on(draws);
        if draws.iter().any(|&drew| drew != one) {
            self.split(group, draws, one, step)?;
        }
        let state = group
            .state
            .as_mut()
            .expect("a group that draws holds a state");
        self.settle(state, step.site, one, step.reset)?;
        group.outcomes.extend(answer(step.result, one));
        Ok(())
    }

    /// The probability of One at `site` in `group`'s state. Prepares the
    /// state when this is the group's first step.
    fn probability_of_one(
        &self,
        group: &mut ShotGroup<E::State>,
        site: usize,
    ) -> Result<f64, GroupFailure<E::Error>> {
        let state = self.prepare(&mut group.state)?;
        self.engine
            .probability_of_one(state, site)
            .map_err(engine_failure)
    }

    /// Splits `group` by its shots' `draws` of the step's site. The shots
    /// that drew `one` stay in `group`. The others wait, on a fork of the
    /// state that keeps their value and a copy of the context.
    ///
    /// The fork is made after P(One) is read and before `group`'s state is
    /// settled. So it copies the state a shot alone would project.
    ///
    /// When settling the fork fails, `group`'s state is untouched. So the
    /// failure fails only the waiting shots, kept as the run's failure.
    /// `group` goes on, unless its shots all come after them.
    ///
    /// # Errors
    ///
    /// The engine's failure to fork. It fails all of `group`'s shots,
    /// before they split. The `MpsEngine` contract leaves the state it
    /// failed on unspecified, so no group goes on with it.
    fn split(
        &mut self,
        group: &mut ShotGroup<E::State>,
        draws: &[bool],
        one: bool,
        step: DrawStep,
    ) -> Result<(), GroupFailure<E::Error>> {
        let state = group
            .state
            .as_ref()
            .expect("a group that draws holds a state");
        let mut fork = self.engine.fork(state).map_err(engine_failure)?;
        let waiting = group.keep_shots_that_drew(draws, one);
        match self.settle(&mut fork, step.site, !one, step.reset) {
            Ok(()) => {
                let other_answer = answer(step.result, !one);
                let split_off = group.split_off(waiting, fork, other_answer);
                self.waiting.push(split_off);
            }
            // Only the waiting shots met the failure, on the fork, which is
            // dropped. The shots going on run on, unless they all come
            // after it: `run_group` then stops them.
            Err(failure) => self.failure.keep(waiting[0].place, failure),
        }
        Ok(())
    }

    /// Keeps the value of `site` in `state`, One when `one`, by projecting
    /// on it. Then, for a reset that kept One, flips `site` back to |0⟩.
    fn settle(
        &self,
        state: &mut E::State,
        site: usize,
        one: bool,
        reset: bool,
    ) -> Result<(), GroupFailure<E::Error>> {
        self.engine
            .project(state, site, one)
            .map_err(engine_failure)?;
        if reset && one {
            let x_gate = QirGate::X.matrix_at(None);
            self.engine
                .apply(state, &[site], x_gate.row_major())
                .map_err(engine_failure)?;
        }
        Ok(())
    }

    /// Keeps the largest bond dimension of `group`'s state in the report,
    /// if gates were applied since it was last read. So it is read where a
    /// sequence of gates ends, before the draws that may shrink it.
    pub(super) fn read_bond(
        &mut self,
        group: &mut ShotGroup<E::State>,
    ) -> Result<(), GroupFailure<E::Error>> {
        if let (true, Some(state)) = (group.bond_unread, &group.state) {
            let bond = self
                .engine
                .shape(state)
                .map_err(engine_failure)?
                .largest_bond_dimension();
            self.report.largest_bond_dimension = self.report.largest_bond_dimension.max(bond);
            group.bond_unread = false;
        }
        Ok(())
    }

    /// The group's state, prepared within the run's capacity when this is
    /// the group's first step. A program's first step that needs a state
    /// prepares it, so a program of no qubits never does.
    fn prepare<'s>(
        &self,
        state: &'s mut Option<E::State>,
    ) -> Result<&'s mut E::State, GroupFailure<E::Error>> {
        if state.is_none() {
            let capacity = self
                .capacity
                .as_ref()
                .expect("a call names a qubit, so the program has qubits");
            *state = Some(self.engine.prepare(capacity).map_err(engine_failure)?);
        }
        Ok(state.as_mut().expect("prepared above"))
    }

    /// Keeps the retained weight of the state of a group whose context
    /// ended, if it has a state, when it is the least so far.
    pub(super) fn end(&mut self, state: Option<&E::State>) -> Result<(), GroupFailure<E::Error>> {
        if let Some(state) = state {
            let weight = self.engine.retained_weight(state).map_err(engine_failure)?;
            let least = &mut self.report.least_retained_weight;
            *least = least.min(weight);
        }
        Ok(())
    }
}

/// A measurement or a reset, as a group draws it: the site drawn, whether
/// it returns to |0⟩ after, and the result the value answers, if any.
#[derive(Clone, Copy)]
pub(super) struct DrawStep {
    /// The site drawn.
    site: usize,
    /// Whether the site returns to |0⟩ after the projection.
    reset: bool,
    /// The result the value answers; `None` for a reset.
    result: Option<u32>,
}

impl DrawStep {
    /// The measurement of `site` into `result`, returning the site to |0⟩
    /// after when `reset`.
    pub(super) fn measurement(site: usize, result: u32, reset: bool) -> Self {
        Self {
            site,
            reset,
            result: Some(result),
        }
    }

    /// The reset of `site`, which answers no result.
    pub(super) fn reset_only(site: usize) -> Self {
        Self {
            site,
            reset: true,
            result: None,
        }
    }
}

/// The outcome that answers `result` with One (when `one`) or Zero. A
/// measurement has a result to answer; a reset has none.
fn answer(result: Option<u32>, one: bool) -> Option<QirOutcome> {
    result.map(|result| QirOutcome {
        result,
        value: if one {
            QirReadout::One
        } else {
            QirReadout::Zero
        },
    })
}

#[cfg(test)]
mod tests;
