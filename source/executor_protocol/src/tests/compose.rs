// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod adapters;
mod chain;
mod run;

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{
    Bits, Count, EVERY_BIT, Relabel, SharedState, Worker, bits_set, block_on, declaring, ends_soon,
    flip_and_toss, reversed_set, run_and_close, shots, undeclaring, values,
};
use crate::tests::map_executor;
use crate::tests::test_double::TestDoubleModel;
use crate::tests::test_double::{Model, TestDoubleExecutor};
use crate::{
    Adapter, AdapterBoundary, AdapterExecutor, AdapterFn, AdapterLogic, AdaptiveProgram, Apply,
    Below, BuildError, BuildFaults, CallInput, Capabilities, Chain, ChainHead, Close, ComponentId,
    ComposedJob, DefaultDriver, Describe, Driver, Executor, ExecutorLogic, InstructionBatch,
    InstructionBatchRef, InstructionCall, InstructionMismatch, InstructionSet, JobFaults, LogP,
    MakeState, Need, Needs, NoCheck, OnePerShot, OnePerShotAdapter, OnePerShotExecutor,
    OnePerShotPlacedExecutor, OperandKind, Outcome, Outcomes, OwnThread, Rejected, ShotContext,
    ShotId, ShotOutcome, ShotSet, StaticProgram, TypedInstruction, UnmetNeed, Value, adapter_fn,
    compose, driver, executor_fn, wait, wait_async,
};

use OperandKind::{Block, Tag};

/// The log of the [`Logger`]s of a test.
type Log = Arc<Mutex<Vec<String>>>;

fn entries(log: &Log) -> Vec<String> {
    log.lock().expect("no test thread panicked").clone()
}

/// An adapter that passes the calls it is called with down, and logs each
/// batch by its name and each of its shots, in the batch's order: one entry
/// per shot.
struct Logger {
    name: &'static str,
    log: Log,
}

impl Adapter for Logger {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Logger {
    type Output = Logged<E>;

    fn apply(self, downstream: E) -> Logged<E> {
        Logged {
            logger: self,
            inner: downstream,
        }
    }
}

struct Logged<E> {
    logger: Logger,
    inner: E,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for Logged<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        let Logger { name, log } = &self.logger;
        let mut log = log.lock().expect("no test thread panicked");
        log.extend(batch.shots.iter().map(|j| format!("{name} {j}")));
        drop(log);
        self.inner.call(batch)
    }
}

impl<E: Close> Close for Logged<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// A program that sends each of its batches to every shot, in turn, and
/// gives their outcomes.
struct Program {
    batches: Vec<Vec<InstructionCall>>,
    needs: Needs,
}

impl Program {
    /// [`flip_and_toss`] in `set`'s numbering, needing `set`.
    fn flip_and_toss(set: InstructionSet) -> Self {
        Self {
            batches: flip_and_toss(&set),
            needs: Needs::default().with_instruction_set(Some(Arc::new(set))),
        }
    }
}

impl AdaptiveProgram for Program {
    type Instruction = InstructionCall;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }

    async fn drive<E>(self, executor: &mut E, shots: &ShotSet) -> Vec<ShotOutcome>
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        let mut outcomes = Vec::new();
        for batch in &self.batches {
            let reply = executor
                .call(InstructionBatchRef {
                    shots,
                    instructions: batch,
                })
                .expect("the program's batches are valid");
            let (events, end) = wait_async(reply).await;
            end.expect("the shots do not fail");
            outcomes.extend(events);
        }
        outcomes
    }
}

fn logger(name: &'static str, log: &Log) -> Logger {
    Logger {
        name,
        log: Arc::clone(log),
    }
}

/// Two adapters composed, a counter upstream of a debugger: a call reaches the
/// counter first.
#[test]
fn adapters_composed_put_the_left_upstream_of_the_right() {
    let log = Log::default();
    let adapters = compose(logger("counter", &log), logger("debugger", &log));
    let executor =
        compose(adapters, TestDoubleExecutor::with_model(Bits::default(), 7)).expect("loggers fit");
    run_and_close(executor, &[0], &flip_and_toss(&bits_set())[0]);
    assert_eq!(entries(&log), ["counter 0", "debugger 0"]);
}

/// The same three components, grouped either way, make the same chain: a
/// call meets them in the same order and gives the same outcomes.
#[test]
fn composition_is_associative() {
    let (left, right) = (Log::default(), Log::default());
    let grouped_left = compose(
        compose(logger("a", &left), logger("b", &left)),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    )
    .expect("loggers fit");
    let grouped_right = compose(
        logger("a", &right),
        compose(
            logger("b", &right),
            TestDoubleExecutor::with_model(Bits::default(), 7),
        )
        .expect("loggers fit"),
    )
    .expect("loggers fit");
    let batch = &flip_and_toss(&bits_set())[1];
    assert_eq!(
        run_and_close(grouped_left, &[0, 1], batch),
        run_and_close(grouped_right, &[0, 1], batch)
    );
    assert_eq!(entries(&left), entries(&right));
}

/// A job composed with two adapters and an executor, grouped three
/// ways, adapter by adapter, the adapters composed first, or the executor
/// composed first, makes the same run: its calls meet the adapters in the
/// same order and give the same outcomes.
#[test]
fn the_grouping_of_a_jobs_chain_does_not_matter() {
    let logs = [Log::default(), Log::default(), Log::default()];
    let job = || ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&[0, 1]));
    let one_by_one = compose(
        compose(compose(job(), logger("a", &logs[0])), logger("b", &logs[0])),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    );
    let adapters_first = compose(
        compose(job(), compose(logger("a", &logs[1]), logger("b", &logs[1]))),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    );
    let executor_first = compose(
        job(),
        compose(
            logger("a", &logs[2]),
            compose(
                logger("b", &logs[2]),
                TestDoubleExecutor::with_model(Bits::default(), 7),
            )
            .expect("loggers fit"),
        )
        .expect("loggers fit"),
    );
    let outputs = [
        values(&one_by_one.expect("loggers fit").wait().0),
        values(&adapters_first.expect("loggers fit").wait().0),
        values(&executor_first.expect("loggers fit").wait().0),
    ];
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
    assert_eq!(entries(&logs[0]), entries(&logs[1]));
    assert_eq!(entries(&logs[0]), entries(&logs[2]));
    assert!(!entries(&logs[0]).is_empty(), "the run called the chain");
}

/// The values of a static program of [`flip_and_toss`] in the bits'
/// numbering, run on shots 0 and 1 by `chain`.
fn run_flip_and_toss<E>(
    chain: Chain<DefaultDriver, InstructionCall, E>,
) -> Vec<(ShotId, u32, Value)>
where
    E: Executor<InstructionBatch<InstructionCall>> + Close,
{
    let set = bits_set();
    let batches = flip_and_toss(&set);
    let needs = Needs::default().with_instruction_set(Some(Arc::new(set)));
    let (outcomes, _) = chain
        .run(StaticProgram::new(needs, batches), [0, 1])
        .expect("shots, needs met")
        .wait();
    values(&outcomes.expect("the bits neither refuse nor fail"))
}

