// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::{IntoFuture, poll_fn};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::thread;
use std::time::Duration;

use super::{Log, Program, Waits, Workers, entries, logger};
use crate::tests::map_executor;
use crate::tests::test_double::{self, Model, TestDoubleExecutor, TestDoubleModel};
use crate::tests::{
    Bits, BitsState, Count, YieldNow, bits_set, block_on, ends_soon, flip_and_toss, run_tasks,
    shots, values,
};
use crate::{
    AdapterLogic, Below, Capabilities, Close, Describe, Drive, Driver, Executor, ExecutorLogic,
    ExecutorReport, InstructionBatch, InstructionBatchRef, InstructionCall, MakeState, Needs,
    NoAdapter, NoCheck, OnePerShot, OnePerShotAdapter, OnePerShotExecutor, Opcode, Outcome,
    Outcomes, ProgramInput, Rejected, Reply, ReplyStatus, ShotContext, ShotId, ShotOutcome,
    ShotSet, Value, adapter_fn, compose, executor_fn, reply_channel, wait, wait_async,
};

/// A driver composed with adapters and an executor makes no shot until its
/// run is waited for.
#[test]
fn a_run_runs_nothing_until_it_is_waited_for() {
    let (log, seen) = (Log::default(), Arc::default());
    let recording = {
        let seen = Arc::clone(&seen);
        map_executor::<InstructionCall, _>(move |executor| Recorded { executor, seen })
    };
    let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&[0]));
    let run = (driver
        >> logger("debugger", &log)
        >> recording
        >> TestDoubleExecutor::with_model(Bits::default(), 7))
    .expect("the adapters fit");
    let seen_now = || {
        seen.lock()
            .expect("no test panicked holding the record")
            .clone()
    };
    assert_eq!(
        seen_now(),
        [],
        "composing called the executor, making a shot"
    );
    assert!(entries(&log).is_empty(), "composing ran a batch");
    run.wait();
    assert_ne!(seen_now(), [], "waiting for the run called the executor");
    assert!(
        !entries(&log).is_empty(),
        "waiting for the run ran its batches"
    );
}

/// What a [`Recorded`] executor saw: a call or a close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    Call,
    Close,
}

/// An executor that records, in order, each call and each close it sees,
/// around `E`.
struct Recorded<E> {
    executor: E,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl<E> Recorded<E> {
    /// `executor`, and its record of what it saw.
    fn new(executor: E) -> (Self, Arc<Mutex<Vec<Seen>>>) {
        let seen = Arc::default();
        let recorded = Self {
            executor,
            seen: Arc::clone(&seen),
        };
        (recorded, seen)
    }

    fn see(&self, event: Seen) {
        self.seen
            .lock()
            .expect("no test panicked holding the record")
            .push(event);
    }
}

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for Recorded<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<E::Reply, Rejected> {
        self.see(Seen::Call);
        self.executor.call(batch)
    }
}

impl<E: Describe> Describe for Recorded<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for Recorded<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.see(Seen::Close);
        self.executor.close()
    }
}

/// What `seen` holds now.
fn seen(seen: &Mutex<Vec<Seen>>) -> Vec<Seen> {
    seen.lock()
        .expect("no test panicked holding the record")
        .clone()
}

/// A run awaited, as an executor of futures takes it, runs nothing until
/// it is polled: no call, no close.
#[test]
fn a_run_turned_into_a_future_runs_nothing_until_polled() {
    let (executor, record) = Recorded::new(TestDoubleExecutor::with_model(Bits::default(), 7));
    let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&[0]));
    let run = compose(driver, executor).expect("no adapter, no fault");
    let future = run.into_future();
    assert_eq!(seen(&record), [], "a call or a close before the poll");
    block_on(future);
    let calls = seen(&record)
        .into_iter()
        .filter(|&event| event == Seen::Call);
    assert_eq!(calls.count(), 2, "the program's two batches");
}

/// A run closes its executor once, after the driver's last call: an
/// executor holding a device releases it when the program is done with it,
/// never while it still calls.
#[test]
fn a_run_closes_its_executor_once_after_the_drivers_last_call() {
    let (executor, record) = Recorded::new(TestDoubleExecutor::with_model(Bits::default(), 7));
    let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&[0, 1]));
    compose(driver, executor)
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(seen(&record), [Seen::Call, Seen::Call, Seen::Close]);
}

/// A run whose program never calls its executor still closes it, once: an
/// executor holding a device releases it whatever the program did.
#[test]
fn a_run_whose_program_never_calls_still_closes_its_executor() {
    let (executor, record) = Recorded::new(TestDoubleExecutor::with_model(Bits::default(), 7));
    let idle = Program {
        batches: Vec::new(),
        ..Program::flip_and_toss(bits_set())
    };
    compose(Driver::new(idle, shots(&[0, 1])), executor)
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(seen(&record), [Seen::Close]);
}

