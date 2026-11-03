// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::alloc::{GlobalAlloc, Layout, System};
use std::any::type_name;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::f64::consts::FRAC_PI_4;
use std::future::Future;
use std::marker::PhantomData;
use std::ops::Shr;
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use crate::compose::WithAdapter;
use crate::{
    Action, Adapter, AdapterBoundary, Adapters, Angle, Apply, Capabilities, Close, Compose,
    Describe, Driver, Executor, ExecutorLifecycle, Fold, ImmediateReply, IncrementalReply,
    Instruction, InstructionBatch, InstructionBatchRef, InstructionCall, InstructionSet, Message,
    Needs, NoAdapter, Operand, OperandKind, Pauli, Program, Rejected, Reply, ReplyStatus, RngId,
    ShotContext, ShotId, ShotOutcome, ShotSet, TypedInstruction, Value, reply_channel, shot_seed,
    wait, wait_async,
};
use crate::{
    BuildError, BuildFault, BuildFaults, BuiltinDriver, ChainPrefix, ComposedAdapters, JobFaults,
    Run, UpstreamOf, WithExecutor,
};

use test_double::Model;

use OperandKind::{Block, Tag};

/// A waker that counts its wakes.
#[derive(Default)]
struct CountWakes(AtomicUsize);

impl Wake for CountWakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn counting() -> (Arc<CountWakes>, Waker) {
    let count = Arc::new(CountWakes::default());
    (Arc::clone(&count), Waker::from(count))
}

fn wakes(count: &CountWakes) -> usize {
    count.0.load(Ordering::SeqCst)
}

/// A waker that sends a signal at each wake, so that a test waits for a
/// wake from another thread, a signal at a time, rather than for a time.
struct SignalWakes(mpsc::Sender<()>);

impl Wake for SignalWakes {
    fn wake(self: Arc<Self>) {
        // The test may have ended, and dropped the receiver: no one waits.
        let _ = self.0.send(());
    }
}

/// A [`SignalWakes`] waker, and the signals of its wakes.
fn signalling() -> (mpsc::Receiver<()>, Waker) {
    let (signal, signals) = mpsc::channel();
    (signals, Waker::from(Arc::new(SignalWakes(signal))))
}

