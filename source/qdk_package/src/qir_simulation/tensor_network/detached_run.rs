// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A run off the Python thread, which Ctrl-C stops.
//!
//! A tensor-network run may last hours. Meanwhile Python must keep running
//! its other threads, and Ctrl-C must stop the run and free the device.
//! Python runs other threads only while the GIL is released, and handles a
//! signal only when a thread checks for one. So the run goes to a thread of
//! its own, and the caller's thread waits with the GIL released, checking
//! for signals. On Ctrl-C it sets the run's stop flag, waits for the run to
//! end, and raises `KeyboardInterrupt`.
//!
//! The run is given as a closure, so the engine is made and used on the run
//! thread, and its states need not be `Send`.

#[cfg(test)]
mod tests;

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::ScopedJoinHandle;
use std::time::Duration;

use pyo3::prelude::*;

use super::run_failure::RunFailure;

/// How often the caller's thread checks for a Python signal while a run is
/// under way.
const SIGNAL_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// Calls `run` on a thread of its own, with the stop flag it must obey,
/// while the caller's thread waits with the GIL released; on a signal, sets
/// the flag, waits for `run` to end, and raises the signal's exception.
/// Otherwise gives what `run` gives, its failure raised as
/// [`RunFailure::into_py_err`] says; a `run` that panics raises
/// `RuntimeError` ([`panicked_run`]).
///
/// ```text
/// caller's thread (GIL released)        run thread
/// ──────────────────────────────        ──────────────────────────
/// spawn ──────────────────────────────▶ run(stop_flag)
/// every SIGNAL_CHECK_INTERVAL,            runs batches of gates
///   check for a signal
/// Ctrl-C: set stop_flag ──────────────▶   sees the flag between
///                                         batches, frees its states
/// outcome ◀──────────────────────────── sends its outcome, or panics
/// raise the interrupt, the panic's
/// RuntimeError, or give the outcome
/// ```
pub(super) fn run_detached<T, F>(py: Python<'_>, run: F) -> PyResult<T>
where
    T: Send,
    F: FnOnce(Arc<AtomicBool>) -> Result<T, RunFailure> + Send,
{
    let stop_flag = Arc::new(AtomicBool::new(false));
    let (sender, outcomes) = mpsc::channel();
    std::thread::scope(|scope| {
        let run_stop_flag = Arc::clone(&stop_flag);
        let run_thread = scope.spawn(move || {
            // The receiver may be gone only if the caller's thread panicked.
            let _ = sender.send(run(run_stop_flag));
        });
        match wait_for_outcome(py, outcomes, &stop_flag) {
            // A panic is a defect, so its `RuntimeError` is raised even
            // after an interrupt: the defect is what needs reporting.
            (None, _) => Err(panicked_run(run_thread).into_py_err()),
            (Some(_), Some(interrupt)) => Err(interrupt),
            (Some(outcome), None) => outcome.map_err(RunFailure::into_py_err),
        }
    })
}

/// Waits for the run's outcome as [`wait_stopping_on_signal`] says, each
/// wait at most [`SIGNAL_CHECK_INTERVAL`] with the GIL released, and
/// Python's pending signal taken between waits as its exception.
fn wait_for_outcome<O: Send>(
    py: Python<'_>,
    mut outcomes: mpsc::Receiver<O>,
    stop_flag: &AtomicBool,
) -> (Option<O>, Option<PyErr>) {
    let wait_once = || {
        // A closure that releases the GIL must be `Send`: a receiver
        // borrowed mutably is, one borrowed shared is not.
        let receiver = &mut outcomes;
        py.detach(move || receiver.recv_timeout(SIGNAL_CHECK_INTERVAL))
    };
    wait_stopping_on_signal(wait_once, || py.check_signals().err(), stop_flag)
}

/// Waits for a run's outcome, one `wait_once` at a time, and takes a
/// pending signal with `take_signal` after each wait that timed out. On
/// the first signal, it sets `stop_flag` and waits on: the run stops at its
/// next batch and frees its states, and the caller must not return before
/// that. It takes no later signal, leaving it to Python. Gives the outcome,
/// `None` when the run thread ended without sending one, and the first
/// signal, if one came.
///
/// The wait and the signal are arguments, so its tests give them without
/// threads, timing or Python.
fn wait_stopping_on_signal<O, S>(
    mut wait_once: impl FnMut() -> Result<O, mpsc::RecvTimeoutError>,
    mut take_signal: impl FnMut() -> Option<S>,
    stop_flag: &AtomicBool,
) -> (Option<O>, Option<S>) {
    let mut signal = None;
    loop {
        match wait_once() {
            Ok(outcome) => return (Some(outcome), signal),
            Err(mpsc::RecvTimeoutError::Disconnected) => return (None, signal),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if signal.is_none() {
                    signal = take_signal();
                    if signal.is_some() {
                        stop_flag.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

/// The failure of a run whose thread ended without sending its outcome,
/// which it does only by panicking: `RuntimeError`, with the panic's
/// message. The thread is joined here, as `std::thread::scope` would
/// otherwise panic in turn, raising a `PanicException` instead.
pub(super) fn panicked_run<T>(run_thread: ScopedJoinHandle<'_, T>) -> RunFailure {
    let no_outcome = "the tensor-network run ended without an outcome";
    let message = match run_thread.join() {
        Err(panic) => panic_message(&*panic),
        Ok(_) => None,
    };
    RunFailure::Runtime(match message {
        Some(message) => format!("{no_outcome}: {message}"),
        None => no_outcome.to_string(),
    })
}

/// The message a panic carries: a `&str` for a literal message, a `String`
/// for a formatted one; `None` for any other payload.
fn panic_message(panic: &(dyn Any + Send)) -> Option<String> {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
}