/// A program of `bool`s that reads twice on its shots, each read to its
/// end, and gives how each call went: refused, or how its reply ended.
struct ReadsTwice;

impl Drive for ReadsTwice {
    type Instruction = bool;
    type Output = [Result<Result<(), String>, Rejected>; 2];

    async fn drive<E: Executor<InstructionBatch<bool>>>(
        self,
        executor: &mut E,
        own: &ShotSet,
    ) -> Self::Output {
        [(); 2].map(|()| test_double::run(executor, own, &[true]).map(|(_, end)| end))
    }
}

/// A run whose first call fails, at shot 1, so that the executor refuses
/// the second, still closes its executor once, after the driver's last
/// call: an executor holding a device releases it whatever its shots did.
#[test]
fn a_run_closes_its_executor_once_after_a_call_failed() {
    let model = TestDoubleModel {
        failing: Some(1),
        ..TestDoubleModel::default()
    };
    let (executor, record) = Recorded::new(TestDoubleExecutor::with_model(model, 7));
    let ([first, second], _) = compose(Driver::new(ReadsTwice, shots(&[0, 1])), executor)
        .expect("no adapter, no fault")
        .wait();
    first
        .expect("the executor accepts the first read")
        .expect_err("shot 1's read fails");
    second.expect_err("the failed executor refuses the second read");
    assert_eq!(seen(&record), [Seen::Call, Seen::Call, Seen::Close]);
}

/// A run through a debugger closes the last executor of its chain once,
/// after the driver's last call: the debugger's executor of the shot does
/// not close it again, so a device behind the debugger is released once.
#[test]
fn a_run_through_an_adapter_closes_the_last_executor_once_after_the_drivers_last_call() {
    let (executor, record) = Recorded::new(TestDoubleExecutor::with_model(Bits::default(), 7));
    let log = Log::default();
    let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&[0]));
    compose(driver >> logger("debugger", &log), executor)
        .expect("a debugger fits any executor")
        .wait();
    assert_eq!(seen(&record), [Seen::Call, Seen::Call, Seen::Close]);
}

/// A run waited for gives what its program gives, and the report its
/// executor gave when the run closed it, each shot's report folded.
#[test]
fn a_run_gives_the_programs_output_and_the_executors_report() {
    let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&[0, 1, 2]));
    let (outcomes, report) = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(outcomes.len(), 6, "two outcomes per shot");
    assert_eq!(
        report,
        ExecutorReport {
            calls: 2,
            states: Count(3),
            ..ExecutorReport::default()
        }
    );
}

/// A chain composed with a probe that draws nothing and sends what it
/// accepts (a logger) gives the outcomes that its program gives driving the
/// executor alone, built with the same seed, on the same shots, coins
/// included.
#[test]
fn a_run_draws_what_its_executor_alone_draws() {
    let (seed, ids) = (11, [0, 4, 9]);
    let alone = {
        let program = Program::flip_and_toss(bits_set());
        let mut executor = TestDoubleExecutor::with_model(Bits::default(), seed);
        block_on(program.drive(&mut executor, &shots(&ids)))
    };
    let log = Log::default();
    let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&ids));
    let (outcomes, _) = (driver
        >> logger("debugger", &log)
        >> TestDoubleExecutor::with_model(Bits::default(), seed))
    .expect("a logger fits")
    .wait();
    assert_eq!(values(&outcomes), values(&alone));
}

/// The program of a simulator of [`Bits`] made with `executor_fn`: runs
/// the batch on the shot's bits.
fn run_bits(
    bits: &mut BitsState,
    input: &ProgramInput<'_, InstructionCall>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    let readouts = bits
        .run(input.instructions())
        .map_err(|refused| refused.to_string())?;
    for (tag, value) in readouts {
        outcomes.push(Outcome::new(tag, value));
    }
    Ok(())
}

