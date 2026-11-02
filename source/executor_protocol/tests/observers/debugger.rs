// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A debugger that logs: an adapter that records each call of each shot,
//! with what the call read, or why it failed or was refused below, and
//! passes the readouts up as they came. It is written with `adapter_fn`, a
//! state per shot (`OnePerShot`): each shot's state is its handle on the
//! log, and the user keeps a handle of the whole, to read after the run. It
//! reports nothing, so the chain's report is the one below it, unchanged.
//!
//! It logs what each call read, so it waits for the call's readouts below
//! (`Below::run`); a debugger that logs only the calls sends them on
//! (`Below::send`), as the profiler does. The pausing debugger of the guide
//! `tests/debugger.rs` is written by hand, since it holds calls; this one
//! holds none.
//!
//! Waiting for one shot's readouts sends that shot's call down alone, so
//! below it the shots of a call run one by one, in the call's order, each
//! in a call of its own. Six things follow:
//!
//! - It is transparent over an executor that offers `replayable`: a
//!   shot's outcomes there do not depend on which shots share the
//!   executor, so not on a call per shot
//!   (`the_debugger_changes_no_outcome`; planned: `.needs(..)` on
//!   adapters, to require it when the chain is built).
//! - A refusal below of one shot, after an earlier shot's call went down,
//!   fails the run, where the run alone is refused: the earlier shot ran.
//! - A call refused whole, by the check below, refuses the run, as alone,
//!   and is logged in the call's first shot alone, since the refusal ends
//!   the call before the other shots send it.
//! - A call cancelled below ends failed, and fails the debugger, which
//!   refuses every later call, where alone the call ends cancelled: a run
//!   cannot read a reply that was cancelled.
//! - A reply below that is not whole, one that reads a tag of a shot twice
//!   or a shot it was not given, fails the call and the debugger, where
//!   alone its readouts pass up as they came: `Below::run` checks what it
//!   waits for, and the log holds why.
//! - Each call blocks the thread that called the debugger until its
//!   readouts come below: to keep the caller free, run the chain on a
//!   thread of its own (planned: `run_on` on adapters, to place it).
//!
//! The same logic with one state, without `.shots(OnePerShot)`, is the
//! transparent alternative: it would run each call below whole, as the
//! program sent it, so the first three things above would not follow. It
//! would be transparent over any executor, and a call refused below would
//! refuse the run, as alone. What it loses is the log by shot: its log
//! would be by call, since its logic is not given the call's shots
//! (planned: the shots of a call, on `ProgramInput`), so a call that fails,
//! is refused or reads nothing could not be put in a shot's log. The last
//! three would remain, since it would still wait on `Below::run`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use executor_protocol::{
    AdapterError, AdapterLogic, Below, InstructionCall, MakeState, OnePerShot, OnePerShotAdapter,
    Outcomes, ProgramInput, ShotContext, ShotId, ShotOutcome, adapter_fn,
};

/// One call the debugger passed down, with its result below.
#[derive(Clone, Debug, PartialEq)]
pub struct LoggedCall {
    /// The call's instructions, as the debugger passed them down.
    pub instructions: Vec<InstructionCall>,
    /// What the call gave below: none if it never returned, as when the
    /// executor below panicked.
    pub result: Option<CallResult>,
}

/// What a call gave below: its readouts, in the order the reply below gave
/// them, or why it failed or was refused there.
pub type CallResult = Result<Vec<ShotOutcome>, AdapterError>;

/// The debugger's log of one chain's run: each shot's calls, in the order
/// they were made, each with its result. A handle: its clones share one
/// log, so the user keeps one and gives one to each debugger that logs
/// into it. Two debuggers in one chain may share it, to log the calls
/// above and below an adapter in one place: each call holds its own result.
///
/// A chain runs one job (`Chain::run` takes the chain), so give each chain
/// a log of its own: one log given to two chains adds their calls to each
/// shot's, with nothing to tell the runs apart.
#[derive(Clone, Debug, Default)]
pub struct DebugLog(Arc<Mutex<BTreeMap<ShotId, Vec<LoggedCall>>>>);

impl DebugLog {
    /// The log of shot `shot`, which a shot's state records into.
    fn for_shot(&self, shot: ShotId) -> ShotLog {
        ShotLog {
            shot,
            log: self.clone(),
        }
    }

    /// What shot `shot` called in the run this log records, in the order
    /// the calls were made, each with its result: none if the shot never
    /// ran. Given to one debugger alone, a call that failed or was refused
    /// below, or did not return, is the shot's last.
    pub fn of_shot(&self, shot: ShotId) -> Vec<LoggedCall> {
        let shots = self.0.lock().expect("no logic panics holding the log");
        shots.get(&shot).cloned().unwrap_or_default()
    }
}

/// One shot's state in the debugger: its part of the log.
pub struct ShotLog {
    shot: ShotId,
    log: DebugLog,
}

impl ShotLog {
    /// Records a call of `instructions`, about to go below, and returns
    /// where: until [`ShotLog::returned`], the logged call has no result.
    /// Another debugger on the log may record calls after it before it
    /// returns.
    fn call(&mut self, instructions: &[InstructionCall]) -> usize {
        let mut shots = self.log.0.lock().expect("no logic panics holding the log");
        let logged = shots.entry(self.shot).or_default();
        logged.push(LoggedCall {
            instructions: instructions.to_vec(),
            result: None,
        });
        logged.len() - 1
    }

    /// Records the result returned by the call recorded at `call`.
    fn returned(&mut self, call: usize, result: &CallResult) {
        let mut shots = self.log.0.lock().expect("no logic panics holding the log");
        let logged = shots.get_mut(&self.shot);
        let logged = logged.and_then(|logged| logged.get_mut(call));
        logged.expect("the call was recorded before it ran").result = Some(result.clone());
    }
}

/// The debugger's logic: records one shot's call, runs it below, waiting
/// for what it reads, records that, and passes the readouts up. The call
/// is recorded before it runs, so that one that panics below is in the
/// log; one that fails or is refused below is recorded with its reason
/// before the call above ends.
fn log_and_pass(
    log: &mut ShotLog,
    input: &ProgramInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    let call = log.call(input.instructions());
    let result = below.run(input.instructions());
    log.returned(call, &result);
    outcomes.extend(result?);
    Ok(())
}

/// The debugger, recording into `log`, a handle the user keeps a clone of:
/// a state per shot, each its shot's part of the log. It states no
/// instruction set: it passes on the one below, whatever it is. Over an
/// executor that describes none, the chain is refused when it is built
/// (`NoInstructionSetDownstream`), since what it sends could not be
/// checked.
pub fn debugger(
    log: DebugLog,
) -> OnePerShotAdapter<
    InstructionCall,
    InstructionCall,
    ShotLog,
    impl MakeState<ShotLog>,
    impl AdapterLogic<InstructionCall, InstructionCall, ShotLog>,
> {
    adapter_fn(
        move |shot: ShotContext| Ok(log.for_shot(shot.shot())),
        log_and_pass,
    )
    .shots(OnePerShot)
}