/// `driver() >> a >> b >> executor` grouped in each of its five
/// ways makes a chain with the same run: a job's calls meet the adapters in
/// the same order and give the same outcomes. Among them,
/// `(driver() >> a) >> rest` and `driver() >> (adapters >> executor)?`;
/// `driver() >> a` is a chain's head again.
#[test]
fn the_grouping_of_a_drivers_chain_does_not_matter() {
    let logs: [Log; 5] = Default::default();
    let bits = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let ab = |log| (logger("a", log), logger("b", log));
    let outputs = [
        {
            let (a, b) = ab(&logs[0]);
            run_flip_and_toss(compose(driver() >> a >> b, bits()).expect("loggers fit"))
        },
        {
            let (a, b) = ab(&logs[1]);
            run_flip_and_toss(compose(driver() >> compose(a, b), bits()).expect("loggers fit"))
        },
        {
            let (a, b) = ab(&logs[2]);
            let executor = compose(compose(a, b), bits()).expect("loggers fit");
            run_flip_and_toss(compose(driver(), executor).expect("loggers fit"))
        },
        {
            let (a, b) = ab(&logs[3]);
            let executor =
                compose(a, compose(b, bits()).expect("loggers fit")).expect("loggers fit");
            run_flip_and_toss(compose(driver(), executor).expect("loggers fit"))
        },
        {
            let (a, b) = ab(&logs[4]);
            let executor = compose(b, bits()).expect("loggers fit");
            let head: ChainHead<DefaultDriver, _> = driver() >> a;
            run_flip_and_toss(compose(head, executor).expect("loggers fit"))
        },
    ];
    for (output, log) in outputs.iter().zip(&logs).skip(1) {
        assert_eq!(output, &outputs[0]);
        assert_eq!(entries(log), entries(&logs[0]));
    }
    assert!(!entries(&logs[0]).is_empty(), "the job called the chain");
}

/// A driver written by hand, as a runtime is: it sends a [`Program`]'s
/// batches in turn, each to every shot of the job, and gives their
/// outcomes.
struct InTurn;

impl Driver<Program> for InTurn {
    type Instruction = InstructionCall;
    type Output = Vec<ShotOutcome>;

    fn needs(&self, program: &Program) -> Needs {
        program.needs.clone()
    }

    async fn drive<E>(&self, program: Program, executor: &mut E, shots: &ShotSet) -> Self::Output
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        let mut outcomes = Vec::new();
        for batch in &program.batches {
            let reply = executor
                .call(InstructionBatchRef {
                    shots,
                    instructions: batch,
                })
                .expect("the program's batches are valid");
            let (events, end) = wait_async(reply).await;
            end.expect("the shots do not fail");
            outcomes.extend(events);
        }
        outcomes
    }
}

/// A user's runtime heads a chain as the crate's driver does,
/// `ChainHead::new(runtime) >> adapter >> executor`: its calls pass through
/// the adapter below it, which with the executor is the one executor it is
/// given, and the job gives what it gives under the crate's driver.
#[test]
fn a_driver_written_by_hand_calls_through_the_adapters_below_it() {
    let by_hand = Log::default();
    let bits = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let (outcomes, _) = (ChainHead::new(InTurn) >> logger("a", &by_hand) >> bits())
        .expect("the logger fits")
        .run(Program::flip_and_toss(bits_set()), [0, 1])
        .expect("shots, needs met")
        .wait();
    let crates = Log::default();
    let (expected, _) = (driver() >> logger("a", &crates) >> bits())
        .expect("the logger fits")
        .run(Program::flip_and_toss(bits_set()), [0, 1])
        .expect("shots, needs met")
        .wait();
    assert_eq!(outcomes, expected);
    assert_eq!(entries(&by_hand), entries(&crates));
    assert!(!entries(&by_hand).is_empty(), "the job called the chain");
}

/// A shot's profile: its shot, and the calls it was given so far.
struct Profile {
    shot: ShotId,
    calls: u32,
}

/// A profiler, as a user writes one in a function of its own: it returns
/// the type its last option gives, `.shots(OnePerShot)`, its closures
/// opaque and named by their role, since `>>` takes the adapter by its
/// type, which a bare `impl Adapter` hides; each `+ use<>`, since its
/// closures hold a clone of `log`, not a borrow, so that its chain may move
/// to another thread. It logs each call of each shot, and sends it down as
/// it is.
fn profiler(
    log: &Log,
) -> OnePerShotAdapter<
    bool,
    bool,
    Profile,
    impl MakeState<Profile> + use<>,
    impl AdapterLogic<bool, bool, Profile> + use<>,
> {
    let log = Arc::clone(log);
    let profile = move |profile: &mut Profile,
                        input: &CallInput<'_, bool>,
                        below: &mut Below<'_, bool>,
                        _: &mut Outcomes<'_>| {
        profile.calls += 1;
        let entry = format!("profiler: shot {}, call {}", profile.shot, profile.calls);
        log.lock().expect("no test panics").push(entry);
        below.send(input.instructions())
    };
    let start = |shot: ShotContext| {
        Ok(Profile {
            shot: shot.shot(),
            calls: 0,
        })
    };
    adapter_fn(start, profile).shots(OnePerShot)
}

/// A rewrite with one state, in a function of its own, as [`profiler`]:
/// it logs each call, and sends down each bool negated.
fn negation(
    log: &Log,
) -> AdapterFn<bool, bool, (), impl MakeState<()> + use<>, impl AdapterLogic<bool, bool, ()> + use<>>
{
    let log = Arc::clone(log);
    let negate = move |(): &mut (),
                       input: &CallInput<'_, bool>,
                       below: &mut Below<'_, bool>,
                       _: &mut Outcomes<'_>| {
        log.lock()
            .expect("no test panics")
            .push("negation".to_owned());
        let negated: Vec<bool> = input.instructions().iter().map(|bit| !bit).collect();
        below.send(&negated)
    };
    adapter_fn(|_| Ok(()), negate)
}

/// The logic of [`readouts`]: it reads each bool it is called with,
/// tagged in the order its shot reads them.
fn read_in_order(
    next: &mut u32,
    input: &CallInput<'_, bool>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    for &bit in input.instructions() {
        outcomes.push(Outcome::new(*next, bit.into()));
        *next += 1;
    }
    Ok(())
}

/// An executor of bools with a state per shot, in a function of its own,
/// as [`profiler`]: it reads each bool it is called with, tagged in the
/// order its shot reads them.
fn readouts() -> OnePerShotExecutor<bool, u32, impl MakeState<u32>, impl ExecutorLogic<bool, u32>> {
    executor_fn(per_shot_readouts(), |_| Ok(0), read_in_order).shots(OnePerShot)
}

/// What [`readouts`] promise: each shot's reads continue from one call to
/// the next, on its own state.
fn per_shot_readouts() -> Capabilities {
    Capabilities::default()
        .with_resumable_shots(true)
        .with_replayable(true)
}

/// [`readouts`], each shot on a thread of its own: `.run_on(..)` before
/// `.shots(..)`, so its logic is copied to each shot's placement, and its
/// function names it `+ Clone`.
fn readouts_on_a_thread_per_shot() -> OnePerShotPlacedExecutor<
    bool,
    u32,
    impl MakeState<u32>,
    impl ExecutorLogic<bool, u32> + Clone,
    OwnThread,