/// A run of one shot gives it the outcomes, coins included, that a run of
/// several gives it, on a simulator seeded with 11 that offers replayable
/// shots: a replay of each of shots 9, 1, 4 and 6 alone gives what it gave
/// among the others.
#[test]
fn a_shot_run_alone_gives_what_it_gives_among_others() {
    let ids = [9, 1, 4, 6];
    let run = |ids: &[ShotId]| {
        let bits = Bits::default();
        let simulator = executor_fn(
            bits.capabilities(),
            move |context: ShotContext| Ok(bits.state(&context).expect("bits are always made")),
            run_bits,
        )
        .seed(11)
        .shots(OnePerShot);
        let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(ids));
        let (outcomes, _) = (driver >> simulator).expect("no adapter, no fault").wait();
        outcomes
    };
    let among_others = run(&ids);
    let coins: Vec<Value> = (among_others.iter())
        .filter(|o| o.tag == 1)
        .map(|o| o.value)
        .collect();
    assert!(
        coins.contains(&Value::Zero) && coins.contains(&Value::One),
        "the shots toss coins of both sides, so a shot given another's \
         draws would show: {coins:?}"
    );
    for j in ids {
        let its_own: Vec<ShotOutcome> = (among_others.iter())
            .filter(|o| o.shot == j)
            .copied()
            .collect();
        assert_eq!(values(&run(&[j])), values(&its_own), "shot {j}");
    }
}

/// The logic of an observer made with `adapter_fn`: sends each call down
/// as it is.
fn send_as_is(
    (): &mut (),
    input: &ProgramInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    below.send(input.instructions())
}

/// The run's driver, on shots that are not a range.
fn driver() -> Driver<Program, NoAdapter<InstructionCall>> {
    Driver::new(Program::flip_and_toss(bits_set()), shots(&[2, 5, 6, 11]))
}

/// The run's simulator, made with `executor_fn`, a state per shot.
fn simulator() -> OnePerShotExecutor<
    InstructionCall,
    BitsState,
    impl MakeState<BitsState>,
    impl ExecutorLogic<InstructionCall, BitsState>,
> {
    let bits = Bits::default();
    executor_fn(
        bits.capabilities(),
        move |context: ShotContext| Ok(bits.state(&context).expect("bits are always made")),
        run_bits,
    )
    .seed(11)
    .shots(OnePerShot)
}

/// The values the run of `driver() >> simulator()` reads, no adapter
/// between them.
fn read_alone() -> Vec<(u32, u32, Value)> {
    let (alone, _) = (driver() >> simulator()).expect("no adapter").wait();
    assert!(!alone.is_empty(), "the program reads bits");
    values(&alone)
}

/// `driver >> observer >> simulator`, an observer made with `adapter_fn`
/// with one state: grouped either way with `>>`, the chain gives the
/// outcomes the simulator gives alone.
#[test]
fn a_driver_and_an_adapter_fn_compose_in_either_grouping() {
    let observer = || adapter_fn(|_| Ok(()), send_as_is);
    let (left, _) = ((driver() >> observer()) >> simulator())
        .expect("an observer fits")
        .wait();
    let downstream = (observer() >> simulator()).expect("an observer fits");
    let (right, _) = (driver() >> downstream)
        .expect("the driver needs what it offers")
        .wait();
    assert_eq!(values(&left), read_alone());
    assert_eq!(values(&right), read_alone());
}

/// `driver >> profiler >> simulator`, a profiler made with `adapter_fn`
/// with a state per shot: grouped either way with `>>`, the chain gives
/// the outcomes the simulator gives alone.
#[test]
fn a_driver_and_a_per_shot_adapter_fn_compose_in_either_grouping() {
    let profiler = || adapter_fn(|_| Ok(()), send_as_is).shots(OnePerShot);
    let (left, _) = ((driver() >> profiler()) >> simulator())
        .expect("a profiler fits")
        .wait();
    let downstream = (profiler() >> simulator()).expect("a profiler fits");
    let (right, _) = (driver() >> downstream)
        .expect("the driver needs what it offers")
        .wait();
    assert_eq!(values(&left), read_alone());
    assert_eq!(values(&right), read_alone());
}

/// `driver >> observer >> profiler >> simulator`, the profiler with a
/// state per shot, the observer with one: the adapters grouped either way
/// with `>>`, the chain gives the outcomes the simulator gives alone.
#[test]
fn two_adapter_fns_compose_in_either_grouping() {
    let observer = || adapter_fn(|_| Ok(()), send_as_is);
    let profiler = || adapter_fn(|_| Ok(()), send_as_is).shots(OnePerShot);
    let first = ((observer() >> profiler()) >> simulator()).expect("the adapters fit");
    let (left, _) = (driver() >> first).expect("the driver fits").wait();
    let downstream = (profiler() >> simulator()).expect("a profiler fits");
    let second = (observer() >> downstream).expect("an observer fits");
    let (right, _) = (driver() >> second).expect("the driver fits").wait();
    assert_eq!(values(&left), read_alone());
    assert_eq!(values(&right), read_alone());
}

/// A profiler's counts: the calls of each instruction it saw.
type Counts = BTreeMap<Opcode, u64>;

/// The logic of the target chain's profiler: counts what its shot runs,
/// then sends the call down as it is.
fn profile(
    counts: &mut Counts,
    input: &ProgramInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), String> {
    for call in input.instructions() {
        *counts.entry(call.opcode).or_default() += 1;
    }
    below.send(input.instructions())
}