/// A minimal async runtime of one future, for the tests: polls it with a
/// waker that unparks this thread, and parks the thread between polls,
/// since it has no other task to run. It stands in for an external runtime,
/// so it is kept apart from the crate's own `block_on`, which runs
/// `Run::wait`: a test of an async caller must not use the crate's parker.
fn block_on<Fut: Future>(future: Fut) -> Fut::Output {
    struct UnparkThread(thread::Thread);
    impl Wake for UnparkThread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(UnparkThread(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}

/// The tests' doubles take `bool`s, an instruction that names itself (a
/// read, or not), so that a program of them is composed with no adapter.
impl TypedInstruction for bool {}

/// `set`, each of its instructions declaring what it does, by its
/// mnemonic, so that matching checks it: a `reset` prepares `Z_0`'s state,
/// a `coin` `X_0`'s (so its read is a toss), an `x` flips, a `cx` is a
/// CNOT, an `h` swaps `X_0` and `Z_0`, an `rz` rotates about `Z_0` by its
/// parameter, a `t` by π/4, and a `measure` observes `Z_0`.
fn declared(set: InstructionSet) -> InstructionSet {
    let mnemonics: Vec<String> = set.mnemonics().map(str::to_owned).collect();
    mnemonics.iter().fold(set, |set, mnemonic| {
        let action = match mnemonic.as_str() {
            "reset" => Action::new([], [1]).stabilize([Pauli::z(0)]),
            "coin" => Action::new([], [1]).stabilize([Pauli::x(0)]),
            "x" => Action::new([1], [1]).pauli(Pauli::x(0)),
            "cx" => Action::new([1, 1], [1, 1]).clifford([
                (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
                (Pauli::z(1), Pauli::z(0) * Pauli::z(1)),
            ]),
            "h" => Action::new([1], [1])
                .clifford([(Pauli::x(0), Pauli::z(0)), (Pauli::z(0), Pauli::x(0))]),
            "rz" => Action::new([1], [1]).rotate(Pauli::z(0), Angle::Parameter(0)),
            "t" => Action::new([1], [1]).rotate(Pauli::z(0), Angle::Literal(FRAC_PI_4)),
            "measure" => Action::new([1], []).observe([Pauli::z(0)]),
            other => unreachable!("the tests' sets have no `{other}`"),
        };
        let operands = set
            .operands(mnemonic)
            .expect("the set has its own mnemonics")
            .to_vec();
        set.with_action(mnemonic, operands, action)
    })
}

/// How long a test waits for something that should happen at once before
/// it fails instead of hanging.
const PATIENT: Duration = Duration::from_secs(20);

thread_local! {
    /// The allocations made on this thread while [`allocations`] counts.
    static ALLOCATED: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The system allocator, counting the allocations [`allocations`] asks for,
/// so that a test checks what promises to allocate nothing.
struct CountingAllocator;

impl CountingAllocator {
    fn count() {
        // `try_with`: the thread's counter may be gone as the thread ends.
        let _ = ALLOCATED.try_with(|count| count.set(count.get().map(|n| n + 1)));
    }
}

// SAFETY: each method passes its arguments to the system allocator as it is.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::count();
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::count();
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        Self::count();
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// The allocations `f` makes on this thread.
fn allocations(f: impl FnOnce()) -> usize {
    ALLOCATED.with(|count| count.set(Some(0)));
    f();
    ALLOCATED.with(Cell::take).expect("counting until now")
}

/// Shot `shot`'s readout `tag`, of One.
fn outcome(shot: ShotId, tag: u32) -> ShotOutcome {
    ShotOutcome::new(shot, tag, Value::One)
}

/// A reply that counts how often it is polled.
struct Counted<R>(R, Arc<AtomicUsize>);

impl<R: Reply> Reply for Counted<R> {
    type Item = R::Item;

    fn poll_events(&mut self, out: &mut Vec<R::Item>, waker: &Waker) -> ReplyStatus {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.poll_events(out, waker)
    }
}

fn counted<R>(reply: R) -> (Counted<R>, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    (Counted(reply, Arc::clone(&polls)), polls)
}

/// A reply that ends with no event `after` from now, on a thread of its
/// own: its sender wakes the caller at most once, at the end, so the count
/// of a parked caller's polls does not depend on when its thread runs.
fn ends_after(after: Duration) -> IncrementalReply<u32> {
    let (sender, reply) = reply_channel();
    thread::spawn(move || {
        thread::sleep(after);
        sender.done();
    });
    reply
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
            .expect("no thread panicked holding it")
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
        let next = shared
            .ready
            .lock()
            .expect("no thread panicked holding it")
            .pop_front();
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

/// An executor of batches that runs each call on a thread of its own for `.0`,
/// then ends the reply with one outcome per shot of the batch: a worker its
/// caller waits for, as the state of each shot it is called on.
struct Worker(Duration);

impl Executor<InstructionBatch<bool>> for Worker {
    type Reply = IncrementalReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        let (sender, reply) = reply_channel();
        let takes = self.0;
        let shots = batch.shots.clone();
        thread::spawn(move || {
            thread::sleep(takes);
            for &shot in &shots {
                sender.push(outcome(shot, 0));
            }
            sender.done();
        });
        Ok(reply)
    }
}

impl Close for Worker {
    type Report = ();

    fn close(&mut self) {}
}

/// A report that counts, as a simulator's would.
#[derive(Clone, Debug, Default, PartialEq)]
struct Count(u64);

impl Fold for Count {
    fn fold(&mut self, other: Self) {
        self.0 += other.0;
    }
}

/// The set of the downstream shots: two bits, a coin, a flip and a read,
/// each declaring what it does ([`declaring`]).
fn bits_set() -> InstructionSet {
    let set = InstructionSet::new("bits")
        .with("coin", [Block])
        .with("x", [Block])
        .with("measure", [Block, Tag]);
    declaring(set, EVERY_BIT)
}

/// The set of bits that also reset, numbered before the flip: a set that
/// [`bits_set`] fits, numbered otherwise from the flip on.
fn resetting_bits_set() -> InstructionSet {
    let set = InstructionSet::new("resetting bits")
        .with("coin", [Block])
        .with("reset", [Block])
        .with("x", [Block])
        .with("measure", [Block, Tag]);
    declaring(set, &["coin", "reset", "x", "measure"])
}

/// The same instructions, numbered otherwise.
fn reversed_set() -> InstructionSet {
    let set = InstructionSet::new("reversed")
        .with("measure", [Block, Tag])
        .with("x", [Block])
        .with("coin", [Block]);
    declaring(set, EVERY_BIT)
}

/// Every instruction of [`bits_set`].
const EVERY_BIT: &[&str] = &["coin", "x", "measure"];

/// `set`, with each instruction of `declared` declaring what it does: a
/// coin prepares its bit in `X_0`'s state, so its read is a toss; a reset
/// prepares it in `Z_0`'s; an `x` flips it; a read observes `Z_0`.
fn declaring(set: InstructionSet, declared: &[&str]) -> InstructionSet {
    declared.iter().fold(set, |set, &mnemonic| {
        let (operands, action) = match mnemonic {
            "coin" => (vec![Block], Action::new([], [1]).stabilize([Pauli::x(0)])),
            "reset" => (vec![Block], Action::new([], [1]).stabilize([Pauli::z(0)])),
            "x" => (vec![Block], Action::new([1], [1]).pauli(Pauli::x(0))),
            "measure" => (
                vec![Block, Tag],
                Action::new([1], []).observe([Pauli::z(0)]),
            ),
            _ => unreachable!("the tests declare only the bits' instructions"),
        };
        set.with_action(mnemonic, operands, action)
    })
}

/// The instructions of `set`, as a vendor describes them: each of
/// `undeclared` declaring nothing, a vendor's instruction whose action its
/// set does not say. Another source's set, so it has another name, one per
/// `undeclared`: a set's name is its identity, and a set of the same name
/// would be matched by mnemonic. `set` itself when `undeclared` is empty.
/// The rest are redeclared by [`declaring`], so `set`'s mnemonics are among
/// the bits' (`coin`, `reset`, `x`, `measure`).
fn undeclaring(set: InstructionSet, undeclared: &[&str]) -> InstructionSet {
    if undeclared.is_empty() {
        return set;
    }
    let name = format!("vendor's bits, without {}", undeclared.join(" or "));
    let vendors = set
        .mnemonics()
        .fold(InstructionSet::new(&name), |vendors, mnemonic| {
            let operands = set.operands(mnemonic).expect("a mnemonic of the set");
            vendors.with(mnemonic, operands.iter().copied())
        });
    let declared: Vec<&str> = set
        .mnemonics()
        .filter(|mnemonic| !undeclared.contains(mnemonic))
        .collect();
    declaring(vendors, &declared)
}

/// A call of `set`.
fn call(set: &InstructionSet, mnemonic: &str, operands: &[Operand]) -> InstructionCall {
    set.call(mnemonic, operands.iter().copied())
        .expect("the tests build only calls of the set")
}

/// Flips bit 0 and reads it, then tosses bit 1 and reads it, in `set`'s
/// numbering.
fn flip_and_toss(set: &InstructionSet) -> Vec<Vec<InstructionCall>> {
    vec![
        vec![
            call(set, "x", &[Operand::Block(0)]),
            call(set, "measure", &[Operand::Block(0), Operand::Tag(0)]),
        ],
        vec![
            call(set, "coin", &[Operand::Block(1)]),
            call(set, "measure", &[Operand::Block(1), Operand::Tag(1)]),
        ],
    ]
}

/// Batches of calls by opcode, in order, and what they need: the data of
/// a static program, for a driver written by hand ([`ByHand`]).
struct Batches {
    needs: Needs,
    batches: Vec<Vec<InstructionCall>>,
}

impl Program for Batches {
    type Instruction = InstructionCall;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}

/// A driver written by hand that runs [`Batches`] as the crate's driver
/// runs a static program: each batch to every shot, in order, waiting for
/// each. Unlike the crate's driver, it panics on a refusal or a failure.
struct ByHand;

impl Driver<Batches> for ByHand {
    async fn drive<E>(
        &self,
        program: Batches,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome>
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        let mut outcomes = Vec::new();
        for instructions in &program.batches {
            let reply = executor
                .call(InstructionBatchRef {
                    shots,
                    instructions,
                })
                .expect("the program's batches are valid");
            let (events, end) = wait_async(reply).await;
            end.expect("the shots do not fail");
            outcomes.extend(events);
        }
        outcomes
    }
}

/// A job composed as a chain's prefix is, for the crate's tests: a driver,
/// a program, the shots it runs, and the adapters composed downstream of
/// it (none at first). Composed with an executor, it gives the
/// [`Run`](crate::Run) of the chain of its driver on its adapters and that
/// executor, its job's faults reported as
/// [`BuildFaults`](crate::BuildFaults).
///
/// It takes no seed: the run's seed is given to the last executor when it
/// is built ([`ExecutorFn::seed`](crate::ExecutorFn::seed)), and reaches
/// each adapter that draws (a noise adapter) when the adapter is built
/// against what is downstream of it
/// ([`AdapterBoundary::drawing_seeds`](crate::AdapterBoundary::drawing_seeds)),
/// so that each draws from seeds of its own and the program states only
/// what it runs.
///
/// # Type parameters
///
/// - `T`: the driver ([`Driver`]): the crate's for [`ComposedJob::new`],
///   or the one given to [`ComposedJob::driven`].
/// - `P`: the program ([`Program`]).
/// - `L`: the adapters composed downstream of it by `>>`
///   ([`ComposedAdapters`](crate::ComposedAdapters)); none at first.
pub(crate) struct ComposedJob<T, P, L> {
    driver: T,
    program: P,
    shots: ShotSet,
    adapters: L,
}

impl<P: Program> ComposedJob<BuiltinDriver, P, NoAdapter<P::Instruction>> {
    /// `program`, to run `shots` as the crate's driver drives it. The same
    /// job also runs as any other does, given to a chain of the crate's
    /// driver: `(driver() >> executor)?.run(program, shots)`
    /// ([`Chain::run`](crate::Chain::run)), its faults then reported
    /// together with the shots' ([`JobFaults`](crate::JobFaults)).
    #[must_use]
    pub(crate) fn new(program: P, shots: ShotSet) -> Self {
        Self::driven(BuiltinDriver, program, shots)
    }
}

impl<T, P: Program> ComposedJob<T, P, NoAdapter<P::Instruction>> {
    /// `program`, to run `shots` as `driver` drives it, as given to a
    /// chain of `driver`: `(driver.into_chain() >> executor)?.run(program,
    /// shots)`.
    #[must_use]
    pub(crate) fn driven(driver: T, program: P, shots: ShotSet) -> Self {
        Self {
            driver,
            program,
            shots,
            adapters: NoAdapter::default(),
        }
    }
}

impl<T, P, L> ComposedJob<T, P, L> {
    /// This job, with its adapters replaced by `f` of them.
    pub(crate) fn map_adapters<M>(self, f: impl FnOnce(L) -> M) -> ComposedJob<T, P, M> {
        ComposedJob {
            driver: self.driver,
            program: self.program,
            shots: self.shots,
            adapters: f(self.adapters),
        }
    }

    /// The driver, the program, the shots and the adapters.
    pub(crate) fn into_parts(self) -> (T, P, ShotSet, L) {
        (self.driver, self.program, self.shots, self.adapters)
    }
}

/// Job, adapter: the job, with `y` downstream of its adapters.
impl<T, P: Program, L: Adapters, Y: Adapters<Accepts = L::Sends>> Compose<Y, WithAdapter>
    for ComposedJob<T, P, L>
{
    type Output = ComposedJob<T, P, ComposedAdapters<L, Y>>;

    fn compose(self, y: Y) -> Self::Output {
        self.map_adapters(|adapters| ComposedAdapters::new(adapters, y))
    }
}

/// Job, executor: the chain of the job's driver on the job's adapters
/// upstream of `executor`, given the job's program and shots as its job,
/// so that the needs and the renumbering are the chain's
/// ([`Chain::run`]). The adapters' faults are returned alone, as the
/// chain's are returned before any job is given; otherwise the job's
/// unmet needs, each as [`BuildError::Unmet`].
impl<T, P, L, E> Compose<E, WithExecutor> for ComposedJob<T, P, L>
where
    T: Driver<P>,
    P: Program,
    L: UpstreamOf<E, Accepts = P::Instruction>,
    L::Output: Executor<InstructionBatch<P::Instruction>> + Close,
{
    type Output = Result<Run<T, P, L::Output>, BuildFaults>;

    fn compose(self, executor: E) -> Self::Output {
        let (driver, program, shots, adapters) = self.into_parts();
        let chain = ChainPrefix::<_, NoAdapter<P::Instruction>>::new(driver)
            .map_adapters(|_| adapters)
            .compose(executor)?;
        chain.run(program, shots).map_err(|JobFaults(faults)| {
            let unmet = faults.into_iter().map(|rejected| match rejected {
                Rejected::Unmet(unmet) => BuildFault {
                    component: type_name::<P>(),
                    error: BuildError::Unmet(unmet),
                },
                other => unreachable!(
                    "a composed job's shots are a set, and its driver finds no fault in \
                     its program, so only needs fail: {other}"
                ),
            });
            BuildFaults(unmet.collect())
        })
    }
}

crate::compose::operator::compose_by_shr!(T, P, L; ComposedJob<T, P, L>);

/// Makes shots of two bits, which toss their coins from the last executor's
/// seed of the shot, and record where the last batch they ran was held.
#[derive(Default)]
struct Bits {
    received: Arc<AtomicUsize>,
    /// Whether it states log probabilities: ln ½ per coin tossed.
    log_probability: bool,
    /// Whether its bits also reset ([`resetting_bits_set`]).
    resets: bool,
    /// The instructions whose action its set leaves undeclared
    /// ([`undeclaring`]); none by default.
    undeclared: &'static [&'static str],
    /// Whether it describes no set at all.
    undescribed: bool,
}

impl Bits {
    fn set(&self) -> InstructionSet {
        let set = if self.resets {
            resetting_bits_set()
        } else {
            bits_set()
        };
        undeclaring(set, self.undeclared)
    }
}

impl Describe for Bits {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
            .with_qubits(2)
            .with_resumable_shots(true)
            .with_replayable(true)
            .with_resets(self.resets)
            .with_instruction_set((!self.undescribed).then(|| Arc::new(self.set())))
            .with_log_probability(self.log_probability)
    }
}

impl Model for Bits {
    type Instruction = InstructionCall;
    type State = BitsState;

    fn state(&self, ctx: &ShotContext) -> Result<BitsState, String> {
        Ok(BitsState {
            shot: ctx.shot(),
            bits: [false; 2],
            rng: ctx.seed(),
            log_p: 0.0,
            set: self.set(),
            received: Arc::clone(&self.received),
        })
    }

    fn log_p(state: &BitsState) -> Option<f64> {
        Some(state.log_p)
    }
}

/// One shot's two bits: an executor of that shot alone.
struct BitsState {
    shot: ShotId,
    bits: [bool; 2],
    rng: u64,
    log_p: f64,
    set: InstructionSet,
    received: Arc<AtomicUsize>,
}

impl BitsState {
    /// Runs `batch` on the bits, giving each readout's tag and value.
    fn run(&mut self, batch: &[InstructionCall]) -> Result<Vec<(u32, Value)>, Rejected> {
        self.set.check_batch(batch)?;
        self.received.store(batch.as_ptr().addr(), Ordering::SeqCst);
        let mut outcomes = Vec::new();
        for call in batch {
            let operand = |i: usize| match call.operands[i] {
                Operand::Block(n) | Operand::Tag(n) => n,
                _ => unreachable!("the set takes blocks and tags"),
            };
            let bit = &mut self.bits[operand(0) as usize];
            match self.set.mnemonic(call.opcode) {
                Some("coin") => {
                    self.rng = shot_seed(self.rng, 0);
                    *bit = self.rng & 1 == 1;
                    self.log_p += 0.5_f64.ln();
                }
                Some("x") => *bit = !*bit,
                Some("reset") => *bit = false,
                _ => outcomes.push((operand(1), (*bit).into())),
            }
        }
        Ok(outcomes)
    }
}

impl Executor<InstructionBatch<InstructionCall>> for BitsState {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        batch.shots.check_only(self.shot)?;
        let shot = self.shot;
        let outcomes = self.run(batch.instructions)?;
        Ok(ImmediateReply::done(
            (outcomes.into_iter())
                .map(|(tag, value)| ShotOutcome::new(shot, tag, value))
                .collect(),
        ))
    }
}

/// Each shot reports a count of one, so an executor's report counts them.
impl Close for BitsState {
    type Report = Count;

    fn close(&mut self) -> Count {
        Count(1)
    }
}

/// A toy ensemble executor: the shots of a run share one state of two bits
/// drawn from its seed, so every shot reads the same values. Its state
/// carries over from one call to the next, so it keeps its failures final
/// and refuses calls once closed by hand, on its [`ExecutorLifecycle`].
struct SharedState {
    /// Its one state, ended at its first close.
    state: Option<BitsState>,
    lifecycle: ExecutorLifecycle,
    /// The calls it ran, counted here for `fails_at`: its lifecycle counts
    /// the calls it admits, refused ones too, but does not tell.
    calls: u64,
    /// The call at which its device stops, after the batch ran; `None`:
    /// never.
    fails_at: Option<u64>,
}

impl SharedState {
    fn new(seed: u64) -> Self {
        Self {
            state: Some(
                Bits::default()
                    .state(&ShotContext::last_executor(seed, 0))
                    .expect("bits always make a state"),
            ),
            lifecycle: ExecutorLifecycle::default(),
            calls: 0,
            fails_at: None,
        }
    }
}

/// What its bits offer, but neither resumable nor replayable shots: its
/// shots share one state on one seed, so a call on one shot changes the
/// others, and a shot's values depend on how the shots' batches are split
/// into calls.
impl Describe for SharedState {
    fn capabilities(&self) -> Capabilities {
        (Bits::default().capabilities())
            .with_resumable_shots(false)
            .with_replayable(false)
    }
}

impl Executor<InstructionBatch<InstructionCall>> for SharedState {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        self.lifecycle.admit()?;
        let state = self
            .state
            .as_mut()
            .expect("its lifecycle admits a call only before close, which ends the state");
        let outcomes = state.run(batch.instructions)?;
        self.calls += 1;
        if self.fails_at == Some(self.calls) {
            self.lifecycle.fail("the device stopped");
            return Ok(ImmediateReply::failed(
                Vec::new(),
                "the device stopped".into(),
            ));
        }
        Ok(ImmediateReply::done(
            batch
                .shots
                .iter()
                .flat_map(|&shot| {
                    (outcomes.iter()).map(move |&(tag, value)| ShotOutcome::new(shot, tag, value))
                })
                .collect(),
        ))
    }
}