> {
    executor_fn(per_shot_readouts(), |_| Ok(0), read_in_order)
        .run_on(OwnThread)
        .shots(OnePerShot)
}

/// The job of the chains of facility components: two calls, of two bools
/// then one.
fn two_calls() -> StaticProgram<bool> {
    StaticProgram::new(Needs::default(), vec![vec![true, false], vec![true]])
}

/// The values of `chain`'s run of [`two_calls`] on shots 2 and 5.
fn run_two_calls<E>(chain: Chain<DefaultDriver, bool, E>) -> Vec<(ShotId, u32, Value)>
where
    E: Executor<InstructionBatch<bool>> + Close,
{
    let (outcomes, _) = chain.run(two_calls(), [2, 5]).expect("shots").wait();
    values(&outcomes.expect("no component refuses or fails"))
}

/// A job given to a driver's chain of the facility's components,
/// written with `>>` all along, as the target chain's user writes it, each
/// component made by a function: each call passes through the profiler,
/// then the negation, to the executor, and each shot's readouts come back
/// in order, negated.
#[test]
fn a_job_runs_through_a_chain_of_the_facilitys_components_written_with_shr() {
    let log = Log::default();
    let chain = (driver() >> profiler(&log) >> negation(&log) >> readouts()).expect("bools fit");
    assert_eq!(run_two_calls(chain), TWO_CALLS_NEGATED);
    assert_eq!(
        entries(&log),
        [
            "profiler: shot 2, call 1",
            "profiler: shot 5, call 1",
            "negation",
            "profiler: shot 2, call 2",
            "profiler: shot 5, call 2",
            "negation",
        ]
    );
}

/// `driver() >> profiler >> negation >> executor`, of the facility's
/// components, each made by a function, grouped in each of its five ways
/// with `>>` alone: the same chain, whose job's calls meet the adapters
/// in the same order and give the same outcomes.
#[test]
fn the_grouping_of_a_chain_of_the_facilitys_components_written_with_shr_does_not_matter() {
    let logs: [Log; 5] = Default::default();
    let outputs = [
        run_two_calls(
            (driver() >> profiler(&logs[0]) >> negation(&logs[0]) >> readouts())
                .expect("bools fit"),
        ),
        run_two_calls(
            (driver() >> (profiler(&logs[1]) >> negation(&logs[1])) >> readouts())
                .expect("bools fit"),
        ),
        run_two_calls(
            (driver()
                >> (profiler(&logs[2]) >> negation(&logs[2]) >> readouts()).expect("bools fit"))
            .expect("bools fit"),
        ),
        run_two_calls(
            (driver()
                >> (profiler(&logs[3]) >> (negation(&logs[3]) >> readouts()).expect("bools fit"))
                    .expect("bools fit"))
            .expect("bools fit"),
        ),
        run_two_calls(
            (driver()
                >> profiler(&logs[4])
                >> (negation(&logs[4]) >> readouts()).expect("bools fit"))
            .expect("bools fit"),
        ),
    ];
    for (output, log) in outputs.iter().zip(&logs).skip(1) {
        assert_eq!(output, &outputs[0]);
        assert_eq!(entries(log), entries(&logs[0]));
    }
    assert!(!entries(&logs[0]).is_empty(), "the job called the chain");
}

/// The outcomes of [`two_calls`] on shots 2 and 5 through the profiler
/// and the negation: each shot's bools read in order, negated.
const TWO_CALLS_NEGATED: [(ShotId, u32, Value); 6] = [
    (2, 0, Value::Zero),
    (2, 1, Value::One),
    (5, 0, Value::Zero),
    (5, 1, Value::One),
    (2, 2, Value::Zero),
    (5, 2, Value::Zero),
];

/// A chain built on the main thread, of components made by functions that
/// take a reference to the user's log, then given to a worker thread that
/// runs its job: it moves there whole, borrowing nothing of the log, and
/// the user reads the log after the run.
#[test]
fn a_chain_of_components_made_from_a_reference_runs_on_another_thread() {
    let log = Log::default();
    let chain = (driver() >> profiler(&log) >> negation(&log) >> readouts()).expect("bools fit");
    let outcomes = std::thread::spawn(move || run_two_calls(chain))
        .join()
        .expect("the run does not panic");
    assert_eq!(outcomes, TWO_CALLS_NEGATED);
    assert_eq!(entries(&log).len(), 6, "the job called the chain");
}

/// A chain whose executor, made by a function, runs each shot on a thread
/// of its own (`.run_on(..)` then `.shots(..)`), written with `>>`: each
/// shot's outcomes are those of the executor that runs every shot on the
/// caller's thread, in order; the shots' threads give theirs in any order.
#[test]
fn a_chain_ending_at_an_executor_with_a_thread_per_shot_made_by_a_function_runs() {
    let log = Log::default();
    let chain = (driver() >> profiler(&log) >> negation(&log) >> readouts_on_a_thread_per_shot())
        .expect("bools fit");
    let mut outcomes = run_two_calls(chain);
    outcomes.sort_by_key(|&(shot, tag, _)| (shot, tag));
    let mut expected = TWO_CALLS_NEGATED;
    expected.sort_by_key(|&(shot, tag, _)| (shot, tag));
    assert_eq!(outcomes, expected);
}

/// The executor of a profiler written by hand: it counts the calls it
/// passes down, and reports that count beside the report of the executor
/// downstream of it.
struct CountsCalls<E> {
    executor: E,
    calls: u64,
}

/// `executor`, its calls counted: with [`map_executor`], a profiler
/// written by hand.
fn counts_calls<E>(executor: E) -> CountsCalls<E> {
    CountsCalls { executor, calls: 0 }
}

impl<E: Describe> Describe for CountsCalls<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for CountsCalls<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<E::Reply, Rejected> {
        self.calls += 1;
        self.executor.call(batch)
    }
}

impl<E: Close> Close for CountsCalls<E> {
    type Report = (u64, E::Report);

    fn close(&mut self) -> Self::Report {
        (std::mem::take(&mut self.calls), self.executor.close())
    }
}

/// The values of `chain`'s run of [`two_calls`] on shots 2 and 5, and its
/// report.
fn run_two_calls_with_report<E>(
    chain: Chain<DefaultDriver, bool, E>,
) -> (Vec<(ShotId, u32, Value)>, E::Report)
where
    E: Executor<InstructionBatch<bool>> + Close,
{
    let (outcomes, report) = chain.run(two_calls(), [2, 5]).expect("shots").wait();
    (
        values(&outcomes.expect("no component refuses or fails")),
        report,
    )
}

