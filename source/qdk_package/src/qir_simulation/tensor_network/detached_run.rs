// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A run off the Python thread, which Ctrl-C stops.
//!
//! A tensor-network run may last hours. For that long its caller needs
//! Python to keep running its other threads, and Ctrl-C to stop the run
//! and give the device back. Python runs other threads only while the
//! caller has released the GIL, and handles a signal only when the caller
//! checks for it, which a thread busy with the run never does. So the run
//! goes to a thread of its own, and the caller's thread waits for it with
//! the GIL released, checking for signals every [`SIGNAL_CHECK`]. On one
//! (Ctrl-C), it sets the run's stop flag, waits for the run to end (an MPS
//! run reads the flag between batches, and frees its states), and raises
//! the signal's exception, `KeyboardInterrupt`.
//!
//! The run is given as a closure, so whatever it makes stays on the run
//! thread: the engine is made and used there, and its states need not be
//! `Send`.

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::ScopedJoinHandle;
use std::time::Duration;

use pyo3::prelude::*;

use super::run_failure::RunFailure;

/// How often the caller's thread checks for a Python signal while a run is
/// under way.
const SIGNAL_CHECK: Duration = Duration::from_millis(100);

/// Calls `run` on a thread of its own, with the stop flag it must obey,
/// while the caller's thread waits with the GIL released; on a signal, sets
/// the flag, waits for `run` to end, and raises the signal's exception.
/// Otherwise gives what `run` gives, its failure raised as
/// [`RunFailure::into_py_err`] says; a `run` that panics raises
/// `RuntimeError` ([`lost_run`]).
pub(super) fn run_detached<T, F>(py: Python<'_>, run: F) -> PyResult<T>
where
    T: Send,
    F: FnOnce(Arc<AtomicBool>) -> Result<T, RunFailure> + Send,
{
    let stop_flag = Arc::new(AtomicBool::new(false));
    let (sender, mut outcomes) = mpsc::channel();
    std::thread::scope(|scope| {
        let run_stop_flag = Arc::clone(&stop_flag);
        let run_thread = scope.spawn(move || {
            // The receiver may be gone only if the caller's thread panicked.
            let _ = sender.send(run(run_stop_flag));
        });
        let mut interrupt = None;
        loop {
            let (waited, returned) = py.detach(move || {
                let waited = outcomes.recv_timeout(SIGNAL_CHECK);
                (waited, outcomes)
            });
            outcomes = returned;
            match waited {
                Ok(outcome) => {
                    return match (interrupt, outcome) {
                        (Some(interrupt), _) => Err(interrupt),
                        (None, outcome) => outcome.map_err(RunFailure::into_py_err),
                    };
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if interrupt.is_none()
                        && let Err(signal) = py.check_signals()
                    {
                        stop_flag.store(true, Ordering::Relaxed);
                        interrupt = Some(signal);
                    }
                }
                // A panic is a defect, so its `RuntimeError` is raised even
                // after an interrupt: the defect is what needs reporting.
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(lost_run(run_thread).into_py_err());
                }
            }
        }
    })
}

/// The failure of a run whose thread ended without sending its outcome,
/// which it does only by panicking: `RuntimeError`, with the panic's
/// message. The thread is joined here, as `std::thread::scope` would
/// otherwise panic in turn, raising a `PanicException` instead.
pub(super) fn lost_run<T>(run_thread: ScopedJoinHandle<'_, T>) -> RunFailure {
    let lost = "the tensor-network run ended without an outcome";
    let Err(panic) = run_thread.join() else {
        return RunFailure::Runtime(lost.to_string());
    };
    let message = panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned());
    RunFailure::Runtime(match message {
        Some(message) => format!("{lost}: {message}"),
        None => lost.to_string(),
    })
}
