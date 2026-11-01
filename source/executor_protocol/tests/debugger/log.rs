// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The debugger's log: each batch that goes through it, and each outcome,
//! refusal and end that comes back, in order.

use std::sync::{Arc, Mutex, MutexGuard};
use std::task::Waker;

use executor_protocol::{
    Executor, InstructionBatch, InstructionBatchRef, Rejected, Reply, ReplyStatus, ShotOutcome,
};

use crate::toy::Op;

/// One line of the debugger's log.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// A batch's instructions, as they went to the executor.
    Batch(Vec<Op>),
    /// The executor's refusal of the batch before it.
    Rejected(Rejected),
    /// An outcome, as it came back.
    Outcome(ShotOutcome),
    /// The end of a batch's reply.
    End(ReplyStatus),
}

pub type Log = Arc<Mutex<Vec<Entry>>>;

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("no thread panicked holding it")
}

/// Sends `batch` to `inner`, and records it and what comes back.
pub fn send<E: Executor<InstructionBatch<Op>>>(
    inner: &mut E,
    batch: InstructionBatchRef<'_, Op>,
    log: &Log,
) -> Result<Recorded<E::Reply>, Rejected> {
    lock(log).push(Entry::Batch(batch.instructions.to_vec()));
    match inner.call(batch) {
        Ok(reply) => Ok(Recorded {
            inner: reply,
            log: Arc::clone(log),
        }),
        Err(rejected) => {
            lock(log).push(Entry::Rejected(rejected.clone()));
            Err(rejected)
        }
    }
}

/// A reply that records each event and its end as they pass.
pub struct Recorded<R> {
    inner: R,
    log: Log,
}

impl<R: Reply<Item = ShotOutcome>> Reply for Recorded<R> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        let from = out.len();
        let status = self.inner.poll_events(out, waker);
        let mut log = lock(&self.log);
        log.extend(out[from..].iter().copied().map(Entry::Outcome));
        if status != ReplyStatus::Pending {
            log.push(Entry::End(status.clone()));
        }
        status
    }
}