/// `driver() >> profiler >> counter >> negation >> executor`, the
/// counter a profiler written by hand between two of the facility's
/// adapters, grouped in each of its fourteen ways: the same chain, whose
/// job's calls meet the adapters in the same order and give the same
/// outcomes and the same report, the counter's count of the calls it
/// passes down included.
#[test]
fn the_grouping_of_a_chain_mixing_adapters_written_by_hand_and_by_the_facility_does_not_matter() {
    fn fits<T>(built: Result<T, BuildFaults>) -> T {
        built.expect("bools fit")
    }
    let logs: [Log; 14] = Default::default();
    let p = |k: usize| profiler(&logs[k]);
    let c = || map_executor::<bool, _>(counts_calls);
    let n = |k: usize| negation(&logs[k]);
    let runs = [
        run_two_calls_with_report(fits(driver() >> p(0) >> c() >> n(0) >> readouts())),
        run_two_calls_with_report(fits(driver() >> p(1) >> c() >> fits(n(1) >> readouts()))),
        run_two_calls_with_report(fits(driver() >> p(2) >> (c() >> n(2)) >> readouts())),
        run_two_calls_with_report(fits(driver() >> p(3) >> fits(c() >> n(3) >> readouts()))),
        run_two_calls_with_report(fits(
            driver() >> p(4) >> fits(compose(c(), fits(n(4) >> readouts()))),
        )),
        run_two_calls_with_report(fits(driver() >> (p(5) >> c()) >> n(5) >> readouts())),
        run_two_calls_with_report(fits(driver() >> (p(6) >> c()) >> fits(n(6) >> readouts()))),
        run_two_calls_with_report(fits(driver() >> (p(7) >> c() >> n(7)) >> readouts())),
        run_two_calls_with_report(fits(driver() >> (p(8) >> (c() >> n(8))) >> readouts())),
        run_two_calls_with_report(fits(driver() >> fits(p(9) >> c() >> n(9) >> readouts()))),
        run_two_calls_with_report(fits(
            driver() >> fits(p(10) >> c() >> fits(n(10) >> readouts())),
        )),
        run_two_calls_with_report(fits(
            driver() >> fits(p(11) >> (c() >> n(11)) >> readouts()),
        )),
        run_two_calls_with_report(fits(
            driver() >> fits(p(12) >> fits(c() >> n(12) >> readouts())),
        )),
        run_two_calls_with_report(fits(
            driver() >> fits(p(13) >> fits(compose(c(), fits(n(13) >> readouts())))),
        )),
    ];
    for (k, (run, log)) in runs.iter().zip(&logs).enumerate().skip(1) {
        assert_eq!(run, &runs[0], "grouping {k}");
        assert_eq!(entries(log), entries(&logs[0]), "grouping {k}");
    }
    assert_eq!(runs[0].0, TWO_CALLS_NEGATED);
    assert_eq!(
        runs[0].1.0, 2,
        "the counter passes the job's two calls down"
    );
    assert!(!entries(&logs[0]).is_empty(), "the job called the chain");
}

/// An adapter written by hand composed with an executor, then put
/// downstream of a chain's head with `>>`, as the facility's executors
/// are: the chain runs as the executor alone does, and its report is the
/// counter's.
#[test]
fn shr_takes_an_executor_composed_with_an_adapter_written_by_hand() {
    let counted = compose(map_executor::<bool, _>(counts_calls), readouts()).expect("bools fit");
    let chain = (driver() >> counted).expect("bools fit");
    let (outcomes, (calls, _)) = run_two_calls_with_report(chain);
    let alone = run_two_calls((driver() >> readouts()).expect("bools fit"));
    assert_eq!(outcomes, alone);
    assert_eq!(calls, 2, "the job's two calls");
}

/// A profiler written by hand, as a type of its own: it counts the calls
/// it passes down ([`CountsCalls`]).
struct CallCounter;

impl Adapter for CallCounter {
    type Accepts = bool;
    type Sends = bool;
}

impl<E: Executor<InstructionBatch<bool>> + Close> Apply<E> for CallCounter {
    type Output = CountsCalls<E>;

    fn apply(self, downstream: E) -> CountsCalls<E> {
        counts_calls(downstream)
    }
}

/// [`readouts`], its calls counted by a [`CallCounter`], in a function of
/// its own, as [`profiler`]: it names the executor it returns.
fn counted_readouts() -> AdapterExecutor<
    CallCounter,
    OnePerShotExecutor<bool, u32, impl MakeState<u32>, impl ExecutorLogic<bool, u32>>,
> {
    compose(CallCounter, readouts()).expect("bools fit")
}

/// A function that makes an adapter written by hand over an executor
/// returns it by name ([`AdapterExecutor`]), as one that makes an
/// `adapter_fn` over an executor does: `>>` takes it after a
/// chain's head, and the chain runs as the executor alone does, its report
/// the counter's.
#[test]
fn a_function_returns_an_adapter_written_by_hand_over_an_executor_for_shr() {
    let chain = (driver() >> counted_readouts()).expect("bools fit");
    let (outcomes, (calls, _)) = run_two_calls_with_report(chain);
    let alone = run_two_calls((driver() >> readouts()).expect("bools fit"));
    assert_eq!(outcomes, alone);
    assert_eq!(calls, 2, "the job's two calls");
}

/// A level a job prepares, the instruction of the lowering tests' jobs.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Level {
    Low,
    High,
}

impl TypedInstruction for Level {}

/// A pulse, between the lowering tests' two lowerings: a line driven or
/// not.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pulse {
    driven: bool,
}

impl TypedInstruction for Pulse {}

/// A lowering of levels to pulses with a state per shot, in a function of
/// its own, as [`profiler`]: it accepts one instruction type and sends
/// another, and its function spells the type with its defaults (no check).
fn pulses()
-> OnePerShotAdapter<Level, Pulse, (), impl MakeState<()>, impl AdapterLogic<Level, Pulse, ()>> {
    let lower = |(): &mut (),
                 input: &CallInput<'_, Level>,
                 below: &mut Below<'_, Pulse>,
                 _: &mut Outcomes<'_>| {
        let pulses: Vec<Pulse> = input
            .instructions()
            .iter()
            .map(|&level| Pulse {
                driven: level == Level::High,
            })
            .collect();
        below.send(&pulses)
    };
    adapter_fn(|_| Ok(()), lower).shots(OnePerShot)
}

/// A lowering of pulses to the bools [`readouts`] reads, with one state,
/// in a function of its own, spelled with its defaults as [`pulses`].
fn bits() -> AdapterFn<Pulse, bool, (), impl MakeState<()>, impl AdapterLogic<Pulse, bool, ()>> {
    let lower = |(): &mut (),
                 input: &CallInput<'_, Pulse>,
                 below: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>| {
        let bits: Vec<bool> = input.instructions().iter().map(|p| p.driven).collect();
        below.send(&bits)
    };
    adapter_fn(|_| Ok(()), lower)
}

/// A job of levels given to a chain that lowers them to pulses, then to
/// bools, each lowering made by a function, one with a state per shot and
/// one with one state: each shot reads the levels it prepared, in order.
#[test]
fn a_job_runs_through_lowerings_between_instruction_types_made_by_functions() {
    let chain = (driver() >> pulses() >> bits() >> readouts()).expect("the lowerings fit");
    let program = StaticProgram::new(
        Needs::default(),
        vec![vec![Level::High, Level::Low], vec![Level::High]],
    );
    let (outcomes, _) = chain.run(program, [2, 5]).expect("shots").wait();
    assert_eq!(
        values(&outcomes.expect("no component refuses or fails")),
        [
            (2, 0, Value::One),
            (2, 1, Value::Zero),
            (5, 0, Value::One),
            (5, 1, Value::Zero),
            (2, 2, Value::One),
            (5, 2, Value::One),
        ]
    );
}

