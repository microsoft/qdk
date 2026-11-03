//! Devices that misbehave on cue, below a chain: coins that fail at a
//! chosen shot and batch, coins whose batches wait at a gate until the
//! test opens it, a wrapper that counts an executor's closes and
//! drops, and can open such a gate as it is closed, a wrapper that
//! refuses one call of an executor ([`RefusesCall`]), and coins that lose
//! chosen readouts ([`scripted_device`]), also as the Qodec runtime's
//! bottom backend ([`ScriptedBackend`]).

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::Duration;

use executor_protocol::{
    CallInput, Capabilities, Close, Describe, Executor, ExecutorLogic, ExecutorPlacement,
    InstructionBatch, InstructionBatchRef, InstructionCall, InstructionSet, MakeState, OnePerShot,
    OnePerShotExecutor, OnePerShotPlacedExecutor, Operand, Outcome, Outcomes, OwnThread,
    PlacementJob, Rejected, ShotContext, ShotId, executor_fn,
};
use qodec_demo::boundary::{Boundary, Ins, value};
use qodec_demo::coin::coin;
use qodec_runtime::{Backend, BasicReadout, BatchResult, Bit, OwnedParam, Readout, Status};

/// A gate a shot's batch waits at until the test opens it, counting the
/// batches that arrived at it.
#[derive(Clone, Debug, Default)]
pub struct Gate(Arc<(Mutex<Held>, Condvar)>);

/// Whether a [`Gate`] is open, and how many batches arrived at it.
#[derive(Debug, Default)]
struct Held {
    open: bool,
    arrived: usize,
}

impl Gate {
    /// Lets every batch waiting at the gate, and every later one, pass.
    pub fn open(&self) {
        let (held, changed) = &*self.0;
        held.lock().unwrap_or_else(PoisonError::into_inner).open = true;
        changed.notify_all();
    }

