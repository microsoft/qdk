// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The first failure of any part of an executor made of parts that fail on
//! threads of their own ([`PartsFailure`]), such as the shots' placements
//! of `.run_on(placement).shots(OnePerShot)`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

#[cfg(doc)]
use super::ExecutorLifecycle;

/// The first failure of any part of an executor made of parts that fail on
/// threads of their own, such as the shots' placements of
/// `.run_on(placement).shots(OnePerShot)`: each part records its
/// failures in it as they happen ([`ExecutorLifecycle::part_of`]), and the
/// executor reads it at the start of each call, so that a call after a
/// part's failure is refused before anything runs, whatever parts it names.
/// It also holds the executor's call being given to its parts
/// ([`begin`](Self::begin)), by which each part numbers its own calls, so
/// that a part's failure names the executor's call it failed in. Of several
/// parts' failures, the earliest call's is kept, with its reason, whatever
/// order they happen in: the earliest failed call decides, as for the
/// executor's own failures ([`ExecutorLifecycle::fail`]), both the reason
/// later calls are refused with and, at close, which states are left out as
/// made for a failed call ([`ShotStates`](crate::ShotStates)). A read while
/// no part failed is one atomic load; a failure, and each read after one,
/// takes a lock, and a new earliest failure costs a copy of its reason.
#[derive(Clone, Debug, Default)]
pub(crate) struct PartsFailure(pub(super) Arc<Parts>);

/// What [`PartsFailure`] shares between an executor and its parts.
#[derive(Debug)]
pub(super) struct Parts {
    /// The earliest of the executor's calls a part failed in, with its
    /// reason, once one did: kept together, so that the reason is that
    /// call's whatever order the parts fail in.
    first: Mutex<Option<(u64, String)>>,
    /// The call of `first`, or `u64::MAX` while none has failed: read
    /// without the lock.
    earliest: AtomicU64,
    /// The executor's call being given to its parts.
    pub(super) call: AtomicU64,
}

impl Default for Parts {
    fn default() -> Self {
        Self {
            first: Mutex::new(None),
            earliest: AtomicU64::new(u64::MAX),
            call: AtomicU64::new(0),
        }
    }
}

impl PartsFailure {
    /// Records that a part failed in the executor's call `call`, for
    /// `reason`; kept unless a part failed in that call or an earlier one
    /// already.
    pub(super) fn record(&self, call: u64, reason: &str) {
        // A poisoned lock still holds a valid record: it is replaced whole.
        let mut first = self.0.first.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*first, Some((earliest, _)) if earliest <= call) {
            return;
        }
        *first = Some((call, reason.to_owned()));
        // Under the lock, as in `FirstFailure::record`: stored after the
        // unlock, a later call's store could raise `earliest` past an
        // earlier call's.
        self.0.earliest.store(call, Ordering::Release);
    }

    /// The earliest of the executor's calls a part failed in, and why,
    /// once one did.
    #[inline]
    pub(crate) fn first(&self) -> Option<(u64, String)> {
        if self.0.earliest.load(Ordering::Acquire) == u64::MAX {
            return None;
        }
        let first = self.0.first.lock().unwrap_or_else(PoisonError::into_inner);
        first.clone()
    }

    /// The executor gives its parts its call `call`: each part admitted from
    /// now on numbers its call so. The parts are given a call on the
    /// executor's thread, inside its `call`.
    #[inline]
    pub(crate) fn begin(&self, call: u64) {
        self.0.call.store(call, Ordering::Relaxed);
    }

    /// The earliest of the executor's calls a part failed in, once one did.
    pub(crate) fn earliest_call(&self) -> Option<u64> {
        let call = self.0.earliest.load(Ordering::Acquire);
        (call != u64::MAX).then_some(call)
    }
}
