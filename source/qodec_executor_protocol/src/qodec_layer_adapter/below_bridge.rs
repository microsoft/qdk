// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`BelowBridge`]: the part below an adapter, as the bottom backend of
//! Qodec's runtime for one shot.
//!
//! Qodec's layers are synchronous: a layer calls the backend below and
//! uses its answer before it returns, as a decoder does when it reads a
//! round's syndrome to choose the next round. The protocol's
//! [`Execute::execute`] answers with a future. So the bridge waits for that
//! future on the calling thread, parking it until the part below wakes it,
//! as the protocol's runner waits for a run. A part below that answers at
//! once, as a simulator does, costs no park; one that answers later, such
//! as a device, holds the thread meanwhile, so the chain runs no other
//! shot in the time. A part below whose answer waits on work of the same
//! task, such as a part that awaits a channel another part of the chain
//! feeds, never gets it: the task is held here, so the run deadlocks.
//!
//! The layers' batches are their own, so one may hold more instructions
//! than the part below takes in a batch, which its offer states. The
//! bridge sends such a batch's instructions in batches of at most that
//! many, in order: Qodec's calls take values, never an earlier readout, so
//! the split reads what one batch would.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;

use executor_protocol::{
    Batch, Execute, ExecuteError, Failure, Instruction, OpcodeMap, Readouts, Seed, ShotConditions,
    ShotSet,
};
use qodec_runtime_recipes::{Backend, BasicReadout, Call};

use crate::QodecInstructionSet;

/// The part below, for one shot of one `adapt`: each batch of bottom-ISA
/// calls the layers send is one `execute` of a one-shot batch below.
pub(super) struct BelowBridge<'a, X> {
    pub(super) below: &'a mut X,
    /// The shot, alone.
    pub(super) shot: &'a ShotSet,
    pub(super) seed: Seed,
    /// Whether the batch the adapter got is checked, and so those it sends.
    pub(super) checked: bool,
    /// The ISA the layers send.
    pub(super) sends: &'a QodecInstructionSet,
    /// Its opcodes in the set below.
    pub(super) map: &'a OpcodeMap,
    pub(super) readouts: &'a mut Readouts,
    /// The conditions the part below raised on this shot, in every batch
    /// the bridge sent it, whether or not the batch read any value: the
    /// results Qodec's layers return carry them only on the calls that
    /// read one.
    pub(super) raised: &'a mut ShotConditions,
    /// The most instructions the part below takes in a batch, from the
    /// offer it was composed with.
    pub(super) most_instructions: Option<u32>,
}

impl<X: Execute> Backend<Vec<Call>, Result<Vec<BasicReadout>, ExecuteError>>
    for BelowBridge<'_, X>
{
    fn execute(&mut self, calls: Vec<Call>) -> Result<Vec<BasicReadout>, ExecuteError> {
        let shot = self.shot.iter().next().expect("a shot");
        let instructions = calls
            .iter()
            .map(|call| self.sends.instruction(call, self.map))
            .collect::<Result<Vec<Instruction>, _>>()
            .map_err(|fault| ExecuteError::Failed {
                shot,
                failure: Failure::Part(Box::new(fault)),
            })?;
        // A part that takes no instruction gets the whole batch, and its
        // boundary refuses it.
        let most = match self.most_instructions {
            Some(most) if most > 0 && (most as usize) < calls.len() => most as usize,
            _ => return self.send(&calls, &instructions),
        };
        let mut results = Vec::with_capacity(calls.len());
        for (calls, instructions) in calls.chunks(most).zip(instructions.chunks(most)) {
            results.extend(self.send(calls, instructions)?);
        }
        Ok(results)
    }
}

impl<X: Execute> BelowBridge<'_, X> {
    /// Sends `instructions`, those of `calls`, below in one batch: the
    /// results of `calls`.
    fn send(
        &mut self,
        calls: &[Call],
        instructions: &[Instruction],
    ) -> Result<Vec<BasicReadout>, ExecuteError> {
        let batches = [
            Batch::new(self.shot, instructions, self.sends.readout_slots(calls))
                .checked_if(self.checked),
        ];
        let readouts = self.readouts.readouts_for(&batches);
        wait(self.below.execute(&batches, self.seed, readouts))?;
        *self.raised |= readouts[0].conditions(0);
        Ok(self
            .sends
            .results(calls, readouts[0].row(0), readouts[0].conditions(0)))
    }
}

/// Polls `future` on this thread until it is ready, parking between polls
/// until its waker is woken: its output. A wake that comes before the
/// thread parks is not lost, since the flag is already set.
fn wait<F: Future>(future: F) -> F::Output {
    let thread_waker = Arc::new(ThreadWaker {
        thread: std::thread::current(),
        woken: AtomicBool::new(false),
    });
    let waker = Waker::from(Arc::clone(&thread_waker));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        while !thread_waker.woken.swap(false, Ordering::Acquire) {
            std::thread::park();
        }
    }
}

/// A waker that marks itself woken, then unparks one thread.
struct ThreadWaker {
    thread: Thread,
    woken: AtomicBool,
}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::Release);
        self.thread.unpark();
    }
}
