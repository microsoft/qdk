// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The caller chooses how to take a reply (the table in [`Completion`]'s
//! documentation): blocking in `wait`, polling while it does other work,
//! several calls in flight at once, or as an async task that leaves its
//! thread to other tasks. Ported from the prototype's `tests/wake.rs`
//! (w2, w4, w5, w9, w10, w11), over test-double executors whose work runs on
//! other threads; the prototype's `ThreadAdapter` and `Threads` are not in
//! this crate, so a test-double worker spawns a thread per call instead.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use crate::{
    Batch, Capabilities, Close, Completer, Completion, Describe, Executor, Incremental, MakeShot,
    Outcome, Rejected, ShotContext, ShotId, ShotOutcome, ShotSet, Status, SubmissionRef, Value,
    completer, open, wait, wait_async,
};

/// How long a test waits for something that should happen at once before
/// it fails instead of hanging.
const PATIENT: Duration = Duration::from_secs(20);

fn outcome(tag: u32) -> Outcome {
    Outcome {
        tag,
        value: Value::One,
    }
}

/// A waker that counts its wakes and unparks the thread that made it.
struct Counter {
    wakes: AtomicUsize,
    thread: Thread,
}

impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        self.thread.unpark();
    }
}

impl Counter {
    fn new() -> (Arc<Self>, Waker) {
        let counter = Arc::new(Self {
            wakes: AtomicUsize::new(0),
            thread: thread::current(),
        });
        (Arc::clone(&counter), Waker::from(counter))
    }

    fn wakes(&self) -> usize {
        self.wakes.load(Ordering::SeqCst)
    }
}

/// A completion that counts how often it is polled.
struct Counted<C>(C, Arc<AtomicUsize>);

impl<C: Completion> Completion for Counted<C> {
    type Item = C::Item;

    fn poll_events(&mut self, out: &mut Vec<C::Item>, waker: &Waker) -> Status {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.poll_events(out, waker)
    }
}

fn counted<C>(completion: C) -> (Counted<C>, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    (Counted(completion, Arc::clone(&polls)), polls)
}

/// `wait` as the prototype had it before P3: poll, `yield_now`, poll again.
fn spin_wait<C: Completion>(mut completion: C, out: &mut Vec<C::Item>) -> Result<(), String> {
    loop {
        match completion.poll_events(out, Waker::noop()) {
            Status::Done => return Ok(()),
            Status::Failed(reason) => return Err(reason),
            Status::Pending => thread::yield_now(),
        }
    }
}

/// Runs `f` on its own thread and fails the test if it does not end soon: a
/// lost wake would park `wait` forever, and a blocked task would deadlock a
/// single-threaded runtime.
fn ends_soon<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || tx.send(f()));
    rx.recv_timeout(PATIENT)
        .expect("it never ended: a wake was lost, or a thread was blocked")
}

/// An L3 executor whose work is done elsewhere: each call hands its reply's
/// completer to the test, which pushes the outcomes and ends the reply when
/// it likes.
struct Gated {
    shot: ShotId,
    completers: mpsc::Sender<(ShotId, Completer<Outcome>)>,
}

impl Executor<Batch<bool>> for Gated {
    type Response = Incremental<Outcome>;

    fn call(&mut self, _: &[bool]) -> Result<Self::Response, Rejected> {
        let (completer, reply) = completer();
        self.completers
            .send((self.shot, completer))
            .map_err(|_| Rejected::Unavailable("the test has ended".into()))?;
        Ok(reply)
    }
}

impl Close for Gated {
    type Report = ();

    fn close(&mut self) {}
}

/// Makes a [`Gated`] executor per shot.
struct GatedShots(mpsc::Sender<(ShotId, Completer<Outcome>)>);

impl Describe for GatedShots {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl MakeShot<bool> for GatedShots {
    type Shot = Gated;