/// [`readouts`] that also gives each shot's log probability, each bool it
/// read having been a fair coin's: its function spells the type with no
/// check ([`NoCheck`]) and the reading opaque, as a simulator's function
/// does.
fn readouts_of_fair_coins() -> OnePerShotExecutor<
    bool,
    u32,
    impl MakeState<u32>,
    impl ExecutorLogic<bool, u32>,
    NoCheck<bool>,
    impl FnMut(&u32) -> f64,
> {
    executor_fn(per_shot_readouts(), |_| Ok(0), read_in_order)
        .log_probability(|reads: &u32| f64::from(*reads) * 0.5_f64.ln())
        .shots(OnePerShot)
}

/// A chain whose executor, made by a function, reads each shot's log
/// probability: the run's report gives each shot's, that of the three
/// bools it read.
#[test]
fn a_chain_ending_at_an_executor_that_reads_log_probabilities_made_by_a_function_reports_them() {
    let chain = (driver() >> readouts_of_fair_coins()).expect("bools fit");
    let (outcomes, report) = chain.run(two_calls(), [2, 5]).expect("shots").wait();
    outcomes.expect("no component refuses or fails");
    let mut expected = LogP::default();
    expected.add(2, 3.0 * 0.5_f64.ln());
    expected.add(5, 3.0 * 0.5_f64.ln());
    assert_eq!(report.log_p, expected);
}

/// [`readouts`], as a function returns an executor whose type it hides:
/// a bare `impl Executor + Describe + Close`.
fn hidden_readouts() -> impl Executor<InstructionBatch<bool>> + Describe + Close {
    readouts()
}

/// An adapter made by a function, composed by `compose` over an executor
/// whose type its function hides, then given a job: the job runs through
/// the adapter to the executor.
#[test]
fn an_adapter_made_by_a_function_composes_over_an_executor_of_hidden_type() {
    let log = Log::default();
    let below = compose(negation(&log), hidden_readouts()).expect("bools fit");
    let chain = (driver() >> below).expect("bools fit");
    assert_eq!(run_two_calls(chain), TWO_CALLS_NEGATED);
}

/// A count of calls with one state that borrows the caller's tally, in a
/// function of its own: its state lives no longer than the tally, as
/// code that keeps its data on the stack writes it. It sends each call
/// down as it is.
fn tallied(
    tally: &Mutex<u32>,
) -> AdapterFn<
    bool,
    bool,
    &Mutex<u32>,
    impl MakeState<&Mutex<u32>>,
    impl AdapterLogic<bool, bool, &Mutex<u32>>,
> {
    let count = |tally: &mut &Mutex<u32>,
                 input: &CallInput<'_, bool>,
                 below: &mut Below<'_, bool>,
                 _: &mut Outcomes<'_>| {
        *tally.lock().expect("no test panics") += 1;
        below.send(input.instructions())
    };
    adapter_fn(move |_| Ok(tally), count)
}

/// A chain whose adapter's state borrows a tally on the caller's stack,
/// run while the tally lives: the adapter counts the job's two calls.
#[test]
fn an_adapter_whose_state_borrows_the_callers_data_runs_in_a_chain() {
    let tally = Mutex::new(0);
    let chain = (driver() >> tallied(&tally) >> readouts()).expect("bools fit");
    run_two_calls(chain);
    assert_eq!(*tally.lock().expect("no test panics"), 2);
}

/// Three faulty adapters upstream of an executor: a code that sends `h`,
/// which the executor lacks, upstream of a noise model that draws with the
/// executor's id, upstream of a code that sends `t`, which it lacks too.
/// Grouped either way, the chain reports the same faults, in the same
/// order, upstream first.
#[test]
fn a_chain_reports_the_same_faults_whatever_the_grouping() {
    let sending = |mnemonic| move || Relabel::new(bits_set().with(mnemonic, [Block]), false);
    let (code_h, code_t) = (sending("h"), sending("t"));
    let noise =
        || map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
    let Err(grouped_left) = compose(
        compose(compose(code_h(), noise()), code_t()),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("the three adapters are faulty");
    };
    let Err(grouped_right) = compose(
        compose(code_h(), compose(noise(), code_t())),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("the three adapters are faulty");
    };
    assert_eq!(grouped_left.0.len(), 3, "{grouped_left}");
    assert_eq!(grouped_left, grouped_right);
}

/// Two faulty codes, one that sends `h` and one that sends `t`, both of
/// which the executor lacks, between the crate's driver and the
/// executor: the chain reports the same faults whether the codes are
/// composed with the driver one by one or composed together first.
#[test]
fn a_drivers_chain_reports_the_same_faults_whatever_the_grouping() {
    let sending = |mnemonic| move || Relabel::new(bits_set().with(mnemonic, [Block]), false);
    let (code_h, code_t) = (sending("h"), sending("t"));
    let bits = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let Err(one_by_one) = compose(driver() >> code_h() >> code_t(), bits()) else {
        panic!("the two codes are faulty");
    };
    let Err(together) = compose(driver() >> compose(code_h(), code_t()), bits()) else {
        panic!("the two codes are faulty");
    };
    assert_eq!(one_by_one.0.len(), 2, "{one_by_one}");
    assert_eq!(together, one_by_one);
}

/// An adapter that passes the calls it is called with down, and counts the
/// times it is built.
struct CountsBuilds(Arc<AtomicUsize>);

