// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The grouped run: how an [`MpsResumer`] runs the shots of one run.
//!
//! Shots that drew the same values form a [`ShotGroup`] and share one
//! state, so a run holds few states. For instance, 1,000 shots of a Bell
//! pair share one state until their first measurement, and two after it:
//!
//! ```text
//!  shots 0-999 on ψ ── H(0), CX(0, 1) ── mz(0) ─┬─ the fewer value's shots,
//!                                               │  on ψ projected on it
//!                                               └─ the other value's shots,
//!                                                  on a fork of ψ projected
//!                                                  on it
//!  then mz(1): each group's shots all draw its value, so neither splits
//! ```
//!
//! `src/mps_simulator/design.md` ("The phases of a run") names each phase
//! after the function or call that does it, and its file.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::OutputRecord;
use crate::qir::{
    QirMachine, QirOutcome, QirProgram, QirResumeError, QirShotContext, QirShotId, QirStop,
};
use tensornet::{MpsEngine, MpsShape};

#[cfg(doc)]
use super::MpsResumer;
use super::lowering::{SiteStep, lower};
use super::mps_resumer_error::MpsResumerError;
use super::mps_run_report::MpsRunReport;
use super::qubit_order::QubitOrder;

mod first_failure;
mod shot_group;
mod site_steps;

use first_failure::{FirstFailure, GroupFailure};
use shot_group::ShotGroup;
use site_steps::DrawStep;

/// One run of an [`MpsResumer`]: what every group of the run shares, the
/// groups that wait their turn, and the first failure.
///
/// **The loop.** [`start`](Self::start) makes one group of every shot, on
/// one machine context and no state. [`run`](Self::run) pops the groups off
/// a stack until none waits. `run_group` runs one group, batch by batch,
/// until its context ends. `run_step` runs each step of a batch on the
/// group's state, in [`site_steps`]. A split pushes the group that waits
/// onto the stack.
///
/// **Failures.** [`FirstFailure`] keeps the failure of the first shot in
/// the order of the shots. A group's shots keep that order, so a group
/// whose first shot comes after the failing shot is dropped. `run` drops
/// it unrun when it waits; `run_group` runs no step of it, so it stops the
/// group after the step whose split leaves it only such shots.
/// [`MpsResumer::with_stop_flag`] says where the stop flag is read; the
/// callers of `check_stop_flag` apply it.
pub(super) struct GroupedRun<'r, E: MpsEngine> {
    /// The engine the run's states live on.
    engine: &'r E,
    /// The machine that runs the program's classical part for each group.
    machine: &'r QirMachine<'r>,
    /// The run's shots, which name a failure.
    shots: &'r [QirShotId],
    /// The site of each of the program's qubits, the same for every group.
    qubit_order: QubitOrder,
    /// The capacity of each state. `None` for a program of no qubits, which
    /// needs no state.
    capacity: Option<MpsShape>,
    /// The flag that stops the run, if any.
    stop_flag: Option<&'r AtomicBool>,
    /// The resumer's report of this run, filled as the shots run.
    report: &'r mut MpsRunReport,
    /// The groups split off that wait their turn, the last to run next.
    waiting: Vec<ShotGroup<E::State>>,
    /// Each shot's records, at its place in the order of the run's shots,
    /// once its group ended.
    records: Vec<Option<Vec<OutputRecord>>>,
    /// The first failing shot in the order of the run's shots.
    failure: FirstFailure<E::Error>,
}

