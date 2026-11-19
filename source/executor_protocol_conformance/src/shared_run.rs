// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SharedRun`]: one run's setup and log, shared by the parts of a
//! check's chain and by the check: how the check set the run up
//! ([`RunSetup`]), with the conditions the kit's executor raises
//! ([`raised_conditions`]), and what the wrapper of the part under test
//! and the kit's driver and executor saw ([`RunLog`]). Its own module, so
//! that the wrappers, the driver and the executor of the kit's chain
//! ([`chain`](crate::chain)), which all use it, do not use each other.

mod raised_conditions;
mod run_log;
mod run_setup;

use std::cell::RefCell;
use std::rc::Rc;

pub(crate) use raised_conditions::{
    RAISED_IN_TURN, Raise, Raised, every_kind, raised_in_turn, raising_halves,
};
pub(crate) use run_log::{BatchBelow, RowRecord, RunLog};
pub(crate) use run_setup::RunSetup;

/// The setup and the log of the current run, shared by the parts of one
/// chain and the check that reads the log.
#[derive(Clone, Default)]
pub(crate) struct SharedRun(Rc<RefCell<State>>);

#[derive(Default)]
struct State {
    setup: RunSetup,
    log: RunLog,
}

impl SharedRun {
    /// The shared run of a run set up as `setup`.
    pub(crate) fn new(setup: RunSetup) -> Self {
        Self(Rc::new(RefCell::new(State {
            setup,
            log: RunLog::default(),
        })))
    }

    /// The current run's setup.
    pub(crate) fn setup(&self) -> RunSetup {
        self.0.borrow().setup.clone()
    }

    /// Sets the next run up as `setup`, with a new log, which keeps what
    /// the kit read of the part when it mounted it: its name and offer.
    pub(crate) fn next_run(&self, setup: RunSetup) {
        let mut state = self.0.borrow_mut();
        let part = std::mem::take(&mut state.log.part);
        let not_replayable = state.log.not_replayable;
        state.setup = setup;
        state.log = RunLog {
            part,
            not_replayable,
            ..RunLog::default()
        };
    }

    /// The current run's log.
    pub(crate) fn log(&self) -> RunLog {
        self.0.borrow().log.clone()
    }

    /// Changes the log.
    pub(crate) fn write(&self, change: impl FnOnce(&mut RunLog)) {
        change(&mut self.0.borrow_mut().log);
    }

    /// Counts an `execute` of the part under test: its index in the run.
    pub(crate) fn begin_execute(&self) -> usize {
        let mut state = self.0.borrow_mut();
        let execute = state.log.executes;
        state.log.executes += 1;
        state.log.calls = 0;
        execute
    }

    /// The index of the latest `execute` of the part under test in the
    /// run, if it got one.
    pub(crate) fn latest_execute(&self) -> Option<usize> {
        self.0.borrow().log.executes.checked_sub(1)
    }

    /// Counts a call the kit's executor got: the `execute` of the part
    /// above it it belongs to, and its index among that `execute`'s calls.
    pub(crate) fn next_call_below(&self) -> (usize, usize) {
        let mut state = self.0.borrow_mut();
        let call = state.log.calls;
        state.log.calls += 1;
        (state.log.executes.saturating_sub(1), call)
    }
}
