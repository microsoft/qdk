//! An executor that answers later: it wraps any executor of instruction
//! batches, and each reply it gives has nothing at its first poll, its
//! events coming from the second poll on. The wrapped executor still runs
//! the call at `call`: what comes later is when the reply is seen, not when
//! the work runs. It also counts the shots in flight (those of the replies
//! given and not yet ended or dropped; a refused call's are never counted),
//! keeping the most there were at once and the most shots of one call that
//! read ([`InFlight`]): how much of the executor's room for work in flight
//! the chain above uses.
//!
//! Its lateness is one poll, which any adapter that polls its sends uses up
//! (the protocol's adapter facility polls each send at once): over an
//! adapter, it does not show whether the run yields while a reply below is
//! pending; [`AnswersOnYield`] does.
//!
//! The protocol has no form for such an executor: it is written by hand
//! here, with its `>>`, as any executor of another crate is.
//!
//! [`AnswersOnYield`]: crate::answers_on_yield::AnswersOnYield

use std::ops::Shr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Waker;

use executor_protocol::{
    Capabilities, ChainPrefix, Close, Compose, Describe, Executor, InstructionBatch,
    InstructionBatchRef, Rejected, Reply, ReplyStatus, WithExecutor,
};

/// The shots in flight, now and at most at once, and the most shots of
/// one call that read, shared by an [`AnswersLater`] and whoever reads it.
#[derive(Debug, Default)]
pub struct InFlight {
    now: AtomicUsize,
    most: AtomicUsize,
    widest_read: AtomicUsize,
}

impl InFlight {
    /// The most shots that were in flight at once, in one call or several.
    #[must_use]
    pub fn most(&self) -> usize {
        self.most.load(Ordering::SeqCst)
    }

    /// The most shots of one call whose reply gave events: how many shots
    /// the executor read at once.
    #[must_use]
    pub fn widest_read(&self) -> usize {
        self.widest_read.load(Ordering::SeqCst)
    }

    fn begin(&self, shots: usize) {
        let now = self.now.fetch_add(shots, Ordering::SeqCst) + shots;
        self.most.fetch_max(now, Ordering::SeqCst);
    }

    fn end(&self, shots: usize, read: bool) {
        self.now.fetch_sub(shots, Ordering::SeqCst);
        if read {
            self.widest_read.fetch_max(shots, Ordering::SeqCst);
        }
    }
}

/// `executor`, answering later; it counts its shots in flight in
/// `in_flight`.
///
/// # Type parameters
///
/// - `E`: the wrapped executor, set by the executor [`AnswersLater::new`]
///   is given.
pub struct AnswersLater<E> {
    executor: E,
    in_flight: Arc<InFlight>,
}

impl<E> AnswersLater<E> {
    /// `executor`, its replies answering later, their shots counted in
    /// `in_flight`.
    pub fn new(executor: E, in_flight: Arc<InFlight>) -> Self {
        Self {
            executor,
            in_flight,
        }
    }
}

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for AnswersLater<E> {
    type Reply = LaterReply<E::Reply>;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        let reply = self.executor.call(batch)?;
        let shots = batch.shots.as_slice().len();
        self.in_flight.begin(shots);
        Ok(LaterReply {
            reply,
            polled: false,
            read: false,
            in_flight: Some((Arc::clone(&self.in_flight), shots)),
        })
    }
}

impl<E: Describe> Describe for AnswersLater<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for AnswersLater<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// A reply of an [`AnswersLater`]: pending at its first poll, which wakes
/// its waker, then the wrapped reply's.
///
/// # Type parameters
///
/// - `R`: the wrapped reply, set by the wrapped executor's reply type.
pub struct LaterReply<R> {
    reply: R,
    polled: bool,
    /// Whether it gave events.
    read: bool,
    /// Its shots, held in flight until the reply ends, or is dropped.
    in_flight: Option<(Arc<InFlight>, usize)>,
}

impl<R: Reply> Reply for LaterReply<R> {
    type Item = R::Item;

    fn poll_events(&mut self, out: &mut Vec<R::Item>, waker: &Waker) -> ReplyStatus {
        if !self.polled {
            self.polled = true;
            waker.wake_by_ref();
            return ReplyStatus::Pending;
        }
        let given = out.len();
        let status = self.reply.poll_events(out, waker);
        self.read |= out.len() > given;
        if !matches!(status, ReplyStatus::Pending)
            && let Some((in_flight, shots)) = self.in_flight.take()
        {
            in_flight.end(shots, self.read);
        }
        status
    }
}

impl<R> Drop for LaterReply<R> {
    fn drop(&mut self) {
        if let Some((in_flight, shots)) = self.in_flight.take() {
            in_flight.end(shots, self.read);
        }
    }
}

/// `>>` for a chain that ends at an [`AnswersLater`], grouped from the
/// left: `static_driver() >> a >> b >> later`. The protocol implements `>>`
/// for its own executors alone, so an executor of another crate writes it.
/// FRICTION: one impl per type of the protocol that may stand on its left
/// (the driver, composed adapters, each form of adapter); this crate writes
/// the one its chains use.
impl<T, L, E> Shr<AnswersLater<E>> for ChainPrefix<T, L>
where
    Self: Compose<AnswersLater<E>, WithExecutor>,
{
    type Output = <Self as Compose<AnswersLater<E>, WithExecutor>>::Output;

    fn shr(self, executor: AnswersLater<E>) -> Self::Output {
        self.compose(executor)
    }
}

#[cfg(test)]
#[path = "tests/answers_later.rs"]
mod tests;
