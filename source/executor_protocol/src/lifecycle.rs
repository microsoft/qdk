// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The life of a session: open, failed from its first failed call on, and
//! closed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Waker;

use crate::{Completion, Rejected, Status};

/// The first failed call of a session and why, shared by the session and the
/// replies of its calls. Every call and poll reads `call`, one atomic load;
/// the lock is taken only on a failure.
#[derive(Debug)]
struct FirstFailure {
    /// The earliest failed call, or `u64::MAX` while none has failed.
    call: AtomicU64,
    reason: Mutex<Option<(u64, String)>>,
}

impl Default for FirstFailure {
    fn default() -> Self {
        Self {
            call: AtomicU64::new(u64::MAX),
            reason: Mutex::new(None),
        }
    }
}

impl FirstFailure {
    fn record(&self, call: u64, reason: &str) {
        // A poisoned lock still holds a valid record: it is replaced whole.
        let mut first = self.reason.lock().unwrap_or_else(PoisonError::into_inner);
        if first.as_ref().is_none_or(|(earliest, _)| call < *earliest) {
            *first = Some((call, reason.to_owned()));
            self.call.store(call, Ordering::Release);
        }
    }

    /// The reason of the earliest failed call, if it is earlier than `call`.
    fn before(&self, call: u64) -> Option<String> {
        if self.call.load(Ordering::Acquire) >= call {
            return None;
        }
        let first = self.reason.lock().unwrap_or_else(PoisonError::into_inner);
        first.as_ref().map(|(_, reason)| reason.clone())
    }
}

/// The life of a session, for the executors that open sessions: open; failed
/// from its first failed call on; closed. A call to a failed session is
/// refused with [`Rejected::SessionFailed`], and to a closed one with
/// [`Rejected::Closed`], which is checked first.
///
/// A failure is final because the shots it hit are left part-way through a
/// batch, so a later batch on them would run on a state the caller's program
/// never made. It is final for the whole session, not only for the failed
/// call's shots: the other shots' states are sound, but no driver needs to
/// go on with them yet. A driver that needs it would keep the set of failed
/// shots here, and refuse only a call that names one.
#[derive(Debug, Default)]
pub struct Lifecycle {
    failure: Arc<FirstFailure>,
    calls: u64,
    closed: bool,
}

impl Lifecycle {
    /// Admits a call. Checked before the call's own faults, so that a closed
    /// or failed session says so whatever the call.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] after [`Lifecycle::close`]; otherwise
    /// [`Rejected::SessionFailed`] after a call failed.
    pub fn admit(&mut self) -> Result<(), Rejected> {
        if self.closed {
            return Err(Rejected::Closed);
        }
        if let Some(reason) = self.failure.before(u64::MAX) {
            return Err(Rejected::SessionFailed(reason));
        }
        self.calls += 1;
        Ok(())
    }

    /// Records that the call admitted last failed, for an executor that
    /// learns of it before the call returns.
    pub fn fail(&self, reason: &str) {
        self.watch().fail(reason);
    }

    /// A handle on the call admitted last, for its reply: a reply that learns
    /// of a failure only when polled records it through the handle.
    #[must_use]
    pub fn watch(&self) -> Watch {
        Watch {
            failure: Arc::clone(&self.failure),
            call: self.calls,
        }
    }

    /// `completion`, the reply of the call admitted last, watched: see
    /// [`Watched`].
    pub fn watched<C: Completion>(&self, completion: C) -> Watched<C> {
        Watched {
            inner: completion,
            watch: self.watch(),
        }
    }

    /// Ends the session: later calls are refused with [`Rejected::Closed`].
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// Whether the session was closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

/// A reply's handle on its session's [`Lifecycle`]. The call's number only
/// orders the session's calls: it counts every admitted call, also those
/// refused afterwards for their own faults, so it is not a submission id.
#[derive(Clone, Debug)]
pub struct Watch {
    failure: Arc<FirstFailure>,
    call: u64,
}

impl Watch {
    /// Records that this call failed. The earliest failed call is kept: a
    /// driver that polls several replies may see their failures out of
    /// order.
    pub fn fail(&self, reason: &str) {
        self.failure.record(self.call, reason);
    }

    /// Why this call must end without more events: an earlier call failed. A
    /// driver that keeps several calls in flight learns of a failure when it
    /// polls, so this call may have been admitted before the failure was
    /// known.
    #[must_use]
    pub fn earlier(&self) -> Option<String> {
        self.failure
            .before(self.call)
            .map(|reason| format!("an earlier submission failed: {reason}"))
    }
}

/// A reply that fails its session when it fails, and ends without more
/// events when an earlier call of its session failed: what a session that
/// learns of failures only when its replies are polled wraps them in.
pub struct Watched<C> {
    inner: C,
    watch: Watch,
}

impl<C: Completion> Completion for Watched<C> {
    type Item = C::Item;

    fn poll_events(&mut self, out: &mut Vec<C::Item>, waker: &Waker) -> Status {
        if let Some(reason) = self.watch.earlier() {
            return Status::Failed(reason);
        }
        let status = self.inner.poll_events(out, waker);
        if let Status::Failed(reason) = &status {
            self.watch.fail(reason);
        }
        status
    }
}