/// A profiler of instruction calls, a state per shot, given its state's
/// closure `M`, its logic `L` and its state report `K`.
type Profiler<M, L, K> =
    OnePerShotAdapter<InstructionCall, InstructionCall, Counts, M, L, NoCheck<InstructionCall>, K>;

/// The target chain's profiler: counts per shot, and reports its counts.
fn profiler() -> Profiler<
    impl MakeState<Counts>,
    impl AdapterLogic<InstructionCall, InstructionCall, Counts>,
    impl FnMut(&Counts) -> Counts,
> {
    profiler_reporting(Counts::clone)
}

/// The profiler of the target chain, reporting what `report` keeps of its
/// counts.
fn profiler_reporting<K: FnMut(&Counts) -> Counts>(
    report: K,
) -> Profiler<impl MakeState<Counts>, impl AdapterLogic<InstructionCall, InstructionCall, Counts>, K>
{
    adapter_fn(|_| Ok(Counts::default()), profile)
        .state_report(report)
        .shots(OnePerShot)
}

/// What a profiler of measurements keeps of `counts`: the measurements'.
fn measurements(counts: &Counts) -> Counts {
    let measure = bits_set().opcode("measure").expect("the bits' instruction");
    counts
        .iter()
        .filter(|&(opcode, _)| *opcode == measure)
        .map(|(opcode, count)| (*opcode, *count))
        .collect()
}

/// What the profiler counts over the run of `driver()`: each of its four
/// shots flips once, tosses once and measures twice.
fn the_runs_counts() -> Counts {
    let set = bits_set();
    let opcode = |mnemonic| set.opcode(mnemonic).expect("the bits' instruction");
    Counts::from([
        (opcode("x"), 4),
        (opcode("coin"), 4),
        (opcode("measure"), 8),
    ])
}

/// The report of the run of `driver() >> simulator()`, no adapter between
/// them.
fn report_alone() -> ExecutorReport<()> {
    let (_, report) = (driver() >> simulator()).expect("no adapter").wait();
    report
}

/// The target chain with its profiler: `run.wait()` gives the profiler's
/// counts, folded over the run's four shots, beside the simulator's
/// report.
#[test]
fn a_runs_wait_gives_the_adapters_report_beside_the_one_below() {
    let run = (driver() >> profiler() >> simulator()).expect("a profiler fits");
    let (_, (counts, report)): (_, (Counts, ExecutorReport<()>)) = run.wait();
    assert_eq!(counts, the_runs_counts());
    assert_eq!(report, report_alone());
}

/// The target chain with a debugger above its profiler and a transpiler
/// below, neither given a report: the run's report is the profiler's
/// beside the simulator's, with no pair for either, as its type states.
#[test]
fn an_adapter_given_no_state_report_adds_no_pair_to_the_report() {
    let debugger = adapter_fn(|_| Ok(()), send_as_is).shots(OnePerShot);
    let transpiler = adapter_fn(|_| Ok(()), send_as_is);
    let run = (driver() >> debugger >> profiler() >> transpiler >> simulator())
        .expect("the adapters fit");
    let (_, (counts, report)): (_, (Counts, ExecutorReport<()>)) = run.wait();
    assert_eq!(counts, the_runs_counts());
    assert_eq!(report, report_alone());
}

/// A profiler above a profiler of measurements: under every grouping of
/// `>>`, the run's report is the same, the upper profiler's counts beside
/// the pair of the lower's measurements and the simulator's report.
#[test]
fn a_runs_report_is_the_same_under_every_grouping_of_shr() {
    type Report = (Counts, (Counts, ExecutorReport<()>));
    let lower = || profiler_reporting(measurements);
    let groupings: [(&str, Report); 4] = [
        ("d >> a >> b >> e", {
            let run = driver() >> profiler() >> lower() >> simulator();
            run.expect("the profilers fit").wait().1
        }),
        ("d >> (a >> b) >> e", {
            let run = driver() >> (profiler() >> lower()) >> simulator();
            run.expect("the profilers fit").wait().1
        }),
        ("d >> a >> (b >> e)", {
            let below = (lower() >> simulator()).expect("a profiler fits");
            let run = driver() >> profiler() >> below;
            run.expect("the profilers fit").wait().1
        }),
        ("d >> (a >> (b >> e))", {
            let below = (lower() >> simulator()).expect("a profiler fits");
            let both = (profiler() >> below).expect("a profiler fits");
            (driver() >> both).expect("the driver fits").wait().1
        }),
    ];
    let the_runs_measurements = measurements(&the_runs_counts());
    let expected = (the_runs_counts(), (the_runs_measurements, report_alone()));
    for (grouping, report) in groupings {
        assert_eq!(report, expected, "{grouping}");
    }
}