impl Adapter for CountsBuilds {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn build(self, _: &mut AdapterBoundary<'_>) -> Self {
        self.0.fetch_add(1, Ordering::SeqCst);
        self
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for CountsBuilds {
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// A noise model that draws with the executor's id, composed with the
/// executor first, then an adapter composed upstream of that part: the
/// part's fault comes back alone, and the adapter upstream is never built.
#[test]
fn a_part_composed_with_the_executor_first_reports_its_faults_before_upstream_is_built() {
    let builds = Arc::new(AtomicUsize::new(0));
    let noise =
        map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
    let build = || {
        let part = compose(noise, TestDoubleExecutor::with_model(Bits::default(), 7))?;
        compose(CountsBuilds(Arc::clone(&builds)), part)
    };
    let Err(faults) = build() else {
        panic!("the noise model is faulty");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(
        faults.0[0].error,
        BuildError::RngInUse(ComponentId::LAST_EXECUTOR)
    );
    assert_eq!(builds.load(Ordering::SeqCst), 0, "the adapter was built");
}

/// Two noise models given the one id: the refusal says which id, why that
/// is refused, and how to change it, as `.drawing(..)` gives an adapter
/// its id.
#[test]
fn a_reused_id_says_which_why_and_how_to_change_it() {
    let noise = ComponentId::named("noise");
    let message = BuildError::RngInUse(noise).to_string();
    assert!(message.contains(&noise.to_string()), "{message}");
    assert!(
        message.contains("two components would share them"),
        "{message}"
    );
    assert!(message.contains("another id"), "{message}");
    assert!(message.contains(".drawing("), "{message}");
}

/// A noise model drawing with its own id over an executor given no run's
/// seed: the refusal names the noise model's random numbers, and says how
/// to give the run's seed, `.seed(..)` on the last executor.
#[test]
fn a_refusal_for_no_run_seed_names_the_adapter_and_says_how_to_seed() {
    let noise = ComponentId::named("noise");
    let message = BuildError::NoRunSeed(noise).to_string();
    assert!(message.contains(&noise.to_string()), "{message}");
    assert!(message.contains(".seed("), "{message}");
}

/// A program of `bool`s that takes each reply with [`wait`], as a
/// synchronous program does.
struct Waits;

impl AdaptiveProgram for Waits {
    type Instruction = bool;
    type Output = usize;

    async fn drive<E>(self, executor: &mut E, shots: &ShotSet) -> usize
    where
        E: Executor<InstructionBatch<bool>>,
    {
        let mut outcomes = Vec::new();
        for _ in 0..3 {
            let reply = executor
                .call(InstructionBatchRef {
                    shots,
                    instructions: &[true],
                })
                .expect("the workers take any batch");
            wait(reply, &mut outcomes).expect("the workers do not fail");
        }
        outcomes.len()
    }
}

/// A model whose shots' states are [`Worker`]s, whose replies end on
/// threads of their own.
pub(in crate::tests) struct Workers;

impl Describe for Workers {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

impl Model for Workers {
    type Instruction = bool;
    type State = Worker;

    fn state(&self, _: &ShotContext) -> Result<Worker, String> {
        Ok(Worker(Duration::from_millis(5)))
    }
}

/// A program's needs are checked against what the chain offers when its job
/// is given to the chain, which composing the tests' `ComposedJob` with an
/// executor does: a program that needs more qubits than the chain has is
/// refused then, before anything runs, the program named.
#[test]
fn a_job_whose_needs_the_chain_lacks_is_refused_when_composed() {
    let mut program = Program::flip_and_toss(bits_set());
    program.needs = program.needs.with_qubits(3);
    let Err(faults) = ComposedJob::new(program, shots(&[0]))
        >> TestDoubleExecutor::with_model(Bits::default(), 7)
    else {
        panic!("three qubits on two bits must be refused");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert!(faults.0[0].component.ends_with("Program"), "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the qubits are unmet: {faults}");
    };
    assert_eq!((unmet.need, unmet.offered), (Need::Qubits, 2));
}

/// A program that needs more qubits than the chain has, and the log probability
/// it does not give: both needs are reported at once, in the order of
/// `Capabilities::admit`, when the program is given to the chain.
#[test]
fn every_need_of_a_program_that_the_chain_lacks_is_reported() {
    let mut program = Program::flip_and_toss(bits_set());
    program.needs = program.needs.with_qubits(3).with_log_probability(true);
    let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("nothing between to refuse");
    let Err(JobFaults(faults)) = chain.run(program, [0]) else {
        panic!("three qubits and the log probability on two bits must be refused");
    };
    let needs: Vec<Option<Need>> = faults
        .iter()
        .map(|fault| match fault {
            Rejected::Unmet(unmet) => Some(unmet.need),
            _ => None,
        })
        .collect();
    assert_eq!(needs, [Some(Need::Qubits), Some(Need::LogProbability)]);
}

/// A job of more shots than the executor holds at once is refused when
/// its job is given to the chain, which composing the tests' `ComposedJob`
/// does, the shots unmet, and no shot is made: the executor is never called.
#[test]
fn a_job_of_more_shots_than_the_executor_holds_is_refused_when_composed() {
    let doubles = TestDoubleModel {
        max_shots: Some(2),
        ..TestDoubleModel::default()
    };
    let made = Rc::clone(&doubles.made);
    let Err(faults) =
        ComposedJob::new(Waits, shots(&[0, 1, 2])) >> TestDoubleExecutor::with_model(doubles, 7)
    else {
        panic!("three shots on two at once must be refused");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the shots are unmet: {faults}");
    };
    assert_eq!(
        (unmet.need, unmet.needed, unmet.offered),
        (Need::Shots, 3, 2)
    );
    assert_eq!(made.get(), 0, "composing made a shot");
}

/// Two noise models that draw from the generator the bits draw from, one
/// upstream of the other: the faults of the chain read as one message,
/// each as its component and its error, joined by "; ".
#[test]
fn the_faults_of_a_chain_read_as_one_message_in_their_order() {
    let upstream =
        map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
    let downstream =
        map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
    let bits = TestDoubleExecutor::with_model(Bits::default(), 7);
    let Err(faults) = driver() >> upstream >> downstream >> bits else {
        panic!("both noise models are faulty");
    };
    let [upstream, downstream] = faults.0.as_slice() else {
        panic!("two faults: {faults}");
    };
    // The components are type names, whose text Rust does not promise.
    let expected = format!(
        "{}: {}; {}: {}",
        upstream.component, upstream.error, downstream.component, downstream.error
    );
    assert_eq!(faults.to_string(), expected);
}

/// A chain of the crate's driver, with a noise model that draws from
/// the generator the bits draw from: it has no job yet, and is refused
/// when it is built, with the adapter's fault, as a chain that starts at a
/// driver is.
#[test]
fn a_chain_of_a_driver_reports_its_adapters_faults_when_built() {
    let noise =
        map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
    let bits = TestDoubleExecutor::with_model(Bits::default(), 7);
    let Err(BuildFaults(faults)) = compose(driver() >> noise, bits) else {
        panic!("a noise model drawing from the bits' generator must be refused");
    };
    let errors: Vec<_> = faults.into_iter().map(|fault| fault.error).collect();
    assert_eq!(errors, [BuildError::RngInUse(ComponentId::LAST_EXECUTOR)]);
}

/// Two faults of an adapter's own, of the same text, are two faults: an
/// adapter fault equals only itself, or a clone of it.
#[test]
fn two_adapter_faults_of_the_same_text_are_not_equal() {
    let fault = || BuildError::Adapter(Arc::new(std::io::Error::other("no block of three")));
    let (first, second) = (fault(), fault());
    assert_ne!(first, second);
    assert_eq!(first.clone(), first);
}

/// Two build errors of one kind that state different faults are unequal:
/// another component drawing, another missing instruction, another need.
#[test]
fn two_build_errors_of_the_same_kind_stating_different_faults_are_not_equal() {
    let rng = BuildError::RngInUse;
    assert_ne!(
        rng(ComponentId::LAST_EXECUTOR),
        rng(ComponentId::named("noise"))
    );
    let missing = |mnemonic: &str| {
        BuildError::Instructions(vec![InstructionMismatch::Missing {
            mnemonic: mnemonic.to_owned(),
        }])
    };
    assert_ne!(missing("h"), missing("t"));
    let unmet = |need| {
        BuildError::Unmet(UnmetNeed {
            component: "simulator".to_owned(),
            need,
            needed: 1,
            offered: 0,
            mismatches: Vec::new(),
        })
    };
    assert_ne!(unmet(Need::ResumableShots), unmet(Need::LogProbability));
}

/// An adapter that states the set it accepts and not what it sends: it
/// passes the calls it is called with down, so it sends what it accepts.
struct Accepting {
    accepts: Arc<InstructionSet>,
}

impl Accepting {
    fn new(accepts: InstructionSet) -> Self {
        Self {
            accepts: Arc::new(accepts),
        }
    }
}

impl Adapter for Accepting {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.accepts))
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Accepting {
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// A program whose set declares the instructions of the adapter next to it
/// in another order (its `x` at the adapter's `coin`), composed into a
/// run: each of its batches is copied into the adapter's numbering, so its
/// flip flips the bit.
#[test]
fn a_program_in_another_order_runs_its_own_instructions_through_the_chain() {
    let job = ComposedJob::new(Program::flip_and_toss(reversed_set()), shots(&[0]));
    let (outcomes, _) =
        (job >> Accepting::new(bits_set()) >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the bits run what it accepts")
            .wait();
    assert_eq!(values(&outcomes)[0], (0, 0, Value::One), "x, then measure");
}

/// A program and bits that share one set, described once by its vendor and
/// declaring no action, as QEP's driver and executor share theirs: a set's
/// name is its identity, so the program's calls are matched by mnemonic, and
/// the run runs them.
#[test]
fn a_job_and_an_executor_sharing_a_set_without_actions_are_composed() {
    let vendors = undeclaring(bits_set(), EVERY_BIT);
    let job = ComposedJob::new(Program::flip_and_toss(vendors), shots(&[0]));
    let downstream = Bits {
        undeclared: EVERY_BIT,
        ..Bits::default()
    };
    let (outcomes, _) = (job >> TestDoubleExecutor::with_model(downstream, 7))
        .expect("one set at both ends, matched by mnemonic")
        .wait();
    assert_eq!(values(&outcomes)[0], (0, 0, Value::One), "x, then measure");
}

/// A chain of two adapters, the downstream one accepting a vendor's set
/// that leaves its read undeclared, upstream of bits whose vendor's set
/// leaves their flip undeclared: each boundary's fault is reported at
/// once when the chain is built, from upstream.
#[test]
fn every_undeclared_action_of_a_chain_is_reported_at_once() {
    let between = Accepting::new(undeclaring(bits_set(), &["measure"]));
    let downstream = Bits {
        undeclared: &["x"],
        ..Bits::default()
    };
    let Err(faults) = driver()
        >> Accepting::new(bits_set())
        >> between
        >> TestDoubleExecutor::with_model(downstream, 7)
    else {
        panic!("each boundary leaves an action undeclared");
    };
    let undeclared = |mnemonic: &str| InstructionMismatch::UndeclaredAction {
        mnemonic: mnemonic.to_owned(),
    };
    let mismatches: Vec<_> = faults
        .0
        .iter()
        .map(|fault| match &fault.error {
            BuildError::Instructions(mismatches) => mismatches.clone(),
            other => panic!("only undeclared actions: {other}"),
        })
        .collect();
    assert_eq!(
        mismatches,
        [
            vec![undeclared("measure")],
            vec![undeclared("x"), undeclared("measure")],
        ]
    );
}

/// An adapter that passes calls by opcode through as they are, stating no
/// set, under a program that tosses a coin, upstream of bits whose set, a
/// vendor's of another name, leaves the coin's action undeclared: the
/// adapter accepts the bits' set as it is, so the program's calls are
/// checked against it, and the run is refused as the program's need unmet,
/// the coin named.
#[test]
fn a_passing_adapter_of_calls_by_opcode_stating_no_set_accepts_the_set_downstream() {
    let passing = map_executor::<InstructionCall, _>(|below| below);
    let job = ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&[0]));
    let downstream = Bits {
        undeclared: &["coin"],
        ..Bits::default()
    };
    let Err(faults) = job >> passing >> TestDoubleExecutor::with_model(downstream, 7) else {
        panic!("the bits leave the coin's action undeclared");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the program's need is unmet: {faults}");
    };
    assert_eq!(
        unmet.mismatches,
        vec![InstructionMismatch::UndeclaredAction {
            mnemonic: "coin".to_owned(),
        }]
    );
}

/// An adapter that passes calls by opcode through as they are, stating no
/// set, between a program and bits that share one set declaring no action,
/// as QEP's profiler sits between its driver and executor: the adapter
/// offers the bits' set as it is, under its name, so the program's calls
/// are still matched by mnemonic, and the run runs them.
#[test]
fn a_passing_adapter_offers_the_set_downstream_under_its_name() {
    let passing = map_executor::<InstructionCall, _>(|below| below);
    let vendors = undeclaring(bits_set(), EVERY_BIT);
    let job = ComposedJob::new(Program::flip_and_toss(vendors), shots(&[0]));
    let downstream = Bits {
        undeclared: EVERY_BIT,
        ..Bits::default()
    };
    let (outcomes, _) = (job >> passing >> TestDoubleExecutor::with_model(downstream, 7))
        .expect("the program's set is the one downstream, by name")
        .wait();
    assert_eq!(values(&outcomes)[0], (0, 0, Value::One), "x, then measure");
}

/// A program of calls by opcode that states no set, on bits that describe
/// theirs: its opcodes cannot be checked, so the run is refused, as the
/// program's need of an instruction set unmet.
#[test]
fn a_job_of_calls_by_opcode_that_states_no_set_is_refused_when_composed() {
    let program = Program {
        batches: flip_and_toss(&bits_set()),
        needs: Needs::default(),
    };
    let job = ComposedJob::new(program, shots(&[0]));
    let Err(faults) = job >> TestDoubleExecutor::with_model(Bits::default(), 7) else {
        panic!("the program states no set");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the program's need is unmet: {faults}");
    };
    assert_eq!(unmet.need, Need::InstructionSet);
    assert!(unmet.mismatches.is_empty(), "{unmet}");
}

/// A typed program and a typed adapter, neither stating a set, upstream of
/// workers that describe none: the compiler checks that each boundary runs
/// the same type, so none needs a set, and the chain composes and runs.
#[test]
fn a_typed_chain_that_describes_no_set_is_composed() {
    let passing = map_executor::<bool, _>(|below| below);
    let outcomes = ends_soon(|| {
        let job = ComposedJob::new(Waits, shots(&[0, 1]));
        let (outcomes, _) = (job >> passing >> TestDoubleExecutor::with_model(Workers, 7))
            .expect("a typed boundary needs no set")
            .wait();
        outcomes
    });
    assert_eq!(outcomes, 6, "one outcome per shot and batch");
}

/// An adapter that decodes calls by opcode into typed instructions, `bool`s
/// for [`Workers`], stating the set it sends and the set it accepts, if any.
struct DecodeCalls {
    sends: Option<Arc<InstructionSet>>,
    accepts: Option<Arc<InstructionSet>>,
}

impl DecodeCalls {
    /// A decoder stating only the set it accepts, `accepts`.
    fn accepting(accepts: InstructionSet) -> Self {
        Self {
            sends: None,
            accepts: Some(Arc::new(accepts)),
        }
    }
}

impl Adapter for DecodeCalls {
    type Accepts = InstructionCall;
    type Sends = bool;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        self.sends.clone()
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        self.accepts.clone()
    }
}

impl<E: Executor<InstructionBatch<bool>> + Close> Apply<E> for DecodeCalls {
    type Output = DecodedCalls<E>;

    fn apply(self, downstream: E) -> DecodedCalls<E> {
        DecodedCalls(downstream)
    }
}

/// [`DecodeCalls`] upstream of an executor.
struct DecodedCalls<E>(E);

impl<E: Executor<InstructionBatch<bool>>> Executor<InstructionBatch<InstructionCall>>
    for DecodedCalls<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        let decoded: Vec<bool> = batch.instructions.iter().map(|_| true).collect();
        self.0.call(InstructionBatchRef {
            shots: batch.shots,
            instructions: &decoded,
        })
    }
}

impl<E: Close> Close for DecodedCalls<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.0.close()
    }
}