/// One state for all the shots, so it reports a count of one, at its first
/// close, which ends the state; a second close reports nothing.
impl Close for SharedState {
    type Report = Count;

    fn close(&mut self) -> Count {
        self.lifecycle.close();
        self.state
            .take()
            .map_or_else(Count::default, |mut state| state.close())
    }
}

/// An adapter that accepts [`bits_set`] and sends each call down by its
/// mnemonic in the set it sends, a set of its own; when it adopts, it sends
/// in the numbering of the set downstream instead. It records where the
/// last batch it sent was held.
struct Relabel {
    accepts: Arc<InstructionSet>,
    sends: Arc<InstructionSet>,
    /// The set whose numbering its calls are sent in.
    target: Arc<InstructionSet>,
    /// Whether it sends in the downstream numbering when it can.
    adopts: bool,
    sent: Arc<AtomicUsize>,
}

impl Relabel {
    /// Sending `sends`, or in the downstream numbering when `adopts`.
    fn new(sends: InstructionSet, adopts: bool) -> Self {
        let sends = Arc::new(sends);
        Self {
            accepts: Arc::new(bits_set()),
            target: Arc::clone(&sends),
            sends,
            adopts,
            sent: Arc::default(),
        }
    }

    /// This adapter, sending its calls in `target`'s numbering.
    fn with(&self, target: &Arc<InstructionSet>) -> Self {
        Self {
            accepts: Arc::clone(&self.accepts),
            sends: Arc::clone(&self.sends),
            target: Arc::clone(target),
            adopts: self.adopts,
            sent: Arc::clone(&self.sent),
        }
    }
}

