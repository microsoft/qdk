// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which Python exception a run thread that panicked raises
//! ([`panicked_run`]), a defect the package's Python tests cannot cause;
//! and how the caller's thread waits for a run's outcome while signals
//! come ([`wait_stopping_on_signal`]), its waits and signals scripted.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;

use super::{RunFailure, panicked_run, wait_stopping_on_signal};

/// A run thread that panics (a defect) raises `RuntimeError` with the
/// panic's message, rather than a `PanicException` from the thread scope,
/// whether the message is a literal or formatted (an `expect` on an error,
/// an index out of bounds, a `panic!` with arguments: most defects), which
/// reach the thread's join as a `&str` and a `String` payload.
#[test]
fn a_run_that_panics_is_a_runtime_error_with_its_message() {
    fn literal() {
        panic!("a defect of the run");
    }
    fn formatted() {
        let shot = 3;
        panic!("a defect of shot {shot}");
    }
    let cases: [(&str, fn(), &str); 2] = [
        ("literal", literal, "a defect of the run"),
        ("formatted", formatted, "a defect of shot 3"),
    ];
    for (case, defect, expected) in cases {
        let failure = std::thread::scope(|scope| panicked_run(scope.spawn(defect)));
        assert!(
            matches!(&failure, RunFailure::Runtime(message) if message.contains(expected)),
            "{case}: a run that panics is a RuntimeError with the panic's message: {failure:?}"
        );
    }
}

/// Ctrl-C during a run sets the run's stop flag at once, and the caller
/// waits on until the run ends: the run stops at its next batch and frees
/// its states, so the caller must not return before. Without a signal, the
/// run is not stopped, and the wait gives its outcome alone. The run sends
/// its outcome at the fourth wait. Each row is the signals pending, and
/// whether the flag is set as each wait begins: a Ctrl-C, pending at the
/// first wait's timeout, so the flag is set from the second wait on; and
/// none, so the flag is never set.
#[test]
fn a_signal_sets_the_stop_flag_and_the_wait_goes_on_to_the_outcome() {
    let rows: [(&[&str], _); 2] = [
        (&["Ctrl-C"], [false, true, true, true]),
        (&[], [false, false, false, false]),
    ];
    for (signals, flag_at_each_wait) in rows {
        let waited = wait_through(&[TIMEOUT, TIMEOUT, TIMEOUT, Ok(OUTCOME)], signals);
        assert_eq!(waited.flag_at_each_wait, flag_at_each_wait, "{signals:?}");
        assert_eq!(
            (waited.outcome, waited.signal),
            (Some(OUTCOME), signals.first().copied()),
            "{signals:?}"
        );
    }
}

/// Only the first signal is taken: a second Ctrl-C, pressed while the run
/// stops, is left to Python, and the first is the one the caller raises.
/// Two signals are pending at the first wait's timeout; the run sends its
/// outcome at the third wait.
#[test]
fn only_the_first_signal_is_taken() {
    let waited = wait_through(&[TIMEOUT, TIMEOUT, Ok(OUTCOME)], &["first", "second"]);
    assert_eq!(waited.signal, Some("first"));
    assert_eq!(waited.signals_left, ["second"]);
}

/// A run thread that ends without sending an outcome (it panicked) ends the
/// wait with none, and with the signal taken before, if any. Each row is
/// the signals pending, and the signal given: none; and a Ctrl-C, taken at
/// the first wait's timeout, before the thread ends at the second wait.
#[test]
fn a_run_thread_that_ends_without_an_outcome_ends_the_wait_with_none() {
    let rows: [(&[&str], _); 2] = [(&[], None), (&["Ctrl-C"], Some("Ctrl-C"))];
    for (signals, signal) in rows {
        let waited = wait_through(&[TIMEOUT, Err(RecvTimeoutError::Disconnected)], signals);
        assert_eq!(
            (waited.outcome, waited.signal),
            (None, signal),
            "{signals:?}"
        );
    }
}

/// The outcome a test's run sends.
const OUTCOME: u32 = 7;

/// A wait that timed out before the run sent its outcome.
const TIMEOUT: Result<u32, RecvTimeoutError> = Err(RecvTimeoutError::Timeout);

/// What [`wait_through`] gave and left.
struct Waited {
    /// The outcome the wait gave.
    outcome: Option<u32>,
    /// The signal the wait gave.
    signal: Option<&'static str>,
    /// Whether the stop flag was set as each wait began.
    flag_at_each_wait: Vec<bool>,
    /// The signals still pending when the wait ended, the first first.
    signals_left: Vec<&'static str>,
}

/// Waits for a run's outcome while `signals` are pending, the first first,
/// each wait giving the next of `waits`.
fn wait_through(waits: &[Result<u32, RecvTimeoutError>], signals: &[&'static str]) -> Waited {
    let stop_flag = AtomicBool::new(false);
    let mut waits = waits.iter().copied();
    let mut flag_at_each_wait = Vec::new();
    let mut signals: VecDeque<_> = signals.iter().copied().collect();
    let (outcome, signal) = wait_stopping_on_signal(
        || {
            flag_at_each_wait.push(stop_flag.load(Ordering::Relaxed));
            waits.next().expect("the scene ends with the run")
        },
        || signals.pop_front(),
        &stop_flag,
    );
    Waited {
        outcome,
        signal,
        flag_at_each_wait,
        signals_left: signals.into(),
    }
}