impl<'r, E: MpsEngine> GroupedRun<'r, E> {
    /// The run of `shots` of `machine`'s program on `engine`, before any
    /// shot runs. It computes the qubit order and the capacity from the
    /// program, and makes one group of every shot. Each shot draws from its
    /// RNG under `seed`. The run stops once `stop_flag`, if any, is set
    /// ([`MpsResumer::with_stop_flag`]).
    ///
    /// `shots` is not empty: a run of no shots needs neither the qubit order
    /// nor the capacity, so [`MpsResumer`] does not start it.
    ///
    /// # Errors
    ///
    /// [`MpsResumerError::Capacity`], naming the first shot, when the
    /// program's qubits and `max_bond_dimension` make no chain.
    pub(super) fn start(
        engine: &'r E,
        machine: &'r QirMachine<'r>,
        shots: &'r [QirShotId],
        seed: u64,
        max_bond_dimension: NonZeroUsize,
        stop_flag: Option<&'r AtomicBool>,
        report: &'r mut MpsRunReport,
    ) -> Result<Self, QirResumeError<MpsResumerError<E::Error>>> {
        let program = machine.program();
        let &first_shot = shots
            .first()
            .expect("the resumer starts no run of no shots");
        let capacity =
            capacity_of(program, max_bond_dimension).map_err(|error| QirResumeError::Resumer {
                shot: first_shot,
                error,
            })?;
        Ok(Self {
            engine,
            machine,
            shots,
            qubit_order: QubitOrder::for_program(program),
            capacity,
            stop_flag,
            report,
            waiting: vec![ShotGroup::of_every_shot(shots, seed, machine.start_shot())],
            records: vec![None; shots.len()],
            failure: FirstFailure::none(),
        })
    }

    /// Runs the groups one at a time until none waits. Gives each shot's
    /// records, or the first failing shot.
    ///
    /// The groups wait on a stack, so the last split off runs first:
    ///
    /// ```text
    ///  event               running   waiting, the top on the right
    ///  start               —         G
    ///  pop G               G         —
    ///  G splits off G1     G         G1
    ///  G splits off G2     G         G1 G2
    ///  G ends              —         G1 G2
    ///  pop G2              G2        G1
    /// ```
    ///
    /// # Errors
    ///
    /// The first failing shot in the order of the run's shots: its context
    /// trapped, or the resumer failed it.
    pub(super) fn run(
        mut self,
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<MpsResumerError<E::Error>>> {
        while let Some(mut group) = self.waiting.pop() {
            if self.comes_after_the_failure(&group) {
                continue;
            }
            match self.run_group(&mut group) {
                Ok(GroupEnd::Ended(records)) => self.record(&group, &records),
                Ok(GroupEnd::Dropped) => {}
                // The group's shots now, which failed: a split may have
                // moved its first shots to a group of their own.
                Err(failure) => self.failure.keep(group.shots[0].place, failure),
            }
        }
        self.into_result()
    }

    /// Gives each shot of `group`, whose context ended, its `records`.
    fn record(&mut self, group: &ShotGroup<E::State>, records: &[OutputRecord]) {
        for shot in &group.shots {
            self.records[shot.place] = Some(records.to_vec());
        }
    }

    /// Once no group waits: each shot's records, or the first failing shot
    /// and why it failed.
    fn into_result(
        self,
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<MpsResumerError<E::Error>>> {
        if let Some(error) = self.failure.into_error(self.shots) {
            return Err(error);
        }
        Ok(self
            .records
            .into_iter()
            .map(|records| records.expect("every shot ended or the run failed"))
            .collect())
    }

    /// Whether a shot before every shot of `group` failed: the run's result
    /// is then that failure, whatever `group` does, so `group` is dropped.
    fn comes_after_the_failure(&self, group: &ShotGroup<E::State>) -> bool {
        // A group's shots are in the order of the run's: a split keeps it.
        self.failure.comes_before(group.shots[0].place)
    }

    /// Runs `group` until its context ends, giving its records; the groups
    /// that split off it wait on [`Self::waiting`]. Drops `group`, running
    /// no further step, once its shots all come after the run's failure:
    /// before its first step, or after a step whose split leaves it only
    /// such shots, whether settling the fork failed or not (the step has
    /// settled `group`'s own value).
    fn run_group(
        &mut self,
        group: &mut ShotGroup<E::State>,
    ) -> Result<GroupEnd, GroupFailure<E::Error>> {
        loop {
            self.check_stop_flag()?;
            self.run_steps(group)?;
            if self.comes_after_the_failure(group) {
                return Ok(GroupEnd::Dropped);
            }
            self.read_bond(group)?;
            match self.resume(&mut group.context, &group.outcomes)? {
                QirStop::Ended(records) => {
                    self.end(group.state.as_ref())?;
                    return Ok(GroupEnd::Ended(records.to_vec()));
                }
                QirStop::Batch(calls) => {
                    let steps = lower(calls, &self.qubit_order)?;
                    group.start_batch(steps);
                }
            }
        }
    }

    /// Runs the steps of `group`'s batch not yet run, in order, while
    /// `group` holds a shot that does not come after the run's failure. A
    /// split may leave it only shots that come after: no later step runs,
    /// and `run_group` then drops it.
    fn run_steps(&mut self, group: &mut ShotGroup<E::State>) -> Result<(), GroupFailure<E::Error>> {
        while !self.comes_after_the_failure(group)
            && let Some(step) = group.next_step()
        {
            self.run_step(&step, group)?;
        }
        Ok(())
    }

    /// The group's `context` resumed with the `outcomes` of its batch: its
    /// next batch, or its end with its records. The stop flag is read again
    /// before the batch is lowered or a trap reported; an ended context
    /// keeps its records.
    fn resume<'c>(
        &self,
        context: &'c mut QirShotContext,
        outcomes: &[QirOutcome],
    ) -> Result<QirStop<'c>, GroupFailure<E::Error>> {
        let stop = self.machine.resume(context, outcomes);
        if !matches!(stop, Ok(QirStop::Ended(_))) {
            self.check_stop_flag()?;
        }
        stop.map_err(GroupFailure::Trapped)
    }