impl Adapter for Relabel {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.sends))
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.accepts))
    }

    /// When it adopts, it sends in the downstream numbering.
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        let downstream = boundary.downstream().instruction_set.clone();
        match downstream {
            Some(set) if self.adopts && boundary.adopt_downstream_numbering().is_some() => Self {
                sends: Arc::clone(&set),
                ..self.with(&set)
            },
            _ => self,
        }
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Relabel {
    type Output = Relabeled<E>;

    fn apply(self, downstream: E) -> Relabeled<E> {
        Relabeled {
            relabel: self,
            inner: downstream,
            batch: Vec::new(),
        }
    }
}

struct Relabeled<E> {
    relabel: Relabel,
    inner: E,
    batch: Vec<InstructionCall>,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for Relabeled<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        let shots = batch.shots;
        let batch = batch.instructions;
        let Relabel {
            accepts,
            target,
            sent,
            ..
        } = &self.relabel;
        accepts.check_batch(batch)?;
        self.batch.clear();
        for call in batch {
            let mnemonic = accepts.mnemonic(call.opcode).expect("checked above");
            let opcode = target.opcode(mnemonic).expect("the sets fit");
            self.batch
                .push(InstructionCall::new(opcode, call.operands.iter().copied()));
        }
        sent.store(self.batch.as_ptr().addr(), Ordering::SeqCst);
        self.inner.call(InstructionBatchRef {
            shots,
            instructions: &self.batch,
        })
    }
}

