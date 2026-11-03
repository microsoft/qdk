//! An executor whose replies are answered only once the run above yields:
//! it wraps any executor of instruction batches, and each reply it gives
//! holds its events back until the run awaited over it
//! ([`YieldGate::await_run`]) returns `Pending` after the call, or until a
//! timeout passes. The answer comes from another thread, as a remote
//! device's does, so it shows what [`AnswersLater`] cannot: whether a chain
//! yields while a reply below is pending, or holds the thread that polls
//! it. A reply answered by its timeout was waited for on that thread, since
//! the run could not yield while it was pending, unless that thread was not
//! scheduled for the whole timeout; [`YieldGate::timed_out`] counts those
//! replies. The run is polled again only once woken, so a wake lost while
//! the run is yielded fails the run's await ([`YieldGate::await_run`]
//! panics). A wake lost while the run waits inside its poll, as a Qodec
//! layer does for a read below, hangs that poll instead: a test over such
//! a layer bounds its own time. The wrapped executor still runs the call
//! at `call`.
//!
//! [`AnswersLater`]: crate::answers_later::AnswersLater

use std::future::{Future, IntoFuture};
use std::ops::Shr;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;
use std::time::{Duration, Instant};

use executor_protocol::{
    Capabilities, ChainPrefix, Close, Compose, Describe, Executor, InstructionBatch,
    InstructionBatchRef, Rejected, Reply, ReplyStatus, WithExecutor,
};

/// The yields of a run awaited by [`YieldGate::await_run`], shared with
/// the [`AnswersOnYield`] below it, and the replies it answered.
#[derive(Debug)]
pub struct YieldGate {
    /// How often the run yielded.
    yields: Mutex<usize>,
    yielded: Condvar,
    timeout: Duration,
    replies: AtomicUsize,
    timed_out: AtomicUsize,
}

impl YieldGate {
    /// A gate whose replies wait at most `timeout` for the run to yield.
    #[must_use]
    pub fn new(timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            yields: Mutex::new(0),
            yielded: Condvar::new(),
            timeout,
            replies: AtomicUsize::new(0),
            timed_out: AtomicUsize::new(0),
        })
    }

    /// The replies answered so far, by a yield or by the timeout.
    #[must_use]
    pub fn replies(&self) -> usize {
        self.replies.load(Ordering::SeqCst)
    }

    /// The replies answered by the timeout: those the run did not yield
    /// for while they were pending, unless the thread that polls the run
    /// was not scheduled for the whole timeout.
    #[must_use]
    pub fn timed_out(&self) -> usize {
        self.timed_out.load(Ordering::SeqCst)
    }

    /// `run`, awaited: polled until it is ready, each `Pending` answering
    /// the replies pending below, then polled again once woken.
    ///
    /// # Type parameters
    ///
    /// - `F`: the awaited run, set by the caller (a chain's run, for one).
    ///
    /// # Panics
    ///
    /// When the run, having yielded, is not woken within [`WAKE_LIMIT`]: a
    /// wake from a reply below was lost on its way to the run.
    pub fn await_run<F: IntoFuture>(&self, run: F) -> F::Output {
        let mut run = pin!(run.into_future());
        let unparks = Arc::new(Unparks {
            thread: std::thread::current(),
            woken: AtomicBool::new(false),
        });
        let waker = Waker::from(Arc::clone(&unparks));
        let mut context = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(output) = run.as_mut().poll(&mut context) {
                return output;
            }
            *self
                .yields
                .lock()
                .expect("no thread panics holding the yields") += 1;
            self.yielded.notify_all();
            unparks.park_until_woken();
        }
    }

    fn seen(&self) -> usize {
        *self
            .yields
            .lock()
            .expect("no thread panics holding the yields")
    }

    /// Waits until the run yields after `seen` yields, or the timeout
    /// passes; counts the reply, and whether the timeout answered it.
    fn wait_for_a_yield_after(&self, seen: usize) {
        let yields = self
            .yields
            .lock()
            .expect("no thread panics holding the yields");
        let (yields, waited) = (self.yielded)
            .wait_timeout_while(yields, self.timeout, |yields| *yields == seen)
            .expect("no thread panics holding the yields");
        drop(yields);
        if waited.timed_out() {
            self.timed_out.fetch_add(1, Ordering::SeqCst);
        }
        self.replies.fetch_add(1, Ordering::SeqCst);
    }
}