/// An executor, around `E`, that records the shots named by each call it
/// sees.
struct SeesShots<E> {
    executor: E,
    seen: Arc<Mutex<Vec<ShotId>>>,
}

impl<E> SeesShots<E> {
    /// `executor`, and the shots its calls named, call after call.
    fn new(executor: E) -> (Self, Arc<Mutex<Vec<ShotId>>>) {
        let seen = Arc::default();
        let recording = Self {
            executor,
            seen: Arc::clone(&seen),
        };
        (recording, seen)
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for SeesShots<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        (self.seen.lock())
            .expect("no test thread panicked")
            .extend(batch.shots);
        self.executor.call(batch)
    }
}

impl<E: Describe> Describe for SeesShots<E> {
    fn capabilities(&self) -> Capabilities {
        self.executor.capabilities()
    }
}

impl<E: Close> Close for SeesShots<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// A program of the bits' set that flips and reads on its shots, then
/// tosses and reads on `then`, some of them.
struct OnItsShotsThenSome {
    then: ShotSet,
}

impl Drive for OnItsShotsThenSome {
    type Instruction = InstructionCall;
    type Output = ();

    fn needs(&self) -> Needs {
        Needs::default().with_instructions(Some(Arc::new(bits_set())))
    }

    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        self,
        executor: &mut E,
        own: &ShotSet,
    ) {
        let [flip, toss] =
            <[_; 2]>::try_from(flip_and_toss(&bits_set())).expect("flip_and_toss has two batches");
        for (shots, batch) in [(own, &flip), (&self.then, &toss)] {
            let reply = executor
                .call(InstructionBatchRef {
                    shots,
                    instructions: batch,
                })
                .expect("the bits run the set's batches on the driver's shots");
            wait(reply, &mut Vec::new()).expect("the bits do not fail");
        }
    }
}

/// A driver of shots 9, 1 and 4 that calls all of them, then shots 4 and 9:
/// the last executor sees each of these shots as often as the driver named
/// it, none more and no other, with no adapter, one, or two.
#[test]
fn the_shots_a_driver_names_reach_the_last_executor_through_every_adapter() {
    let driver = || {
        let then = shots(&[4, 9]);
        Driver::new(OnItsShotsThenSome { then }, shots(&[9, 1, 4]))
    };
    let executor = || SeesShots::new(TestDoubleExecutor::with_model(Bits::default(), 7));
    let passing = || map_executor::<InstructionCall, _>(|below| below);
    let log = Log::default();
    let sorted = |seen: Arc<Mutex<Vec<ShotId>>>| {
        let mut seen = seen.lock().expect("no test thread panicked").clone();
        seen.sort_unstable();
        seen
    };
    let (none, seen_by_none) = executor();
    compose(driver(), none)
        .expect("no adapter, no fault")
        .wait();
    let (one, seen_through_one) = executor();
    compose(driver() >> logger("debugger", &log), one)
        .expect("a logger fits")
        .wait();
    let (two, seen_through_two) = executor();
    compose(driver() >> passing() >> logger("debugger", &log), two)
        .expect("a passing adapter and a logger fit")
        .wait();
    let named = [1, 4, 4, 9, 9];
    assert_eq!(sorted(seen_by_none), named, "with no adapter");
    assert_eq!(sorted(seen_through_one), named, "through one adapter");
    assert_eq!(sorted(seen_through_two), named, "through two adapters");
}

/// A run awaited on an executor of futures gives what it gives waited for.
#[test]
fn a_run_awaited_gives_what_it_gives_waited_for() {
    let run = || {
        let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&[0, 1]));
        (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("no adapter, no fault")
    };
    let awaited = block_on(async { run().await });
    let waited = run().wait();
    assert_eq!(awaited, waited);
}

/// A run whose program and executor are `Send`, awaited: its future is
/// not `Send`, so a caller that needs one waits for the run instead.
#[test]
fn an_awaited_runs_future_is_not_send() {
    fn assert_send<T: Send>(_: &T) {}
    // `checked` names one function when the type is not `Send`, and two,
    // which does not compile, when it is (a `compile_fail` doc example
    // would run only under `cargo test --doc`, not in CI's `cargo test`).
    trait NotSend<Which> {
        fn checked(&self) {}
    }
    impl<T> NotSend<()> for T {}
    impl<T: Send> NotSend<u8> for T {}
    let driver = Driver::new(Program::flip_and_toss(bits_set()), shots(&[0, 1]));
    let run = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault");
    assert_send(&run);
    NotSend::checked(&run.into_future());
}