impl<E: Close> Close for Relabeled<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// A code that states its whole offer upstream from scratch, a logical
/// device of four qubits on the bits' set, assigned at once: unlike a real
/// adapter, which starts it from downstream's
/// ([`AdapterBoundary::offered_upstream`]), it drops every field `compose`
/// derives (the run's seed, the ids drawn with below), so that the tests
/// see `compose` put them back.
struct StatesItsOfferFromScratch;

impl Adapter for StatesItsOfferFromScratch {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        *boundary.offered_upstream() = Capabilities::default()
            .with_qubits(4)
            .with_instruction_set(Some(Arc::new(bits_set())));
        self
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E>
    for StatesItsOfferFromScratch
{
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// An adapter written by hand whose closure maps the executor downstream
/// of it to the executor it gives ([`Apply`]): for the tests of what
/// composes an adapter written by hand with an executor. It changes no
/// capability but one: what its executor draws, or skips of the downstream
/// calls, is unknown to the log probability the executor downstream of it
/// reports, so it states none. Made by [`map_executor`].
struct MapExecutor<I, F> {
    map: F,
    draws: Option<RngId>,
    instructions: PhantomData<fn() -> I>,
}

/// The adapter that maps the executor downstream of it, of instructions
/// `I`, with `map`.
fn map_executor<I, F>(map: F) -> MapExecutor<I, F> {
    MapExecutor {
        map,
        draws: None,
        instructions: PhantomData,
    }
}

impl<I, F> MapExecutor<I, F> {
    /// This adapter, drawing random numbers with `id`
    /// ([`Adapter::draws`]).
    fn drawing(mut self, id: RngId) -> Self {
        self.draws = Some(id);
        self
    }
}

impl<I: Instruction, F> Adapter for MapExecutor<I, F> {
    type Accepts = I;
    type Sends = I;

    fn draws(&self) -> Option<RngId> {
        self.draws
    }

    /// It offers no `log_probability`: what its executor draws is unknown to
    /// the log probability its downstream executor reports.
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        boundary.offered_upstream().log_probability = false;
        self
    }
}

impl<I, F, E, O> Apply<E> for MapExecutor<I, F>
where
    I: Instruction,
    F: FnOnce(E) -> O,
    O: Executor<InstructionBatch<I>> + Close,
{
    type Output = O;

    fn apply(self, downstream: E) -> O {
        (self.map)(downstream)
    }
}

/// `>>` with an adapter on the right, as for the crate's adapters.
impl<I, F, Rhs> Shr<Rhs> for MapExecutor<I, F>
where
    Rhs: Adapters,
    Self: Compose<Rhs, WithAdapter>,
{
    type Output = <Self as Compose<Rhs, WithAdapter>>::Output;

    fn shr(self, rhs: Rhs) -> Self::Output {
        self.compose(rhs)
    }
}

/// An executor, around `E`, whose close panics, as one with a bug may: for
/// the tests of what closes it once.
struct PanicsOnClose<E>(E);

impl<M: Message, E: Executor<M>> Executor<M> for PanicsOnClose<E> {
    type Reply = E::Reply;