    fn make(&self, ctx: &ShotContext) -> Gated {
        Gated {
            shot: ctx.shot(),
            completers: self.0.clone(),
        }
    }
}

/// An L3 executor that runs each call on a thread of its own for `.0`, then
/// ends the reply with one outcome: a worker its caller waits for.
struct Worker(Duration);

impl Executor<Batch<bool>> for Worker {
    type Response = Incremental<Outcome>;

    fn call(&mut self, _: &[bool]) -> Result<Self::Response, Rejected> {
        let (completer, reply) = completer();
        let takes = self.0;
        thread::spawn(move || {
            thread::sleep(takes);
            completer.push(outcome(0));
            completer.done();
        });
        Ok(reply)
    }
}

impl Close for Worker {
    type Report = ();

    fn close(&mut self) {}
}

/// Makes a [`Worker`] per shot: a per-shot session whose shots run on
/// threads of their own.
struct Workers(Duration);

impl Describe for Workers {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl MakeShot<bool> for Workers {
    type Shot = Worker;

    fn make(&self, _: &ShotContext) -> Worker {
        Worker(self.0)
    }
}

// ---------------------------------------------------------------- a per-shot session

/// A per-shot session hands over a shot's events as soon as that shot has
/// them, before the submission ends, and wakes its caller each time
/// (prototype w2).
#[test]
fn a_per_shot_session_hands_over_a_shots_events_before_the_submission_ends() {
    let (tx, rx) = mpsc::channel();
    let shots = ShotSet::range(1..=2).expect("shots");
    let mut session = open(&GatedShots(tx), 0, &shots).expect("opens");
    let mut reply = session
        .call(SubmissionRef {
            shots: &shots,
            batch: &[],
        })
        .expect("accepted");
    let mut completers: Vec<_> = rx.try_iter().collect();
    completers.sort_by_key(|(j, _)| *j);
    let Ok([(1, shot_1), (2, shot_2)]) = <[_; 2]>::try_from(completers) else {
        panic!("one completer per shot");
    };

    let (counter, waker) = Counter::new();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, &waker), Status::Pending);
    assert!(out.is_empty());
    // Shot 1's executor has an outcome: it pushes it, and the session wakes
    // its caller.
    shot_1.push(outcome(0));
    assert_eq!(counter.wakes(), 1);
    assert_eq!(
        reply.poll_events(&mut out, &waker),
        Status::Pending,
        "an event, and more to come"
    );
    assert_eq!(
        out,
        [ShotOutcome {
            shot: 1,
            tag: 0,
            value: Value::One
        }]
    );
    shot_1.done();
    assert_eq!(counter.wakes(), 2);
    assert_eq!(
        reply.poll_events(&mut out, &waker),
        Status::Pending,
        "shot 2 is still running"
    );
    shot_2.extend([outcome(0), outcome(1)]);
    assert_eq!(counter.wakes(), 3);
    shot_2.done();
    assert_eq!(reply.poll_events(&mut out, &waker), Status::Done);
    assert_eq!(
        out[1..],
        [
            ShotOutcome {
                shot: 2,
                tag: 0,
                value: Value::One
            },
            ShotOutcome {
                shot: 2,
                tag: 1,
                value: Value::One
            }
        ]
    );
}

// ---------------------------------------------------------------- a synchronous driver: wait