/// A program of `bool`s that reads once on its shots and awaits the reply.
struct AwaitsARead;

impl Drive for AwaitsARead {
    type Instruction = bool;
    type Output = Vec<ShotOutcome>;

    async fn drive<E: Executor<InstructionBatch<bool>>>(
        self,
        executor: &mut E,
        own: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let read = InstructionBatchRef {
            shots: own,
            instructions: &[true],
        };
        let reply = executor.call(read).expect("the executor runs its shots");
        let (outcomes, end) = wait_async(reply).await;
        end.expect("the read ends well");
        outcomes
    }
}

/// A run awaited on a runtime of one thread leaves the thread to the other
/// tasks while its program awaits a reply: the run's read ends only when
/// another task, on the same thread, ends it. Run on a thread of its own,
/// so that a run that blocked its thread fails the test instead of hanging
/// it.
#[test]
fn a_run_awaited_leaves_its_thread_to_other_tasks_while_its_program_awaits() {
    let log = ends_soon(|| {
        let model = TestDoubleModel {
            deferred: vec![0],
            ..TestDoubleModel::default()
        };
        let deferred = Rc::clone(&model.seen);
        let run = (Driver::new(AwaitsARead, shots(&[0]))
            >> TestDoubleExecutor::with_model(model, 7))
        .expect("no adapter, no fault");
        let log = RefCell::new(Vec::new());
        let awaits = async {
            let (outcomes, _) = run.await;
            log.borrow_mut()
                .push(format!("run gave {}", outcomes.len()));
        };
        let ends_the_read = async {
            loop {
                let read = deferred.borrow_mut().replies.get_mut(&0).and_then(Vec::pop);
                if let Some(read) = read {
                    log.borrow_mut().push("read ended".to_string());
                    read.done();
                    return;
                }
                YieldNow(false).await;
            }
        };
        run_tasks(vec![Box::pin(awaits), Box::pin(ends_the_read)], &log);
        let mut log = log.into_inner();
        log.retain(|entry| entry != "park");
        log
    });
    assert_eq!(log, ["read ended", "run gave 1"]);
}

/// A synchronous program, which parks its thread in `wait` for replies that
/// end on other threads, runs to its end when its run is waited for.
#[test]
fn a_run_waited_for_runs_a_program_that_waits_for_replies_from_other_threads() {
    let outcomes = ends_soon(|| {
        let driver = Driver::new(Waits, shots(&[0, 1]));
        let (output, _) = (driver >> TestDoubleExecutor::with_model(Workers, 7))
            .expect("no adapter, no fault")
            .wait();
        output
    });
    assert_eq!(outcomes, 6, "one outcome per shot and batch");
}

/// A program that awaits one reply and, before it is polled again, takes
/// another with [`wait`]: the first reply ends while `wait` parks.
struct WaitsBetweenAwaits;

impl Drive for WaitsBetweenAwaits {
    type Instruction = bool;
    /// The first reply's status, polled at the end.
    type Output = ReplyStatus;

    async fn drive<E: Executor<InstructionBatch<bool>>>(
        self,
        _: &mut E,
        _: &ShotSet,
    ) -> ReplyStatus {
        let (first_end, mut first) = reply_channel::<ShotOutcome>();
        let (second_end, second) = reply_channel::<ShotOutcome>();
        let mut outcomes = Vec::new();
        let status = poll_fn(|cx| Poll::Ready(first.poll_events(&mut outcomes, cx.waker()))).await;
        assert_eq!(status, ReplyStatus::Pending, "no thread has ended it yet");
        thread::spawn(move || {
            first_end.done();
            thread::sleep(Duration::from_millis(5));
            second_end.done();
        });
        wait(second, &mut outcomes).expect("the second reply ends");
        // Pending until the first reply's wake, which came while `wait`
        // parked, has the program polled again.
        let mut woken = false;
        poll_fn(|_| {
            if woken {
                Poll::Ready(())
            } else {
                woken = true;
                Poll::Pending
            }
        })
        .await;
        poll_fn(|cx| Poll::Ready(first.poll_events(&mut outcomes, cx.waker()))).await
    }
}

/// A run's own parking loses none of its program's wakes to the program's
/// `wait`: a wake that comes while the program parks in `wait` has the
/// program polled again once `wait` returns.
#[test]
fn a_run_waited_for_keeps_the_wakes_that_come_while_its_program_waits() {
    let first = ends_soon(|| {
        let driver = Driver::new(WaitsBetweenAwaits, shots(&[0]));
        let (output, _) = (driver >> TestDoubleExecutor::with_model(Workers, 7))
            .expect("no adapter, no fault")
            .wait();
        output
    });
    assert_eq!(first, ReplyStatus::Done, "the first reply ended");
}