/// How long [`YieldGate::await_run`] waits to be woken after the run
/// yields: far longer than a reply below takes, since each is answered at
/// the yield.
pub const WAKE_LIMIT: Duration = Duration::from_secs(10);

/// The waker of a run awaited by [`YieldGate::await_run`]: it unparks the
/// thread that polls the run.
struct Unparks {
    thread: Thread,
    woken: AtomicBool,
}

impl Unparks {
    /// Parks until woken, which a wake before the call already counts as.
    fn park_until_woken(&self) {
        let deadline = Instant::now() + WAKE_LIMIT;
        // `park` may return before an unpark, and an unpark may be taken by
        // a park below (a layer waiting for a read), so the flag decides.
        while !self.woken.swap(false, Ordering::SeqCst) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "the run was not woken within {WAKE_LIMIT:?} of yielding: a wake below was lost"
            );
            std::thread::park_timeout(left);
        }
    }
}

impl Wake for Unparks {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::SeqCst);
        self.thread.unpark();
    }
}

/// `executor`, each reply answered once the run above yields, by `gate`.
///
/// # Type parameters
///
/// - `E`: the wrapped executor, set by the executor
///   [`AnswersOnYield::new`] is given.
pub struct AnswersOnYield<E> {
    executor: E,
    gate: Arc<YieldGate>,
}

impl<E> AnswersOnYield<E> {
    /// `executor`, its replies answered once the run awaited by `gate`
    /// yields.
    pub fn new(executor: E, gate: Arc<YieldGate>) -> Self {
        Self { executor, gate }
    }
}

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for AnswersOnYield<E> {
    type Reply = OnYieldReply<E::Reply>;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        let reply = self.executor.call(batch)?;
        let answer = Arc::new(Answer::default());
        let (gate, answered) = (Arc::clone(&self.gate), Arc::clone(&answer));
        let seen = gate.seen();
        std::thread::spawn(move || {
            gate.wait_for_a_yield_after(seen);
            answered.give();
        });
        Ok(OnYieldReply { reply, answer })
    }
}

impl<E: Describe> Describe for AnswersOnYield<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for AnswersOnYield<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// Whether a reply's events were let through, and the waker to wake then.
#[derive(Default)]
struct Answer {
    given: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl Answer {
    fn give(&self) {
        self.given.store(true, Ordering::SeqCst);
        let waker = self
            .waker
            .lock()
            .expect("no thread panics holding the waker");
        if let Some(waker) = waker.as_ref() {
            waker.wake_by_ref();
        }
    }
}

/// A reply of an [`AnswersOnYield`]: pending until the run yields after
/// the call, or the timeout passes; then the wrapped reply's.
///
/// # Type parameters
///
/// - `R`: the wrapped reply, set by the wrapped executor's reply type.
pub struct OnYieldReply<R> {
    reply: R,
    answer: Arc<Answer>,
}

impl<R: Reply> Reply for OnYieldReply<R> {
    type Item = R::Item;

    fn poll_events(&mut self, out: &mut Vec<R::Item>, waker: &Waker) -> ReplyStatus {
        if !self.answer.given.load(Ordering::SeqCst) {
            *self
                .answer
                .waker
                .lock()
                .expect("no thread panics holding the waker") = Some(waker.clone());
            if !self.answer.given.load(Ordering::SeqCst) {
                return ReplyStatus::Pending;
            }
        }
        self.reply.poll_events(out, waker)
    }
}

/// `>>` for a chain that ends at an [`AnswersOnYield`], as for an
/// [`AnswersLater`](crate::answers_later::AnswersLater) (the same
/// FRICTION).
impl<T, L, E> Shr<AnswersOnYield<E>> for ChainPrefix<T, L>
where
    Self: Compose<AnswersOnYield<E>, WithExecutor>,
{
    type Output = <Self as Compose<AnswersOnYield<E>, WithExecutor>>::Output;

    fn shr(self, executor: AnswersOnYield<E>) -> Self::Output {
        self.compose(executor)
    }
}