/// `wait` parks until it is woken instead of spinning: a call that ends 30 ms
/// later is polled twice (three times if a stale unpark token wakes the
/// thread early), against hundreds of polls when it spins; and a per-shot
/// session whose shots run on threads wakes its caller at most twice per shot
/// (its events, its end) (prototype w4).
#[test]
fn wait_parks_until_woken_instead_of_spinning() {
    let mut worker = Worker(Duration::from_millis(30));
    let (reply, polls) = counted(worker.call(&[]).expect("accepted"));
    let mut out = Vec::new();
    assert_eq!(wait(reply, &mut out), Ok(()));
    assert_eq!(out, [outcome(0)]);
    let parked = polls.load(Ordering::SeqCst);
    assert!((2..=3).contains(&parked), "{parked} polls");

    let (reply, polls) = counted(worker.call(&[]).expect("accepted"));
    assert_eq!(spin_wait(reply, &mut out), Ok(()));
    let spun = polls.load(Ordering::SeqCst);
    assert!(spun > 100, "{spun} polls when spinning");

    let shots = ShotSet::range(1..=4).expect("shots");
    let mut session = open(&Workers(Duration::from_millis(20)), 0, &shots).expect("opens");
    let (reply, polls) = counted(
        session
            .call(SubmissionRef {
                shots: &shots,
                batch: &[],
            })
            .expect("accepted"),
    );
    let mut out = Vec::new();
    assert_eq!(wait(reply, &mut out), Ok(()));
    assert_eq!(out.len(), 4);
    let parked = polls.load(Ordering::SeqCst);
    assert!(parked <= 2 * 4 + 2, "{parked} polls for 4 shots");
}

/// A completion that wakes its waker during its first poll, then is done.
struct WakesEarly(u32);

impl Completion for WakesEarly {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> Status {
        self.0 += 1;
        if self.0 == 1 {
            waker.wake_by_ref();
            return Status::Pending;
        }
        out.push(self.0);
        Status::Done
    }
}

/// An incremental reply whose first pending poll lets a producer thread end
/// the reply, and returns only once it has: the wake comes before `wait`
/// parks.
struct Handshake {
    reply: Incremental<u32>,
    go: Option<mpsc::Sender<()>>,
    ended: mpsc::Receiver<()>,
}

impl Completion for Handshake {
    type Item = u32;

    fn poll_events(&mut self, out: &mut Vec<u32>, waker: &Waker) -> Status {
        let status = self.reply.poll_events(out, waker);
        if let (Status::Pending, Some(go)) = (&status, self.go.take()) {
            go.send(()).expect("the producer waits for it");
            self.ended.recv().expect("the producer ends the reply");
        }
        status
    }
}

/// A wake that comes before `wait` parks is not lost: `wait` returns on it
/// without a third poll, from the poll itself or from another thread
/// (prototype w5).
#[test]
fn a_wake_before_the_park_is_not_lost() {
    let (polls, out) = ends_soon(|| {
        let (reply, polls) = counted(WakesEarly(0));
        let mut out = Vec::new();
        assert_eq!(wait(reply, &mut out), Ok(()));
        (polls.load(Ordering::SeqCst), out)
    });
    assert_eq!((polls, out), (2, vec![2]));

    let polls = ends_soon(|| {
        let (completer, reply) = completer();
        let (go, started) = mpsc::channel();
        let (finished, ended) = mpsc::channel();
        let producer = thread::spawn(move || {
            started.recv().expect("the caller polls first");
            completer.push(1);
            completer.done(); // wakes the caller, which has not parked yet
            finished.send(()).expect("the caller waits for it");
        });
        let (reply, polls) = counted(Handshake {
            reply,
            go: Some(go),
            ended,
        });
        let mut out = Vec::new();
        assert_eq!(wait(reply, &mut out), Ok(()));
        assert!(producer.join().is_ok());
        assert_eq!(out, [1]);
        polls.load(Ordering::SeqCst)
    });
    assert_eq!(
        polls, 2,
        "the park returned at once, on the token the wake left"
    );
}

// ---------------------------------------------------------------- a driver that polls

/// The waker of one call among several: marks that call ready, then unparks
/// the driver.
struct Flag {
    index: usize,
    ready: Arc<Mutex<Vec<usize>>>,
    driver: Thread,
}

impl Wake for Flag {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.ready.lock().expect("not poisoned").push(self.index);
        self.driver.unpark();
    }
}