    /// Arrives at the gate, and waits until it is open.
    pub fn pass(&self) {
        let (held, changed) = &*self.0;
        let mut held = held.lock().unwrap_or_else(PoisonError::into_inner);
        held.arrived += 1;
        changed.notify_all();
        while !held.open {
            held = changed.wait(held).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Waits until `batches` batches arrived at the gate, open or not.
    pub fn wait_for_arrivals(&self, batches: usize) {
        let (held, changed) = &*self.0;
        let mut held = held.lock().unwrap_or_else(PoisonError::into_inner);
        while held.arrived < batches {
            held = changed.wait(held).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The batches waiting at the gate: every one that arrived while it is
    /// closed, none once it is open.
    pub fn waiting(&self) -> usize {
        let held = self.0.0.lock().unwrap_or_else(PoisonError::into_inner);
        if held.open { 0 } else { held.arrived }
    }
}

/// What the device's failure says, in every reason that names it.
pub const LOST: &str = "the device lost its qubits";

/// Where the device fails: at the shot's `batch`-th batch (from 0), after
/// waiting at `gate`, if any; then it opens `told`, if any.
#[derive(Clone, Debug)]
pub struct FailAt {
    pub shot: ShotId,
    pub batch: u64,
    pub gate: Option<Gate>,
    pub told: Option<Gate>,
}

impl FailAt {
    /// At the shot's `batch`-th batch, with no gate.
    pub fn new(shot: ShotId, batch: u64) -> Self {
        Self {
            shot,
            batch,
            gate: None,
            told: None,
        }
    }

    /// The same failure, waiting at `gate` first.
    pub fn held_at(mut self, gate: &Gate) -> Self {
        self.gate = Some(gate.clone());
        self
    }

    /// The same failure, on [`failing_coins_on_threads`], opening `told`
    /// once the device recorded it, so that a test waiting at it knows
    /// that the device refuses every later call.
    pub fn telling(mut self, told: &Gate) -> Self {
        self.told = Some(told.clone());
        self
    }

    /// The reason the device gives.
    pub fn reason(&self) -> String {
        format!("{LOST}: shot {}, batch {}", self.shot, self.batch)
    }
}

/// One shot of the failing device: the coins of [`coin_device`]'s shot
/// (`qodec_demo::coin::coin_device`), and the batches it ran.
#[derive(Debug)]
pub struct FailingShot {
    shot: ShotId,
    seed: u64,
    drawn: u64,
    batches: u64,
}

thread_local! {
    /// Whether the device's logic failed its batch on this thread, in the
    /// job [`TellingThread`] runs.
    static FAILED_HERE: Cell<bool> = const { Cell::new(false) };
}

/// A thread of its own per shot, as [`OwnThread`], that opens `told`, if
/// any, once the job in which the shot's batch failed returned: the placed
/// device records a failure in that job, so a call made once `told` is
/// open is refused for it.
#[derive(Clone, Debug)]
pub struct TellingThread {
    told: Option<Gate>,
}

impl ExecutorPlacement for TellingThread {
    fn start(self) -> Box<dyn FnMut(PlacementJob) + Send> {
        let mut thread = OwnThread.start();
        Box::new(move |job| {
            let told = self.told.clone();
            thread(Box::new(move || {
                job();
                if FAILED_HERE.replace(false)
                    && let Some(told) = told
                {
                    told.open();
                }
            }));
        })
    }
}

/// The device's logic: a coin per tag, in call order, as
/// `qodec_demo::coin::coin_device` tosses them, but for the batch `fail`
/// names, which fails.
fn toss_or_fail(
    fail: FailAt,
) -> impl FnMut(
    &mut FailingShot,
    &CallInput<'_, InstructionCall>,
    &mut Outcomes<'_>,
) -> Result<(), String>
+ Clone
+ Send {
    move |shot, input, outcomes| {
        let batch = shot.batches;
        shot.batches += 1;
        if shot.shot == fail.shot && batch == fail.batch {
            if let Some(gate) = &fail.gate {
                gate.pass();
            }
            if fail.told.is_some() {
                FAILED_HERE.set(true);
            }
            return Err(fail.reason());
        }
        for call in input.instructions() {
            for operand in call.operands.iter() {
                if let Operand::Tag(tag) = *operand {
                    let value = coin(shot.seed, shot.drawn).into();
                    shot.drawn += 1;
                    outcomes.push(Outcome::new(tag, value));
                }
            }
        }
        Ok(())
    }
}

/// The promise of the coins: resumable shots, replayable, `set`.
fn promise(set: Arc<InstructionSet>) -> Capabilities {
    Capabilities::default()
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_instruction_set(Some(set))
}

/// A shot's state, made from its context.
fn make(shot: ShotContext) -> Result<FailingShot, Rejected> {
    Ok(FailingShot {
        shot: shot.shot(),
        seed: shot.seed(),
        drawn: 0,
        batches: 0,
    })
}

/// Coins of `set` in a run of seed `seed`, a state per shot, answering
/// within their call, which fail where `fail` says.
pub fn failing_coins(
    set: Arc<InstructionSet>,
    seed: u64,
    fail: FailAt,
) -> OnePerShotExecutor<
    InstructionCall,
    FailingShot,
    impl MakeState<FailingShot>,
    impl ExecutorLogic<InstructionCall, FailingShot>,
> {
    executor_fn(promise(set), make, toss_or_fail(fail))
        .seed(seed)
        .shots(OnePerShot)
}

/// [`failing_coins`], each shot on a thread of its own
/// (`.run_on(TellingThread).shots(OnePerShot)`): they answer later.
pub fn failing_coins_on_threads(
    set: Arc<InstructionSet>,
    seed: u64,
    fail: FailAt,
) -> OnePerShotPlacedExecutor<
    InstructionCall,
    FailingShot,
    impl MakeState<FailingShot> + Clone + Send,
    impl ExecutorLogic<InstructionCall, FailingShot> + Clone + Send,
    TellingThread,
> {
    let told = fail.told.clone();
    executor_fn(promise(set), make, toss_or_fail(fail))
        .seed(seed)
        .run_on(TellingThread { told })
        .shots(OnePerShot)
}

/// Coins of `set` in a run of seed `seed`, each shot on a thread of its
/// own, every batch of which waits at `hold` first: calls stay in flight
/// until the test opens it. They never fail.
pub fn held_coins_on_threads(
    set: Arc<InstructionSet>,
    seed: u64,
    hold: Gate,
) -> OnePerShotPlacedExecutor<
    InstructionCall,
    FailingShot,
    impl MakeState<FailingShot> + Clone + Send,
    impl ExecutorLogic<InstructionCall, FailingShot> + Clone + Send,
    OwnThread,
> {
    let mut toss = toss_or_fail(FailAt::new(ShotId::MAX, u64::MAX));
    let logic = move |shot: &mut FailingShot,
                      input: &CallInput<'_, InstructionCall>,
                      outcomes: &mut Outcomes<'_>| {
        hold.pass();
        toss(shot, input, outcomes)
    };
    executor_fn(promise(set), make, logic)
        .seed(seed)
        .run_on(OwnThread)
        .shots(OnePerShot)
}

/// How often an executor was closed, and dropped, and the batches waiting
/// at its gate when it was first closed.
#[derive(Debug, Default)]
pub struct Ends {
    closes: AtomicUsize,
    drops: AtomicUsize,
    held_at_close: AtomicUsize,
}

impl Ends {
    /// The times it was closed.
    pub fn closes(&self) -> usize {
        self.closes.load(Ordering::SeqCst)
    }

    /// The times it was dropped.
    pub fn drops(&self) -> usize {
        self.drops.load(Ordering::SeqCst)
    }

    /// The batches waiting at the gate a [`Counted`] opens as it is first
    /// closed ([`Counted::opening_at_close`]), as its close reached it.
    pub fn held_at_close(&self) -> usize {
        self.held_at_close.load(Ordering::SeqCst)
    }
}

/// `executor`, its closes and its drop counted in an [`Ends`].
pub struct Counted<E> {
    executor: E,
    ends: Arc<Ends>,
    opens: Option<Gate>,
}

impl<E> Counted<E> {
    /// `executor`, counted in `ends`.
    pub fn new(executor: E, ends: &Arc<Ends>) -> Self {
        Self {
            executor,
            ends: Arc::clone(ends),
            opens: None,
        }
    }

    /// The same, opening `hold` once it is first closed: once the close of
    /// `executor` returned, or once it has waited [`OPEN_AFTER`] (200 ms)
    /// in it. So the calls of `executor` held at `hold` are in flight when
    /// its close starts. A close that waits for them cannot return while
    /// they are held, so it always opens `hold` through the wait of
    /// [`OPEN_AFTER`], and takes at least that long; the gate opens as the
    /// close returns only for a close that does not wait, which returns
    /// (unless it takes longer than [`OPEN_AFTER`]) before they are let go.
    /// It records how many were held as its close started.
    pub fn opening_at_close(mut self, hold: &Gate) -> Self {
        self.opens = Some(hold.clone());
        self
    }
}

/// How long the close of a [`Counted`] that opens a gate waits for the
/// executor's close to return before it opens the gate: the wait through
/// which a close that waits for the held calls lets them go.
pub const OPEN_AFTER: Duration = Duration::from_millis(200);

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for Counted<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        self.executor.call(batch)
    }
}

impl<E: Describe> Describe for Counted<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for Counted<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.ends.closes.fetch_add(1, Ordering::SeqCst);
        let Some(hold) = self.opens.take() else {
            return self.executor.close();
        };
        (self.ends.held_at_close).store(hold.waiting(), Ordering::SeqCst);
        let (returned, has_returned) = mpsc::channel::<()>();
        let opener = thread::spawn(move || {
            let _ = has_returned.recv_timeout(OPEN_AFTER);
            hold.open();
        });
        let report = self.executor.close();
        drop(returned);
        opener.join().expect("the opener opens the gate");
        report
    }
}