    fn call(&mut self, message: M::Ref<'_>) -> Result<E::Reply, Rejected> {
        self.0.call(message)
    }
}

impl<E: Describe> Describe for PanicsOnClose<E> {
    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }
}

impl<E: Close> Close for PanicsOnClose<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        panic!("the close has a bug")
    }
}

/// `instructions` on shot 0 alone.
fn on_shot_0<I>(instructions: &[I]) -> InstructionBatchRef<'_, I> {
    const SHOT_0: &ShotSet = &ShotSet::one(0);
    InstructionBatchRef {
        shots: SHOT_0,
        instructions,
    }
}

fn shots(ids: &[u32]) -> ShotSet {
    ShotSet::new(ids.to_vec()).expect("the tests' shots are distinct")
}

/// The values of `outcomes`, in their order.
fn values(outcomes: &[ShotOutcome]) -> Vec<(u32, u32, Value)> {
    outcomes.iter().map(|o| (o.shot, o.tag, o.value)).collect()
}

/// Runs `batch` on the shots `ids` of `executor`, closes it, and gives their
/// outcomes.
fn run_and_close<E: Executor<InstructionBatch<InstructionCall>> + Close>(
    mut executor: E,
    ids: &[u32],
    batch: &[InstructionCall],
) -> Vec<ShotOutcome> {
    let shots = shots(ids);
    let reply = executor
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: batch,
        })
        .expect("the batch is valid");
    let mut outcomes = Vec::new();
    wait(reply, &mut outcomes).expect("the shots do not fail");
    executor.close();
    outcomes
}

mod adapter;
mod capabilities;
mod compose;
mod conformance;
mod driver;
mod executor;
mod facilities;
mod instruction_set;
mod needs;
mod rejected;
mod reply;
mod seeds;
mod shot_conditions;
mod shot_outcome;
mod shot_set;
mod test_double;
mod wire_codes;