/// A driver keeps several calls in flight, sleeps until one of them wakes
/// it, and polls only the calls that woke it (prototype w9).
#[test]
fn a_driver_keeps_several_calls_in_flight_and_sleeps_until_one_wakes_it() {
    let (tx, rx) = mpsc::channel();
    let mut executors: Vec<Gated> = (0..4)
        .map(|shot| Gated {
            shot,
            completers: tx.clone(),
        })
        .collect();
    let mut replies: Vec<Option<Incremental<Outcome>>> = executors
        .iter_mut()
        .map(|executor| Some(executor.call(&[]).expect("accepted")))
        .collect();
    let mut completers: Vec<_> = rx.try_iter().collect();
    assert_eq!(completers.len(), 4);
    completers.sort_by_key(|(j, _)| *j);
    // The calls end in reverse order, 10 ms apart.
    let producer = thread::spawn(move || {
        for (j, completer) in completers.into_iter().rev() {
            thread::sleep(Duration::from_millis(10));
            completer.push(outcome(j));
            completer.done();
        }
    });

    let ready = Arc::new(Mutex::new((0..4).collect::<Vec<usize>>()));
    let wakers: Vec<Waker> = (0..4)
        .map(|index| {
            Waker::from(Arc::new(Flag {
                index,
                ready: Arc::clone(&ready),
                driver: thread::current(),
            }))
        })
        .collect();
    let (mut polls, mut ended, mut out) = ([0; 4], Vec::new(), Vec::new());
    let deadline = Instant::now() + PATIENT;
    while ended.len() < 4 {
        let woken = std::mem::take(&mut *ready.lock().expect("not poisoned"));
        if woken.is_empty() {
            assert!(Instant::now() < deadline, "never woken");
            thread::park_timeout(Duration::from_millis(100));
            continue;
        }
        // Only the calls whose waker was woken are polled.
        for i in woken {
            let Some(reply) = &mut replies[i] else {
                continue;
            };
            polls[i] += 1;
            if reply.poll_events(&mut out, &wakers[i]) != Status::Pending {
                ended.push(i);
                replies[i] = None;
            }
        }
    }
    assert!(producer.join().is_ok());
    assert_eq!(ended, [3, 2, 1, 0], "each call ended when its worker did");
    assert_eq!(out, [outcome(3), outcome(2), outcome(1), outcome(0)]);
    assert!(polls.iter().all(|&p| p <= 3), "polls per call: {polls:?}");
}

/// A driver that polls with the no-op waker does its own work between polls
/// and takes the events when it wants: nothing is lost, and it never calls
/// `wait` (prototype w10).
#[test]
fn a_driver_with_a_noop_waker_does_other_work_and_takes_the_events_when_it_wants() {
    // The executor pushes and ends while the driver is elsewhere; one poll
    // then takes everything pushed meanwhile.
    let (completer, mut reply) = completer::<Outcome>();
    let mut out = Vec::new();
    assert_eq!(reply.poll_events(&mut out, Waker::noop()), Status::Pending);
    let producer = thread::spawn(move || {
        completer.push(outcome(0));
        completer.push(outcome(1));
        completer.done();
    });
    assert!(producer.join().is_ok());
    assert_eq!(reply.poll_events(&mut out, Waker::noop()), Status::Done);
    assert_eq!(out, [outcome(0), outcome(1)]);

    // A call that runs on a worker thread: the driver does units of its own
    // work between polls.
    let mut worker = Worker(Duration::from_millis(20));
    let mut reply = worker.call(&[]).expect("accepted");
    let (mut out, mut work) = (Vec::new(), 0_u32);
    let deadline = Instant::now() + PATIENT;
    loop {
        match reply.poll_events(&mut out, Waker::noop()) {
            Status::Pending => {}
            Status::Done => break,
            Status::Failed(reason) => panic!("{reason}"),
        }
        assert!(Instant::now() < deadline, "never done");
        thread::sleep(Duration::from_millis(1)); // a unit of the driver's own work
        work += 1;
    }
    assert_eq!(out, [outcome(0)]);
    assert!(work >= 1, "the driver worked while the call was pending");
}

