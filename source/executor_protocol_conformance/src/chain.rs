// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Chain`]: the chain a check runs around the part under test, between
//! the kit's driver ([`ScriptedDriver`]), which sends the check's script,
//! and, below an adapter, the kit's executor ([`RecordingExecutor`]),
//! which answers and logs each batch. A [`Mount`] builds the chain
//! around a new part, wrapped
//! ([`ExecutorUnderTest`](executor_under_test::ExecutorUnderTest),
//! [`AdapterUnderTest`](adapter_under_test::AdapterUnderTest)) so that
//! the kit logs each row the part answers.
//!
//! The wrapper sits below the protocol's boundary, not above it: the
//! boundary shapes each batch's readouts, one row per shot of the batch's
//! readout slots, each value `Missing(Other)`, before it calls the part
//! below it. So the wrapper is reached with readouts already shaped, may
//! prefill their values, calls the part with them, and then reads what the
//! part left: the shape, the values and the conditions. Everything else
//! (the admission, each part's `ready`, the boundary's checks, the catching
//! of panics, the end of the shots) runs as in production, except that the
//! wrapper notes the shots the part holds, catches a panic in an end of
//! shots not held as a fault, and withholds ends where a check drops the
//! part, and that [`Chain`] runs a script's named shots alone when the
//! chain refuses the one more shot the script never names ([`Script`] says
//! when).

mod adapter_under_test;
mod executor_under_test;
mod held_shots;
mod recording_executor;
mod scripted_driver;

use executor_protocol::{
    Batch, BatchReadouts, Driver, Execute, Executor, InstructionSet, Needs, Offer, ReadoutValue,
    RunError, RunRefusal, Runner,
};

pub(crate) use adapter_under_test::AdapterMount;
pub(crate) use executor_under_test::ExecutorMount;
pub(crate) use held_shots::HeldShots;
pub(crate) use recording_executor::RecordingExecutor;
pub(crate) use scripted_driver::ScriptedDriver;

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::nonconformance::{Fault, Findings, Nonconformance, Place, failed_run, uncomposed};
use crate::script::Script;
use crate::shared_run::{RowRecord, RunLog, RunSetup, SharedRun};
use crate::shot_records::ShotRecords;

/// How a check mounts a part in the kit's chain: a new part, made each
/// time, between the kit's driver and, below an adapter, the kit's
/// executor.
pub(crate) trait Mount {
    /// The chain below the kit's driver: the part's wrapper, and the
    /// kit's executor below an adapter.
    type Below: Execute;

    /// The chain around a new part, logging to `run`, or why the kit
    /// cannot run the current setup's script on it.
    fn chain(&mut self, run: &SharedRun) -> Result<Runner<ScriptedDriver, Self::Below>, String>;
}

/// A chain around one part, and the [`SharedRun`] its parts share, to run
/// once or several times.
pub(crate) struct Chain<M: Mount> {
    shared_run: SharedRun,
    runner: Runner<ScriptedDriver, M::Below>,
}

/// What a run gave: what its parts saw, and its failure, if it failed.
pub(crate) struct RunOutcome {
    pub(crate) log: RunLog,
    pub(crate) failure: Option<RunError>,
}

impl RunOutcome {
    /// Why the check cannot use the run, if the chain refused it: a refused
    /// run ran nothing, so it shows nothing of the part's rules, and the
    /// protocol lets a part that is not ready refuse a run, as a busy
    /// device does.
    pub(crate) fn why_refused(&self) -> Option<String> {
        match &self.failure {
            Some(failure @ RunError::Refused(_)) => Some(failed_run(failure)),
            _ => None,
        }
    }
}

impl<M: Mount> Chain<M> {
    /// The chain `mount` builds around a new part, first set up as
    /// `setup`.
    pub(crate) fn new(mount: &mut M, setup: RunSetup) -> Result<Self, String> {
        let shared_run = SharedRun::new(setup);
        let runner = mount.chain(&shared_run)?;
        Ok(Self { shared_run, runner })
    }

    /// Runs the chain once, set up as `setup`, on the shots of a run of
    /// `setup`'s script ([`Script::run_shots`]), with `seed`; or, if the
    /// chain refuses a run of that many shots as `TooManyShots`, on the
    /// shots the script names alone ([`Script`] says why). A refused run
    /// ran nothing (the runner checks `max_shots` before asking any part),
    /// so the rerun is judged alone; a run refused for another reason is
    /// not run again.
    pub(crate) fn run(&mut self, setup: RunSetup, seed: u64) -> RunOutcome {
        let shots = setup.script.run_shots(seed);
        let named = setup.script.named_shots();
        self.shared_run.next_run(setup);
        let mut without_unsent = shots.len() == named.len();
        let mut ran = self.runner.run(&shots, seed);
        if !without_unsent && matches!(ran, Err(RunError::Refused(RunRefusal::TooManyShots { .. })))
        {
            without_unsent = true;
            ran = self.runner.run(&named, seed);
        }
        let failure = ran.err();
        let mut log = self.shared_run.log();
        log.without_unsent = without_unsent;
        RunOutcome { log, failure }
    }

    /// The part's name.
    pub(crate) fn part_name(&self) -> String {
        self.shared_run.log().part
    }
}

