// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Test-only hook for simulating a slow compilation.
//!
//! The update loop's coalescing behavior only manifests when compilation blocks the
//! host event loop long enough for input events to queue up behind it. Release builds
//! compile too fast to reproduce that reliably, so tests inject an artificial delay here.
//!
//! The delay must be a synchronous busy-wait, not a timer: yielding to the host event
//! loop would let queued events drain and defeat the entire purpose. Only the host knows
//! how to read a clock (`std::time` is unavailable on `wasm32-unknown-unknown`), so the
//! waiting itself is delegated to a callback registered by the WASM layer.

use std::cell::RefCell;

type BusyWaitCallback = Box<dyn Fn(u32)>;

thread_local! {
    static BUSY_WAIT_CB: RefCell<Option<BusyWaitCallback>> = const { RefCell::new(None) };
}

/// Registers the busy-wait callback. Should be called once during initialization.
pub fn set_busy_wait_callback(busy_wait: BusyWaitCallback) {
    BUSY_WAIT_CB.with(|f| *f.borrow_mut() = Some(busy_wait));
}

/// Blocks the current thread for `ms` milliseconds. No-op if no callback is registered
/// or if `ms` is zero.
pub(crate) fn busy_wait(ms: u32) {
    if ms == 0 {
        return;
    }
    BUSY_WAIT_CB.with(|f| {
        if let Some(cb) = f.borrow().as_ref() {
            cb(ms);
        }
    });
}