impl<E> Drop for Counted<E> {
    fn drop(&mut self) {
        self.ends.drops.fetch_add(1, Ordering::SeqCst);
    }
}

/// `executor`, refusing its call number `refused` (from 0) whatever it
/// holds, as a device turns a call away for a reason of its own; it passes
/// every other call on.
pub struct RefusesCall<E> {
    executor: E,
    refused: usize,
    calls: usize,
}

impl<E> RefusesCall<E> {
    /// `executor`, refusing its call number `refused`.
    pub fn new(executor: E, refused: usize) -> Self {
        Self {
            executor,
            refused,
            calls: 0,
        }
    }
}

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for RefusesCall<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        let at = self.calls;
        self.calls += 1;
        if at == self.refused {
            return Err(Rejected::Malformed(format!("the device refuses call {at}")));
        }
        self.executor.call(batch)
    }
}

impl<E: Describe> Describe for RefusesCall<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for RefusesCall<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// How a scripted device answers a shot's `n`-th readout, given the
/// shot's seed (`shot_seed(seed, j)`): a bit, or none (a lost readout).
pub type Script = fn(seed: u64, n: u64) -> Bit;

/// One shot of a scripted device: its seed, and how many readouts it made.
#[derive(Debug)]
pub struct ScriptedShot {
    seed: u64,
    drawn: u64,
    script: Script,
}