// ---------------------------------------------------------------- an async driver

/// The ready queue of [`run_tasks`], and the thread that runs the tasks.
struct Tasks {
    ready: Mutex<VecDeque<usize>>,
    thread: Thread,
}

/// Task `index`'s waker: queues the task again, and unparks the thread.
struct TaskWaker {
    index: usize,
    tasks: Arc<Tasks>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.tasks
            .ready
            .lock()
            .expect("not poisoned")
            .push_back(self.index);
        self.tasks.thread.unpark();
    }
}

type Task<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;

/// A std-only async runtime on one thread, for several tasks: it polls the
/// tasks that were woken, in the order they were woken, and parks only when
/// none is (logging "park"), as a single-threaded runtime does.
fn run_tasks(mut tasks: Vec<Task<'_>>, log: &RefCell<Vec<String>>) {
    let shared = Arc::new(Tasks {
        ready: Mutex::new((0..tasks.len()).collect()),
        thread: thread::current(),
    });
    let wakers: Vec<Waker> = (0..tasks.len())
        .map(|index| {
            Waker::from(Arc::new(TaskWaker {
                index,
                tasks: Arc::clone(&shared),
            }))
        })
        .collect();
    let mut done = vec![false; tasks.len()];
    let deadline = Instant::now() + PATIENT;
    while done.contains(&false) {
        let next = shared.ready.lock().expect("not poisoned").pop_front();
        let Some(i) = next else {
            log.borrow_mut().push("park".into());
            assert!(Instant::now() < deadline, "never woken");
            thread::park_timeout(Duration::from_millis(100));
            continue;
        };
        if !done[i]
            && tasks[i]
                .as_mut()
                .poll(&mut Context::from_waker(&wakers[i]))
                .is_ready()
        {
            done[i] = true;
        }
    }
}

/// Gives the other tasks a turn: pending once, and woken at once.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// An async task that awaits a call leaves its thread to the other tasks
/// until the call ends: task A awaits a reply that ends only after task B's
/// fifth step, and B runs on the same thread meanwhile (prototype w11). Run
/// on a thread of its own, so that a task that blocked its thread deadlocks
/// A and B and fails the test instead of hanging it.
#[test]
fn an_async_task_awaiting_a_call_leaves_its_thread_to_other_tasks() {
    ends_soon(two_tasks_on_one_thread);
}

fn two_tasks_on_one_thread() {
    let (completer, reply) = completer::<Outcome>();
    let (go, gate) = mpsc::channel::<()>();
    let producer = thread::spawn(move || {
        gate.recv_timeout(PATIENT).expect("task B lets it go");
        completer.push(outcome(7));
        completer.done();
    });
    let log = RefCell::new(Vec::new());
    let a = async {
        let (got, end) = wait_async(reply).await;
        assert_eq!((got, end), (vec![outcome(7)], Ok(())));
        log.borrow_mut().push("A done".to_string());
    };
    let b = async {
        for step in 0..10 {
            log.borrow_mut().push(format!("B {step}"));
            if step == 4 {
                go.send(()).expect("the producer waits for it"); // A's call may now end
            }
            YieldNow(false).await;
        }
        log.borrow_mut().push("B done".to_string());
    };
    run_tasks(vec![Box::pin(a), Box::pin(b)], &log);
    assert!(producer.join().is_ok());
    let log = log.into_inner();
    let at = |entry: &str| {
        log.iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("no {entry}: {log:?}"))
    };
    assert_eq!(
        log[0], "B 0",
        "A was polled first, and was pending: {log:?}"
    );
    assert!(at("B 4") < at("A done"), "B ran while A awaited: {log:?}");
    assert!(
        !log[..at("B done")].iter().any(|e| e == "park"),
        "the thread slept while B had work: {log:?}"
    );
}