    /// Fails with [`MpsResumerError::Stopped`] once the stop flag is set.
    fn check_stop_flag(&self) -> Result<(), GroupFailure<E::Error>> {
        if self
            .stop_flag
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(GroupFailure::Resumer(MpsResumerError::Stopped));
        }
        Ok(())
    }

    /// Runs `step` on `group`: a gate is applied, a measurement or a reset
    /// drawn. It matches on every kind of [`SiteStep`], so a new kind runs
    /// here. A draw reads the largest bond dimension first
    /// ([`Self::read_bond`]), as projecting may shrink it; a gate does not.
    fn run_step(
        &mut self,
        step: &SiteStep,
        group: &mut ShotGroup<E::State>,
    ) -> Result<(), GroupFailure<E::Error>> {
        match *step {
            SiteStep::Apply(ref gate) => self.apply(group, gate),
            SiteStep::Measure {
                site,
                result,
                reset,
            } => self.draw(group, DrawStep::measurement(site, result, reset)),
            SiteStep::Reset { site } => self.draw(group, DrawStep::reset_only(site)),
        }
    }
}

/// What became of a group that did not fail.
enum GroupEnd {
    /// Its context ended with these records, each of its shots'.
    Ended(Vec<OutputRecord>),
    /// It was dropped, its shots all after the run's failure.
    Dropped,
}

/// The capacity of each state of a run of `program`: a chain of one site
/// per qubit and bonds of at most `max_bond_dimension`; `None` for a
/// program of no qubits, which needs no state.
fn capacity_of<E>(
    program: &QirProgram,
    max_bond_dimension: NonZeroUsize,
) -> Result<Option<MpsShape>, MpsResumerError<E>> {
    let qubits = program.qubits();
    if qubits == 0 {
        return Ok(None);
    }
    let sites = usize::try_from(qubits).expect("a u32 fits a usize");
    MpsShape::qubit_chain(sites, max_bond_dimension.get())
        .map(Some)
        .map_err(|source| MpsResumerError::Capacity {
            qubits,
            max_bond_dimension: max_bond_dimension.get(),
            source,
        })
}

#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod tests;