impl ScriptedShot {
    /// The shot's next readout, as the script says.
    fn draw(&mut self) -> Bit {
        let n = self.drawn;
        self.drawn += 1;
        (self.script)(self.seed, n)
    }
}

/// A device of `set` that answers each readout as `script` says, a state
/// per shot, answering within its call. A readout with no bit comes up as
/// `Value::Missing`.
pub fn scripted_device(
    set: Arc<InstructionSet>,
    seed: u64,
    script: Script,
) -> OnePerShotExecutor<
    InstructionCall,
    ScriptedShot,
    impl MakeState<ScriptedShot>,
    impl ExecutorLogic<InstructionCall, ScriptedShot>,
> {
    let make = move |shot: ShotContext| {
        Ok(ScriptedShot {
            seed: shot.seed(),
            drawn: 0,
            script,
        })
    };
    let answer = |shot: &mut ScriptedShot,
                  input: &CallInput<'_, InstructionCall>,
                  outcomes: &mut Outcomes<'_>| {
        for call in input.instructions() {
            for operand in call.operands.iter() {
                if let Operand::Tag(tag) = *operand {
                    outcomes.push(Outcome::new(tag, value(shot.draw())));
                }
            }
        }
        Ok(())
    };
    executor_fn(promise(set), make, answer)
        .seed(seed)
        .shots(OnePerShot)
}

/// [`scripted_device`]'s shot as the Qodec runtime's bottom backend, as
/// `CoinBackend` is `coin_device`'s.
pub struct ScriptedBackend {
    shot: ScriptedShot,
    bottom: Arc<Boundary>,
}

impl ScriptedBackend {
    /// The shot whose seed is `seed` (`shot_seed(seed, j)`), each
    /// instruction's readouts read from `bottom`, answered by `script`.
    pub fn new(seed: u64, bottom: Arc<Boundary>, script: Script) -> Self {
        Self {
            shot: ScriptedShot {
                seed,
                drawn: 0,
                script,
            },
            bottom,
        }
    }
}

impl Backend<usize, OwnedParam, BasicReadout> for ScriptedBackend {
    type Error = std::convert::Infallible;

    fn execute_batch(
        &mut self,
        instructions: &[Ins],
    ) -> Result<BatchResult<BasicReadout>, Self::Error> {
        let mut results = smallvec::SmallVec::new();
        for ins in instructions {
            let layout = &self.bottom.layouts[ins.instruction_id];
            if layout.readouts == 0 {
                continue;
            }
            let bits: Vec<_> = (0..layout.readouts).map(|_| self.shot.draw()).collect();
            let (outs, flags) = bits.split_at(layout.observes);
            results.push(BasicReadout::new(outs, flags, Status::default()));
        }
        Ok(BatchResult { results })
    }
}