/// A program of the bits' set that calls shots 1 to 4 at once, whatever
/// shots it is given.
struct CallsShotsOneToFour;

impl Drive for CallsShotsOneToFour {
    type Instruction = InstructionCall;
    type Output = Result<(), Rejected>;

    fn needs(&self) -> Needs {
        Needs::default().with_instructions(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(self, executor: &mut E, _: &ShotSet) -> Result<(), Rejected>
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        let batch = InstructionBatchRef {
            shots: &shots(&[1, 2, 3, 4]),
            instructions: &[],
        };
        executor.call(batch).map(drop)
    }
}

/// A driver of shots 1 and 3 that calls shots 2 and 4 too is refused before the
/// executor sees the call: the executor ran no batch and made no shot.
#[test]
fn a_call_on_a_shot_outside_the_drivers_is_refused_before_the_executor_sees_it() {
    let driver = Driver::new(CallsShotsOneToFour, shots(&[1, 3]));
    let (refused, report) = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    refused.expect_err("shots 2 and 4 are outside the driver's shots");
    assert_eq!(report, ExecutorReport::default(), "the executor saw a call");
}

/// A driver of shots 1 and 3 that calls shots 2 and 4 too, one in the gap
/// between its shots, is refused naming shot 2, the first outside its
/// shots.
#[test]
fn a_call_on_shots_outside_the_drivers_is_refused_naming_the_first() {
    let driver = Driver::new(CallsShotsOneToFour, shots(&[1, 3]));
    let (refused, _) = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(refused, Err(Rejected::ShotNotHeld(2)));
}

/// A program of the bits' set that calls each of its sets of shots in turn,
/// whatever shots it is given, and gives each call's result.
struct CallsEach(Vec<ShotSet>);

impl Drive for CallsEach {
    type Instruction = InstructionCall;
    type Output = Vec<Result<(), Rejected>>;

    fn needs(&self) -> Needs {
        Needs::default().with_instructions(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(self, executor: &mut E, _: &ShotSet) -> Vec<Result<(), Rejected>>
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        self.0
            .iter()
            .map(|shots| {
                executor
                    .call(InstructionBatchRef {
                        shots,
                        instructions: &[],
                    })
                    .map(drop)
            })
            .collect()
    }
}

/// A driver given its shots out of order, 9, 1 and 4, holds each of them:
/// its calls on shot 4 alone, on shots 4 and 9, and on all three in another
/// order all run.
#[test]
fn a_driver_whose_shots_are_out_of_order_runs_calls_on_them_in_any_order() {
    let calls = CallsEach(vec![shots(&[4]), shots(&[4, 9]), shots(&[1, 9, 4])]);
    let driver = Driver::new(calls, shots(&[9, 1, 4]));
    let (ran, _) = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(ran, [Ok(()), Ok(()), Ok(())]);
}

/// A driver given its shots out of order, 9, 1 and 4, that calls shots 4, 7
/// and 2 is refused naming shot 7, the first in the call outside its shots,
/// though shot 2 is the least.
#[test]
fn a_call_outside_a_driver_whose_shots_are_out_of_order_is_refused_naming_the_first() {
    let driver = Driver::new(CallsEach(vec![shots(&[4, 7, 2])]), shots(&[9, 1, 4]));
    let (refused, _) = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(refused, [Err(Rejected::ShotNotHeld(7))]);
}

/// A program of the bits' set that calls shot 0, then shot 3, each alone,
/// and gives both calls' results.
struct CallsShotZeroThenShotThree;

impl Drive for CallsShotZeroThenShotThree {
    type Instruction = InstructionCall;
    type Output = [Result<(), Rejected>; 2];

    fn needs(&self) -> Needs {
        Needs::default().with_instructions(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(self, executor: &mut E, _: &ShotSet) -> [Result<(), Rejected>; 2]
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        [0, 3].map(|j| {
            let alone = InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[],
            };
            executor.call(alone).map(drop)
        })
    }
}

/// A driver of the contiguous shots 1 and 2 that calls shot 0, just below
/// them, then shot 3, just above, is refused each time, naming the shot.
#[test]
fn a_call_on_a_shot_just_outside_a_drivers_range_is_refused() {
    let driver = Driver::new(
        CallsShotZeroThenShotThree,
        ShotSet::range(1..=2).expect("1..=2 is not empty"),
    );
    let (refused, _) = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(
        refused,
        [Err(Rejected::ShotNotHeld(0)), Err(Rejected::ShotNotHeld(3))]
    );
}

/// A program of the bits' set that calls shots 1 to 3 at once, then its
/// own shots.
struct CallsShotsOneToThreeThenItsOwn;

impl Drive for CallsShotsOneToThreeThenItsOwn {
    type Instruction = InstructionCall;
    type Output = Result<(), Rejected>;

