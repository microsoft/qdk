// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The debugger, [`Debugger`], and its controls,
//! [`DebuggerHandle`]: while paused, it holds each batch, and the caller's
//! reply stays pending until the batch is stepped through from the handle.
//!
//! It makes its replies both ways a reply can be made. A batch that goes
//! through at once gets the executor's own reply, recorded on its way up
//! ([`Recorded`]). A held batch gets an [`IncrementalReply`] at once, whose
//! [`ReplySender`] the step fills in later, from whichever thread steps.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::task::Waker;

use executor_protocol::{
    Capabilities, Close, Describe, Executor, IncrementalReply, InstructionBatch,
    InstructionBatchRef, Rejected, Reply, ReplySender, ReplyStatus, ShotOutcome, ShotSet, relay,
    reply_channel,
};

use crate::log::{Entry, Log, Recorded, lock, send};
use crate::toy::Op;

/// What the debugger and its handle share: the executor, and the batches
/// held while paused, each with the sender of its caller's reply.
struct State<E> {
    inner: E,
    paused: bool,
    held: VecDeque<(ShotSet, Vec<Op>, ReplySender<ShotOutcome>)>,
}

/// The executor the adapter puts on the whole executor downstream: an
/// executor of the toy's batches upstream of another one.
pub struct Debugger<E> {
    state: Arc<Mutex<State<E>>>,
    log: Log,
}

/// The debugger's controls, usable from any thread.
pub struct DebuggerHandle<E> {
    state: Arc<Mutex<State<E>>>,
    log: Log,
}

impl<E> Clone for DebuggerHandle<E> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            log: Arc::clone(&self.log),
        }
    }
}

/// A debugger around `inner`, not paused, and its handle.
pub fn debugger<E>(inner: E) -> (Debugger<E>, DebuggerHandle<E>) {
    let state = Arc::new(Mutex::new(State {
        inner,
        paused: false,
        held: VecDeque::new(),
    }));
    let log = Log::default();
    let handle = DebuggerHandle {
        state: Arc::clone(&state),
        log: Arc::clone(&log),
    };
    (Debugger { state, log }, handle)
}

impl<E: Executor<InstructionBatch<Op>>> Executor<InstructionBatch<Op>> for Debugger<E> {
    type Reply = DebuggerReply<E::Reply>;

    /// Passes `batch` on, or holds it while paused.
    ///
    /// # Errors
    ///
    /// The executor's refusal, when the batch goes through at once. A held
    /// batch is accepted: see [`DebuggerHandle::step`].
    fn call(&mut self, batch: InstructionBatchRef<'_, Op>) -> Result<Self::Reply, Rejected> {
        let mut state = lock(&self.state);
        if state.paused {
            let (sender, reply) = reply_channel();
            let instructions = batch.instructions.to_vec();
            state
                .held
                .push_back((batch.shots.clone(), instructions, sender));
            return Ok(DebuggerReply::Held(reply));
        }
        send(&mut state.inner, batch, &self.log).map(DebuggerReply::Passed)
    }
}

/// What it offers is what it wraps offers: it draws nothing and changes no
/// instruction.
impl<E: Describe> Describe for Debugger<E> {
    fn capabilities(&self) -> Capabilities {
        lock(&self.state).inner.capabilities()
    }
}

/// Closes what it wraps; a batch still held is refused when stepped.
impl<E: Close> Close for Debugger<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        lock(&self.state).inner.close()
    }
}

/// The debugger's reply: the executor's, recorded; or, for a held batch,
/// the caller's end of a reply that a step completes.
pub enum DebuggerReply<R> {
    Passed(Recorded<R>),
    Held(IncrementalReply<ShotOutcome>),
}

impl<R: Reply<Item = ShotOutcome>> Reply for DebuggerReply<R> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        match self {
            DebuggerReply::Passed(recorded) => recorded.poll_events(out, waker),
            DebuggerReply::Held(held) => held.poll_events(out, waker),
        }
    }
}

impl<E: Executor<InstructionBatch<Op>>> DebuggerHandle<E> {
    /// From now on, holds each batch until it is stepped.
    pub fn pause(&self) {
        lock(&self.state).paused = true;
    }

    /// The batches held now.
    pub fn held(&self) -> usize {
        lock(&self.state).held.len()
    }

    /// Sends the oldest held batch on, and hands its reply to the caller
    /// that sent it: `false` when none is held.
    ///
    /// The step waits for the executor's reply, so one step is one whole
    /// batch. The caller's call already returned `Ok`, so a refusal now
    /// cannot be a [`Rejected`]: it ends the caller's reply as a failure.
    pub fn step(&self) -> bool {
        let mut state = lock(&self.state);
        let Some((shots, instructions, sender)) = state.held.pop_front() else {
            return false;
        };
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions: &instructions,
        };
        match send(&mut state.inner, batch, &self.log) {
            Ok(reply) => relay(reply, sender),
            Err(rejected) => sender.fail(format!("refused when stepped: {rejected}")),
        }
        true
    }

    /// The log so far.
    pub fn log(&self) -> Vec<Entry> {
        lock(&self.log).clone()
    }
}
