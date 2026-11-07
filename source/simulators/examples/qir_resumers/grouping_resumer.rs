// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A resumer that runs its shots together and groups them by their next
//! batch ([`QirGroupingResumer`]): the shots whose next batches are equal
//! run it in one dispatch to their simulators.
//!
//! Example code, not the crate's API: a resumer may choose the order of
//! its shots' batches, as a device that runs one batch on many shots at
//! once would. The example's `main`, the tests and the benchmark include
//! this file, so it names the crate's items by their paths.

use std::convert::Infallible;

use qdk_simulators::qir::{
    QirCall, QirMachine, QirOutcome, QirProgram, QirResumeError, QirResumer, QirShotContext,
    QirShotId, QirStop,
};
use qdk_simulators::{OutputRecord, Simulator};

use super::simulator_resumer::run_batch;

/// Runs the shots of a run together, a context of the machine and a
/// simulator made by `make` per shot (from the program and the shot's id,
/// as the simulator resumer makes it), grouped by their next batch.
///
/// It runs in rounds. In each, it resumes every shot that has not ended,
/// in the order of the run's shots, with the outcomes of its last batch;
/// the shots that stop at equal batches form a group, whatever path each
/// took to it. It then makes one dispatch per group, in the order of each
/// group's first shot: the dispatch runs the group's batch on each of its
/// shots' simulators ([`run_batch`]). So a group splits where its shots'
/// outcomes lead them to different batches, and shots meet again where
/// their batches are equal: a program of `b` batches per shot costs one
/// dispatch per group and round, not one per shot and batch.
///
/// When a shot traps, the shots after it in the run's order stop, and
/// those before it keep running until they end or trap: the run's error
/// names the first shot in that order that trapped. Its runs never wait,
/// and its own work never fails ([`Infallible`]).
pub struct QirGroupingResumer<F> {
    make: F,
    dispatches: Vec<Vec<QirShotId>>,
}

impl<F> QirGroupingResumer<F> {
    /// The resumer whose shots run on the simulators `make` makes.
    pub fn new(make: F) -> Self {
        Self {
            make,
            dispatches: Vec::new(),
        }
    }

    /// The shots of each dispatch it made, in the order of its dispatches,
    /// over all its runs: each dispatch ran one batch on each of its shots.
    pub fn dispatches(&self) -> &[Vec<QirShotId>] {
        &self.dispatches
    }
}

impl<S, F> QirResumer for QirGroupingResumer<F>
where
    S: Simulator,
    F: FnMut(&QirProgram, QirShotId) -> S,
{
    type Error = Infallible;

    async fn run(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> Result<Vec<Vec<OutputRecord>>, QirResumeError<Infallible>> {
        let mut run = GroupedRun {
            shots: shots
                .iter()
                .map(|&id| GroupedShot {
                    id,
                    context: machine.start_shot(),
                    simulator: (self.make)(machine.program(), id),
                    outcomes: Vec::new(),
                    records: None,
                    group: None,
                })
                .collect(),
            trap: None,
        };
        let mut batches = Vec::new();
        loop {
            run.next_round(machine, &mut batches);
            if batches.is_empty() {
                break;
            }
            for (group, batch) in batches.iter().enumerate() {
                self.dispatches.push(run.dispatch(group, batch));
            }
        }
        match run.trap {
            Some((_, error)) => Err(error),
            None => Ok(run
                .shots
                .into_iter()
                .map(|shot| shot.records.expect("every shot ended"))
                .collect()),
        }
    }
}

/// The state of a run: a [`GroupedShot`] per shot, in the order of the
/// run's shots, and the first trap in that order, with its shot's place.
struct GroupedRun<S> {
    shots: Vec<GroupedShot<S>>,
    trap: Option<(usize, QirResumeError<Infallible>)>,
}

impl<S: Simulator> GroupedRun<S> {
    /// The shots still running: those before the first trap.
    fn running(&self) -> usize {
        self.trap
            .as_ref()
            .map_or(self.shots.len(), |(place, _)| *place)
    }

    /// Resumes every running shot that has not ended, in order, and groups
    /// them, until one traps: `batches` becomes the round's distinct
    /// batches, in the order of each one's first shot, and each shot's
    /// group the index of its batch there. Empty once every running shot
    /// ended.
    fn next_round(&mut self, machine: &QirMachine<'_>, batches: &mut Vec<Vec<QirCall>>) {
        batches.clear();
        let running = self.running();
        for (place, shot) in self.shots[..running].iter_mut().enumerate() {
            if shot.records.is_some() {
                continue;
            }
            shot.group = None;
            // Resume the shot with its last batch's outcomes, up to its
            // next stop.
            match machine.resume(&mut shot.context, &shot.outcomes) {
                Ok(QirStop::Ended(records)) => shot.records = Some(records.to_vec()),
                // A batch: the shot joins the group of an equal batch met
                // this round, whatever path it took to it, or starts one.
                Ok(QirStop::Batch(batch)) => {
                    let group = batches
                        .iter()
                        .position(|equal| equal[..] == *batch)
                        .unwrap_or_else(|| {
                            batches.push(batch.to_vec());
                            batches.len() - 1
                        });
                    shot.group = Some(group);
                }
                // A trap: the shots after it stop, and those before it run
                // on, to find the first failing shot in the run's order.
                Err(trap) => {
                    let shot = shot.id;
                    self.trap = Some((place, QirResumeError::Trapped { shot, trap }));
                    break;
                }
            }
        }
    }

    /// Runs `batch` on the simulator of each running shot of `group`,
    /// giving each its outcomes, and gives the shots of the dispatch.
    fn dispatch(&mut self, group: usize, batch: &[QirCall]) -> Vec<QirShotId> {
        let running = self.running();
        let mut members = Vec::new();
        for shot in &mut self.shots[..running] {
            if shot.group == Some(group) {
                shot.outcomes.clear();
                run_batch(&mut shot.simulator, batch, &mut shot.outcomes);
                members.push(shot.id);
            }
        }
        members
    }
}

/// A shot of a grouped run: its id, its context and simulator, the
/// outcomes of its last batch, its records once it ended, and its group in
/// the current round.
struct GroupedShot<S> {
    id: QirShotId,
    context: QirShotContext,
    simulator: S,
    outcomes: Vec<QirOutcome>,
    records: Option<Vec<OutputRecord>>,
    /// The index of its batch among the round's distinct batches; `None`
    /// once it ended.
    group: Option<usize>,
}
