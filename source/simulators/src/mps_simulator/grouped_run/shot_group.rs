// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The shot group: the shots of a run that drew the same values so far, and
//! so share one machine context and one state.
//!
//! [`GroupedRun`] runs each group through the phases of a run. A group
//! draws for each of its shots (`reads_one`), and splits off the shots
//! that drew the other value. Which value goes on is the rule that bounds
//! the states a run holds at once (`value_going_on`).

use std::rc::Rc;

use crate::qir::{QirOutcome, QirShotContext, QirShotId};
use tensornet::sampling::SplitMix64;

#[cfg(doc)]
use super::GroupedRun;
use crate::mps_simulator::lowering::SiteStep;
use crate::mps_simulator::shot_rng::shot_rng;

/// The shots of one group, which drew the same values so far and so share
/// one context and one state.
pub(super) struct ShotGroup<S> {
    /// The group's shots, in the order of the run's shots.
    pub(super) shots: Vec<Shot>,
    /// The machine context the shots share, between two batches: where the
    /// program is, its registers, and what it recorded.
    pub(super) context: QirShotContext,
    /// `None` until the first step that needs it.
    pub(super) state: Option<S>,
    /// The outcomes of the batch the group is in, so far.
    pub(super) outcomes: Vec<QirOutcome>,
    /// The steps of that batch.
    steps: Rc<[SiteStep]>,
    /// The index in `steps` of the step to run next.
    next: usize,
    /// Whether gates were applied since the largest bond dimension was
    /// read.
    pub(super) bond_unread: bool,
}

impl<S> ShotGroup<S> {
    /// The group a run starts with: every shot of `shots`, in their order,
    /// each drawing from its RNG under `seed`, on `context` and no
    /// state, before any batch.
    pub(super) fn of_every_shot(shots: &[QirShotId], seed: u64, context: QirShotContext) -> Self {
        Self {
            shots: shots
                .iter()
                .enumerate()
                .map(|(place, &id)| Shot {
                    place,
                    rng: shot_rng(seed, id),
                })
                .collect(),
            context,
            state: None,
            outcomes: Vec::new(),
            steps: Rc::from([]),
            next: 0,
            bond_unread: false,
        }
    }

    /// The step of the batch to run next, if any is left, counted as run.
    pub(super) fn next_step(&mut self) -> Option<SiteStep> {
        let step = self.steps.get(self.next).cloned()?;
        self.next += 1;
        Some(step)
    }

    /// Starts the batch of `steps`, which no outcome answers yet.
    pub(super) fn start_batch(&mut self, steps: Vec<SiteStep>) {
        self.steps = steps.into();
        self.next = 0;
        self.outcomes.clear();
    }

    /// Each shot's draw against `probability_of_one`, from its own RNG,
    /// in the order of the shots: One when true, as [`reads_one`] reads
    /// the shot's next uniform.
    pub(super) fn draw_each_shot(&mut self, probability_of_one: f64) -> Vec<bool> {
        self.shots
            .iter_mut()
            .map(|shot| reads_one(shot.rng.next_uniform(), probability_of_one))
            .collect()
    }

    /// Keeps the shots whose `draws` are the value `one` (One when true),
    /// each in their order, and gives the others, in theirs.
    pub(super) fn keep_shots_that_drew(&mut self, draws: &[bool], one: bool) -> Vec<Shot> {
        let (mut going_on, mut waiting) = (Vec::new(), Vec::new());
        for (shot, &drew) in std::mem::take(&mut self.shots).into_iter().zip(draws) {
            if drew == one {
                going_on.push(shot);
            } else {
                waiting.push(shot);
            }
        }
        self.shots = going_on;
        waiting
    }

    /// The group of `shots` split off this one at the step just run: its
    /// context copied, on `state`, with this batch's outcomes so far and
    /// `answer`, and the rest of this batch's steps to run.
    pub(super) fn split_off(&self, shots: Vec<Shot>, state: S, answer: Option<QirOutcome>) -> Self {
        let mut outcomes = self.outcomes.clone();
        outcomes.extend(answer);
        Self {
            shots,
            context: self.context.clone(),
            state: Some(state),
            outcomes,
            steps: Rc::clone(&self.steps),
            next: self.next,
            bond_unread: false,
        }
    }
}

/// A shot of a group: its place among the run's shots and its RNG.
pub(super) struct Shot {
    /// Its index in the run's shots, by which the run orders failures.
    pub(super) place: usize,
    /// The RNG its draws come from ([`shot_rng`]).
    rng: SplitMix64,
}

/// Whether a shot that drew `uniform` reads One, where One has the
/// probability `probability_of_one`: exactly when the uniform is below it.
///
/// A uniform is in [0, 1): One only when P(One) > 0, Zero only when
/// P(One) < 1; a P(One) rounded past 0 or 1 draws as a P(One) of 0 or 1
/// would. An impossible value that rounding gives a tiny probability needs
/// no threshold: a shot draws it with that tiny probability only, as a
/// measurement draws and does not post-select.
fn reads_one(uniform: f64, probability_of_one: f64) -> bool {
    uniform < probability_of_one
}

/// The value the shots that go on as the group drew, given each shot's
/// draw (One when true). Where the shots differ, it is the fewer's value.
/// So a group that splits keeps at most half its shots. With the stack of
/// [`GroupedRun::run`], which runs the group split off last first, a run
/// of N shots holds at most ⌊log₂ N⌋ + 1 states at once.
///
/// A tie goes to Zero: any fixed rule would do. No record and no bound
/// depends on it. Which group runs first does, and with it which groups
/// still run after a failure (see `GroupedRun::split`).
pub(super) fn value_going_on(draws: &[bool]) -> bool {
    let ones = draws.iter().filter(|&&one| one).count();
    if ones == 0 || ones == draws.len() {
        ones > 0
    } else {
        2 * ones < draws.len()
    }
}

#[cfg(test)]
mod tests;