/// A decoder stating only the set it accepts, upstream of workers that
/// describe none: what it sends is typed, so its downstream boundary needs
/// no set, and the program's calls are checked against the set it accepts;
/// the chain composes and runs.
#[test]
fn a_decoder_of_calls_by_opcode_over_no_set_is_composed() {
    let outcomes = ends_soon(|| {
        let job = ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&[0]));
        let (outcomes, _) = (job
            >> DecodeCalls::accepting(bits_set())
            >> TestDoubleExecutor::with_model(Workers, 7))
        .expect("the decoder's downstream boundary is typed")
        .wait();
        outcomes
    });
    assert_eq!(outcomes.len(), 2, "one outcome per batch");
}

/// A decoder that accepts bits without their coin, under a program that
/// tosses one: the program's calls are checked against the set the decoder
/// accepts, so the coin is missing and the run is refused.
#[test]
fn a_job_whose_calls_a_decoder_does_not_accept_is_refused_when_composed() {
    let decoder = DecodeCalls::accepting(bits_without_coin());
    let job = ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&[0]));
    let Err(faults) = job >> decoder >> TestDoubleExecutor::with_model(Workers, 7) else {
        panic!("the decoder accepts no coin");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the program's need is unmet: {faults}");
    };
    assert_eq!(unmet.mismatches, vec![missing_coin()]);
}

