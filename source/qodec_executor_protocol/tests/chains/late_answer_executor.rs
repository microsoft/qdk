// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`LateAnswerExecutor`]: an executor whose answer to each `execute` is
//! pending when first asked, as a device's is, and woken later, before the
//! executor it wraps runs the batches.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use executor_protocol::{
    Batch, BatchReadouts, Execute, ExecuteError, ExecutorSetup, InstructionSet, Needs, NotReady,
    Offer, Seed, ShotSet,
};

/// Who wakes the caller waiting for a pending answer, and when.
#[derive(Clone, Copy, Debug)]
pub enum Wake {
    /// Another thread, a moment after the answer is found pending, as a
    /// device's completion does.
    FromAnotherThread,
    /// The executor itself, before it says the answer is pending: the wake
    /// comes before the caller waits.
    BeforePending,
}

/// `executor`, answering each `execute` late.
pub struct LateAnswerExecutor<E> {
    executor: E,
    wake: Wake,
    /// How many answers were pending when first asked.
    pub late_answers: usize,
}

impl<E> LateAnswerExecutor<E> {
    /// `executor`, its answers woken as `wake` says.
    pub fn new(executor: E, wake: Wake) -> Self {
        Self {
            executor,
            wake,
            late_answers: 0,
        }
    }
}

impl<E: ExecutorSetup> ExecutorSetup for LateAnswerExecutor<E> {
    fn name(&self) -> &str {
        self.executor.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.executor.accepts()
    }

    fn offers(&self) -> Offer {
        self.executor.offers()
    }
}

impl<E: Execute> Execute for LateAnswerExecutor<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.executor.ready(needs).await
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        PendingOnce {
            wake: self.wake,
            asked: false,
        }
        .await;
        self.late_answers += 1;
        self.executor.execute(batches, seed, readouts).await
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.executor.end_shots(shots);
    }
}

/// A future pending when first polled, woken as `wake` says, and ready
/// when polled again.
struct PendingOnce {
    wake: Wake,
    asked: bool,
}

impl Future for PendingOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.asked {
            return Poll::Ready(());
        }
        self.asked = true;
        match self.wake {
            Wake::FromAnotherThread => {
                let waker = context.waker().clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(1));
                    waker.wake();
                });
            }
            Wake::BeforePending => context.waker().wake_by_ref(),
        }
        Poll::Pending
    }
}