    fn needs(&self) -> Needs {
        Needs::default().with_instructions(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(self, executor: &mut E, own: &ShotSet) -> Result<(), Rejected>
    where
        E: Executor<InstructionBatch<InstructionCall>>,
    {
        let one_to_three = InstructionBatchRef {
            shots: &shots(&[1, 2, 3]),
            instructions: &[],
        };
        executor
            .call(one_to_three)
            .map(drop)
            .expect_err("shot 2 is outside the driver's shots");
        executor
            .call(InstructionBatchRef {
                shots: own,
                instructions: &[],
            })
            .map(drop)
    }
}

/// A call refused for a shot outside the driver's leaves the executor
/// usable: the driver's next call, on its own shots, runs.
#[test]
fn a_call_refused_for_a_shot_outside_the_drivers_leaves_the_executor_usable() {
    let driver = Driver::new(CallsShotsOneToThreeThenItsOwn, shots(&[1, 3]));
    let (own, _) = (driver >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(own, Ok(()));
}

/// A program of `bool`s that reads on its shots, then on shot `.0`, and
/// gives that call's result.
struct ReadsThenCalls(ShotId);

impl Drive for ReadsThenCalls {
    type Instruction = bool;
    type Output = Result<(), Rejected>;

    async fn drive<E: Executor<InstructionBatch<bool>>>(
        self,
        executor: &mut E,
        own: &ShotSet,
    ) -> Result<(), Rejected> {
        let read = InstructionBatchRef {
            shots: own,
            instructions: &[true],
        };
        let reply = executor
            .call(read)
            .expect("the executor runs the driver's shot");
        wait(reply, &mut Vec::new()).expect("a test double state fails only when told to");
        let other = InstructionBatchRef {
            shots: &ShotSet::one(self.0),
            instructions: &[true],
        };
        executor.call(other).map(drop)
    }
}

/// A driver of shot 5 on a device that holds one state (an executor made
/// with `executor_fn` and no shot strategy) reads on shot 5, then calls
/// shot 6: the run refuses the call, naming shot 6, and the device makes
/// no state for it.
#[test]
fn a_run_of_one_shot_on_a_one_state_executor_refuses_a_call_on_another() {
    let made = Rc::new(Cell::new(0));
    let counted = Rc::clone(&made);
    let device = executor_fn(
        Capabilities::default(),
        move |_| {
            counted.set(counted.get() + 1);
            Ok(())
        },
        |(): &mut (), input: &ProgramInput<'_, bool>, outcomes: &mut Outcomes<'_>| {
            for (tag, &one) in (0..).zip(input.instructions()) {
                outcomes.push(Outcome::new(tag, one.into()));
            }
            Ok(())
        },
    );
    let driver = Driver::new(ReadsThenCalls(6), ShotSet::one(5));
    let (refused, _) = (driver >> device).expect("one shot fits one state").wait();
    assert_eq!(refused, Err(Rejected::ShotNotHeld(6)));
    assert_eq!(made.get(), 1, "only shot 5's state is made");
}

/// A program of `bool`s that reads on its shots, to the read's end, then
/// on shot `.0`, and gives how the read ended and that call's result.
struct ReadsToItsEndThenCalls(ShotId);

impl Drive for ReadsToItsEndThenCalls {
    type Instruction = bool;
    type Output = (Result<Result<(), String>, Rejected>, Result<(), Rejected>);

    async fn drive<E: Executor<InstructionBatch<bool>>>(
        self,
        executor: &mut E,
        own: &ShotSet,
    ) -> Self::Output {
        let read = test_double::run(executor, own, &[true]).map(|(_, end)| end);
        let other = InstructionBatchRef {
            shots: &ShotSet::one(self.0),
            instructions: &[true],
        };
        (read, executor.call(other).map(drop))
    }
}

/// A driver of shot 1, whose read fails, that then calls shot 2 is refused
/// naming shot 2, not the earlier failure: the run checks the driver's
/// shots before the executor, which alone knows of the failure.
#[test]
fn a_call_on_a_shot_outside_the_drivers_is_refused_before_an_earlier_failure() {
    let failing = TestDoubleModel {
        failing: Some(1),
        ..TestDoubleModel::default()
    };
    let driver = Driver::new(ReadsToItsEndThenCalls(2), ShotSet::one(1));
    let ((read, other), _) = (driver >> TestDoubleExecutor::with_model(failing, 7))
        .expect("no adapter, no fault")
        .wait();
    read.expect("the executor accepts the read")
        .expect_err("shot 1's read fails");
    assert_eq!(other, Err(Rejected::ShotNotHeld(2)));
}