/// Runs a new chain around a new part once, as `setup` sets it up, with
/// the trial's seed, notes the run in `findings`, and gives what its parts
/// saw; or, if the run failed, the check's result: what `findings` holds,
/// and why the check cannot use the run.
pub(crate) fn noted_run(
    findings: &mut Findings,
    mount: &mut impl Mount,
    setup: RunSetup,
) -> Result<RunLog, Nonconformance> {
    let log = run_fresh(mount, setup, findings.run_seed()).map_err(|why| findings.failed(why))?;
    findings.note_run(&log);
    Ok(log)
}

/// Runs `script` on a new part `mount` builds, notes the run in
/// `findings`, and gives the shots' records; or, if the run failed, the
/// check's result, as [`noted_run`] gives it.
pub(crate) fn records(
    findings: &mut Findings,
    mount: &mut impl Mount,
    script: Script,
) -> Result<ShotRecords, Nonconformance> {
    let log = noted_run(findings, mount, RunSetup::of(script))?;
    Ok(ShotRecords::of(&log))
}

/// Runs a new chain around a new part once, as `setup` sets it up, with
/// `seed`: what its parts saw, or, if it failed, why the check cannot use
/// it.
///
/// A part the setup keeps from `end_shots` is dropped with its shots held,
/// as a crash would leave it; if it panics then, the panic is caught and
/// named as why the check cannot judge it, rather than ending the
/// implementer's test.
fn run_fresh(mount: &mut impl Mount, setup: RunSetup, seed: u64) -> Result<RunLog, String> {
    let withholds_end_shots = setup.withholds_end_shots;
    let mut chain = Chain::new(mount, setup.clone())?;
    let ran = chain.run(setup, seed);
    if withholds_end_shots && let Err(panic) = catch_unwind(AssertUnwindSafe(|| drop(chain))) {
        return Err(format!(
            "the part panicked when dropped with its shots never ended ({}); the check \
             needs a part that can be dropped so, as a crash leaves it",
            panic_message(&*panic)
        ));
    }
    match ran.failure {
        None => Ok(ran.log),
        Some(failure) => Err(failed_run(&failure)),
    }
}

/// The message of a caught panic, if it has one.
fn panic_message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("with no message")
}

/// The driver of a chain around a part that accepts `accepts`, needing
/// `needs`; or, if the current setup's script does not suit the part, why.
pub(crate) fn driver_for(
    run: &SharedRun,
    accepts: &InstructionSet,
    needs: Needs,
) -> Result<ScriptedDriver, String> {
    if let Some(why) = run.setup().script.cannot_judge_with(accepts) {
        return Err(why);
    }
    Ok(ScriptedDriver::new(run.clone(), accepts.clone(), needs))
}

/// Logs what the part answered for `batches` in `readouts`, in its
/// `execute` numbered `execute`: each batch of another shape as a fault,
/// and each row of the others, with the values prefilled into it, if any.
/// `prefills`, if any, holds a row per shot of each batch:
/// [`prefilled`](crate::readout_prefills::prefilled) wrote them before the
/// part ran, into readouts shaped to `batches`, as `Readouts::readouts_for`
/// gives them.
pub(crate) fn log_rows(
    run: &SharedRun,
    execute: usize,
    batches: &[Batch<'_>],
    readouts: &[BatchReadouts],
    prefills: Option<&[Vec<Vec<ReadoutValue>>]>,
) {
    run.write(|log| {
        for (index, (batch, readouts)) in batches.iter().zip(readouts).enumerate() {
            if readouts.shots() != batch.shots.len()
                || readouts.readout_slots() != batch.readout_slots()
            {
                log.shape_faults.push(Fault {
                    place: Place::Batch {
                        execute,
                        batch: index,
                    },
                    found: format!(
                        "readouts of {} rows of {} slots, for a batch of {} shots of {} slots",
                        readouts.shots(),
                        readouts.readout_slots(),
                        batch.shots.len(),
                        batch.readout_slots()
                    ),
                });
                continue;
            }
            for (row, shot) in batch.shots.iter().enumerate() {
                log.rows.push(RowRecord {
                    execute,
                    batch: index,
                    row,
                    shot,
                    values: readouts.row(row).to_vec(),
                    prefills: prefills
                        .map(|all| all[index][row].clone())
                        .unwrap_or_default(),
                    conditions: readouts.conditions(row),
                });
            }
        }
    });
}

/// The name a part gives, kept in the log for the check's report.
pub(crate) fn name_part(run: &SharedRun, name: &str) {
    run.write(|log| name.clone_into(&mut log.part));
}

/// Notes in the log whether `offer`, the part's, has shots that are not
/// replayable, for the checks of rules that bind a replayable part only.
pub(crate) fn note_offer(run: &SharedRun, offer: &Offer) {
    run.write(|log| log.not_replayable = !offer.replayable);
}

/// The kit's driver composed over `below` into a runner, or the
/// composition's faults as why the kit cannot run it.
pub(crate) fn compose<E: Execute>(
    driver: ScriptedDriver,
    below: Executor<E>,
) -> Result<Runner<ScriptedDriver, E>, String> {
    (Driver::new(driver) >> below).map_err(|faults| uncomposed(&faults))
}
