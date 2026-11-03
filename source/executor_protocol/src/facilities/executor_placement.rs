// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where an executor runs ([`ExecutorPlacement`]): on a thread of its own
//! ([`OwnThread`]), or wherever a closure puts its work ([`PlacementJob`]).

use std::sync::mpsc;
use std::thread;

#[cfg(doc)]
use crate::PlacedExecutor;

/// The work a [`PlacedExecutor`] hands its placement: it runs the calls
/// queued so far, in order, and those queued while it runs, then ends.
pub type PlacementJob = Box<dyn FnOnce() + Send>;

/// Where an executor runs, given to `.run_on(place)`: [`OwnThread`], or a
/// closure that is handed each [`PlacementJob`] and runs it where it
/// chooses, such as `move |job| pool.spawn(job)` for a thread pool.
///
/// The closure's argument type is inferred where the closure passes the
/// job on (`pool.spawn(job)`, `thread::spawn(job)`); one that calls it
/// itself names it: `|job: PlacementJob| job()`.
///
/// A placement runs or drops each job it is handed: a job dropped unrun
/// fails the calls it held, but one kept unrun holds the executor's calls,
/// their replies and its `close` until it runs. And since `close`, or a
/// `wait` on one of its replies, blocks its thread until a job ran, a
/// placement must not defer a job to run later on that thread (a one-thread
/// runtime's spawn, under the task that closes or waits), or it waits for
/// ever; nor may a waker that runs on the executor's thread (woken by a
/// reply, inside a job) close the executor, or `wait` on one of its
/// replies, since it would wait for the job it runs in.
///
/// Dropping a placed executor does not wait for its placement: its calls
/// not yet started are cancelled, and a last job drops the executor it
/// places, with the states it holds, after the call running, if any (a call
/// cannot be stopped mid-way). So what those states hold (a device, GPU
/// memory) is freed some time after `drop` returns: call `close`, which
/// waits, before building another executor on it ([`PlacedExecutor`]). A
/// placement must not panic as `drop` hands it that last job: the panic
/// leaves `drop`, and aborts the process when the drop runs during an
/// unwind.
///
/// A job keeps the panics of the code it runs, so that none reaches the
/// placement's thread (a pool's). A call's fails its reply, since the
/// caller may be elsewhere by then. An instruction's drop that panics, the
/// caller's code, is absorbed, whether its call ran or not; so is the drop
/// of an executor of the crate it places, the implementer's code (a
/// state's, or what a closure captures), wherever it is freed but by
/// `close`, which raises a state's ([`PlacedExecutor`]); and so is the
/// drop of the placement, or of what it captures once started: `drop`
/// never panics. The
/// close's (a report closure's, or a reported state's drop) is raised
/// again in the caller of `close`, which waits for it, its payload
/// unchanged, as `std::thread::scope` and rayon raise the panic of work
/// moved to another thread in the caller that waits for it: the least
/// surprise for a Rust user ([`PlacedExecutor`]).
///
/// Planned, not built: `Remote(link)`, an executor serving the far end of
/// a link (the near end is an executor of its own); and placing any
/// executor, not only those `executor_fn` makes, or a state that is not
/// `Send` (`docs/executors.md`, §Where an executor runs).
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an executor placement",
    note = "a placement is `OwnThread`, or a closure `FnMut(PlacementJob)`; \
            `Remote(link)` is planned, not built"
)]
pub trait ExecutorPlacement {
    /// Starts this placement for one executor, at its first call: what
    /// runs the executor's jobs from then on. The executor hands it one
    /// job at a time.
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send>;
}

/// The placement on a thread of the executor's own, made at its first
/// call: every call of the executor runs on that one thread, so that an
/// executor that must run on one thread (a device's driver that expects
/// it) can be placed, provided it and its state are [`Send`]: the executor
/// is moved to the thread, and a placed executor is an executor only when
/// what it places is `Send`. The thread ends once the executor is dropped,
/// its call running has finished (those queued are cancelled), and the
/// executor it places was dropped there. A call that panicked there stops
/// the placement, though: the executor it places is then dropped where it
/// is closed, or, never closed, usually where the [`PlacedExecutor`] is
/// dropped, not on that thread (a known issue).
///
/// Planned, not built: placing a state that is not `Send`, because it is
/// bound to the thread that made it, such as an engine's GPU context or a
/// state built on `Rc`. Under `.run_on(OwnThread)`, the state closure
/// would run on that thread, so that the state never leaves it and needs
/// no `Send`, as in a prototype of this crate. Today the compiler refuses
/// such a state where the placed executor is called, or at `.shots(..)`
/// after `.run_on(..)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnThread;

impl ExecutorPlacement for OwnThread {
    /// # Panics
    ///
    /// When the system cannot make a thread, as [`thread::spawn`].
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send> {
        let (jobs, queued) = mpsc::channel::<PlacementJob>();
        thread::spawn(move || {
            for job in queued {
                job();
            }
        });
        Box::new(move |job| {
            // The thread runs every job sent, and ends only once this
            // sender is dropped: the send cannot fail.
            jobs.send(job).expect("the executor's thread outlives it");
        })
    }
}

impl<F: FnMut(PlacementJob) + Send + 'static> ExecutorPlacement for F {
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send> {
        Box::new(self)
    }
}