/// A decoder that accepts bits without their coin, in a chain given the
/// job of a program that tosses one: the program's calls are checked
/// against the set the decoder accepts, so the coin is missing and the job
/// is refused, before anything runs.
#[test]
fn a_job_whose_calls_a_decoder_does_not_accept_is_refused() {
    let chain = (driver()
        >> DecodeCalls::accepting(bits_without_coin())
        >> TestDoubleExecutor::with_model(Workers, 7))
    .expect("the decoder's downstream boundary is typed");
    let Err(JobFaults(faults)) = chain.run(Program::flip_and_toss(bits_set()), [0]) else {
        panic!("the decoder accepts no coin");
    };
    let [Rejected::Unmet(unmet)] = &faults[..] else {
        panic!("the program's need is unmet: {faults:?}");
    };
    assert_eq!(unmet.mismatches, vec![missing_coin()]);
}

/// The bits' flip and read, without their coin.
fn bits_without_coin() -> InstructionSet {
    let set = InstructionSet::new("bits without coin")
        .with("x", [Block])
        .with("measure", [Block, Tag]);
    declaring(set, &["x", "measure"])
}

/// The mismatch of a coin that a set lacks.
fn missing_coin() -> InstructionMismatch {
    InstructionMismatch::Missing {
        mnemonic: "coin".to_owned(),
    }
}

/// A model whose shots' states are [`Worker`]s, as for [`Workers`],
/// describing a set of its own.
struct DescribedWorkers(InstructionSet);

impl Describe for DescribedWorkers {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_instruction_set(Some(Arc::new(self.0.clone())))
    }
}

impl Model for DescribedWorkers {
    type Instruction = bool;
    type State = Worker;

    fn state(&self, _: &ShotContext) -> Result<Worker, String> {
        Ok(Worker(Duration::from_millis(5)))
    }
}

/// A decoder stating no set, under a program that tosses a coin, upstream of
/// workers whose set has no coin: the decoder accepts the workers' set as
/// it is, so the program's calls are checked against it, and the run is
/// refused, the coin named.
#[test]
fn a_decoder_stating_no_set_accepts_the_set_downstream() {
    let decoder = DecodeCalls {
        sends: None,
        accepts: None,
    };
    let job = ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&[0]));
    let Err(faults) =
        job >> decoder >> TestDoubleExecutor::with_model(DescribedWorkers(bits_without_coin()), 7)
    else {
        panic!("the workers have no coin");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the program's need is unmet: {faults}");
    };
    assert_eq!(unmet.mismatches, vec![missing_coin()]);
}

/// A decoder stating no set, under a program of calls by opcode, upstream of
/// workers that describe none: no set downstream of the program numbers its
/// calls, so the run is refused, as the program's need of an instruction set
/// unmet.
#[test]
fn a_job_over_a_decoder_and_workers_stating_no_set_is_refused_when_composed() {
    let decoder = DecodeCalls {
        sends: None,
        accepts: None,
    };
    let job = ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&[0]));
    let Err(faults) = job >> decoder >> TestDoubleExecutor::with_model(Workers, 7) else {
        panic!("nothing downstream of the program describes a set");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the program's need is unmet: {faults}");
    };
    assert_eq!((unmet.need, unmet.offered), (Need::InstructionSet, 0));
}

/// A job with no adapter composes with any executor, not only a
/// per-shot one: with a toy ensemble, its run gives what the program gives
/// driving the ensemble itself, every shot reading the one state's values,
/// and the ensemble's report.
#[test]
fn a_job_composes_with_any_executor() {
    let (seed, ids) = (13, [2, 6]);
    let alone = {
        let program = Program::flip_and_toss(bits_set());
        block_on(program.drive(&mut SharedState::new(seed), &shots(&ids)))
    };
    let job = ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&ids));
    let (outcomes, report) = compose(job, SharedState::new(seed))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(values(&outcomes), values(&alone));
    assert_eq!(report, Count(1), "one state for both shots");
}

/// An adapter that passes every call through changes nothing, over an
/// ensemble too: the run gives what the program gives alone, each batch run
/// once on the one state, and the ensemble sees the program's two calls,
/// as alone.
#[test]
fn a_passing_adapter_over_an_ensemble_changes_nothing() {
    let (seed, ids) = (13, [2, 6]);
    let job = || ComposedJob::new(Program::flip_and_toss(bits_set()), shots(&ids));
    let ensemble = || counts_calls(SharedState::new(seed));
    let (alone, alone_report) = compose(job(), ensemble())
        .expect("no adapter, no fault")
        .wait();
    let passing = map_executor::<InstructionCall, _>(|below| below);
    let (outcomes, report) = compose(job() >> passing, ensemble())
        .expect("the adapter states no need")
        .wait();
    assert_eq!(values(&outcomes), values(&alone));
    assert_eq!(alone_report, (2, Count(1)), "two calls on one state");
    assert_eq!(report, alone_report);
}
