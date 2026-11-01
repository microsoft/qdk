// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The debugger as an adapter, [`Debuggers`], to compose in a chain: it
//! puts a debugger on the executor downstream, and hands out its handle.

use std::sync::{Arc, Mutex};

use executor_protocol::{Adapter, Apply, Close, Executor, InstructionBatch};

use crate::debugger::{Debugger, DebuggerHandle, debugger};
use crate::log::lock;
use crate::toy::Op;

/// The debugger as an adapter: composed with an executor, it puts
/// a debugger on it, paused or not, and keeps its handle, which
/// [`handle`](Debuggers::handle) hands out from any thread. A clone shares
/// the handle.
///
/// Known issue (blocks a later unit, the debugger's): the name is plural
/// from when an adapter put a debugger on each shot; it now puts one on
/// the whole executor, so it is to be renamed with the debugger's unit.
pub struct Debuggers<E> {
    paused: bool,
    handle: Arc<Mutex<Option<DebuggerHandle<E>>>>,
}

impl<E> Debuggers<E> {
    /// A debugger that holds each batch until it is stepped.
    pub fn paused() -> Self {
        Self {
            paused: true,
            handle: Arc::default(),
        }
    }

    /// The handle of the debugger, once it is composed with an executor.
    pub fn handle(&self) -> Option<DebuggerHandle<E>> {
        lock(&self.handle).clone()
    }
}

impl<E> Clone for Debuggers<E> {
    fn clone(&self) -> Self {
        Self {
            paused: self.paused,
            handle: Arc::clone(&self.handle),
        }
    }
}

/// It passes the calls it is called with down, so it changes no
/// capability, and implements none of the methods.
impl<E> Adapter for Debuggers<E> {
    type Accepts = Op;
    type Sends = Op;
}

impl<E: Executor<InstructionBatch<Op>> + Close> Apply<E> for Debuggers<E> {
    type Output = Debugger<E>;

    fn apply(self, downstream: E) -> Debugger<E> {
        let (debugger, handle) = debugger(downstream);
        if self.paused {
            handle.pause();
        }
        *lock(&self.handle) = Some(handle);
        debugger
    }
}
