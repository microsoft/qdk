// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::{IntoFuture, poll_fn};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::thread;
use std::time::Duration;

use super::{
    Log, ThreeReads, Waits, Workers, entries, flip_and_toss_batches, flip_and_toss_job, logger,
};
use crate::tests::map_executor;
use crate::tests::test_double::{self, Model, TestDoubleExecutor, TestDoubleModel};
use crate::tests::{
    Batches, Bits, BitsState, ByHand, Count, YieldNow, bits_set, block_on, ends_soon,
    flip_and_toss, run_tasks, shots, values,
};
use crate::{
    AdapterError, AdapterLogic, Below, CallInput, Capabilities, Close, ComposedJob, Describe,
    Driver, DriverExt, Executor, ExecutorLogic, ExecutorReport, Instruction, InstructionBatch,
    InstructionBatchRef, InstructionCall, MakeState, Needs, NoAdapter, NoCheck, OnePerShot,
    OnePerShotAdapter, OnePerShotExecutor, Opcode, Outcome, Outcomes, Program, ReadoutFailure,
    Rejected, Reply, ReplyStatus, ShotContext, ShotId, ShotOutcome, ShotSet, StaticProgram, Value,
    adapter_fn, compose, driver, executor_fn, reply_channel, wait, wait_async,
};

/// A job composed with adapters and an executor makes no shot until its
/// run is waited for.
#[test]
fn a_run_runs_nothing_until_it_is_waited_for() {
    let (log, seen) = (Log::default(), Arc::default());
    let recording = {
        let seen = Arc::clone(&seen);
        map_executor::<InstructionCall, _>(move |executor| Recorded { executor, seen })
    };
    let job = flip_and_toss_job(bits_set(), &[0]);
    let run = (job
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
    let job = flip_and_toss_job(bits_set(), &[0]);
    let run = compose(job, executor).expect("no adapter, no fault");
    let future = run.into_future();
    assert_eq!(seen(&record), [], "a call or a close before the poll");
    block_on(future);
    let calls = seen(&record)
        .into_iter()
        .filter(|&event| event == Seen::Call);
    assert_eq!(calls.count(), 2, "the program's two batches");
}

/// A run closes its executor once, after the driver's last call: an
/// executor holding a device releases it when the driver is done with it,
/// never while it still calls.
#[test]
fn a_run_closes_its_executor_once_after_the_drivers_last_call() {
    let (executor, record) = Recorded::new(TestDoubleExecutor::with_model(Bits::default(), 7));
    let job = flip_and_toss_job(bits_set(), &[0, 1]);
    compose(job, executor).expect("no adapter, no fault").wait();
    assert_eq!(seen(&record), [Seen::Call, Seen::Call, Seen::Close]);
}

/// A run whose driver never calls its executor, here for a program of no
/// batches, still closes it, once: an executor holding a device releases
/// it whatever the driver did.
#[test]
fn a_run_whose_driver_never_calls_still_closes_its_executor() {
    let (executor, record) = Recorded::new(TestDoubleExecutor::with_model(Bits::default(), 7));
    let idle = Batches {
        batches: Vec::new(),
        ..flip_and_toss_batches(bits_set())
    };
    compose(ComposedJob::driven(ByHand, idle, shots(&[0, 1])), executor)
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(seen(&record), [Seen::Close]);
}

/// A program of `I`s, giving an `O`, written in its driver: it has no
/// content, and its driver makes its calls. The drivers below each make
/// fixed calls, some on shots not the job's, to show how a run takes them.
struct WrittenInItsDriver<I, O>(PhantomData<fn() -> (I, O)>);

impl<I: Instruction, O> Program for WrittenInItsDriver<I, O> {
    type Instruction = I;
    type Output = O;
}

/// The job of the program written in `driver` ([`WrittenInItsDriver`]),
/// on `shots`.
fn job_of<T, I, O>(
    driver: T,
    shots: ShotSet,
) -> ComposedJob<T, WrittenInItsDriver<I, O>, NoAdapter<I>>
where
    T: Driver<WrittenInItsDriver<I, O>>,
    I: Instruction,
{
    ComposedJob::driven(driver, WrittenInItsDriver(PhantomData), shots)
}

/// A driver of `bool`s that reads twice on its job's shots, each read to
/// its end, and gives how each call went: refused, or how its reply ended.
struct ReadsTwice;

type TwoReads = [Result<Result<(), String>, Rejected>; 2];

impl Driver<WrittenInItsDriver<bool, TwoReads>> for ReadsTwice {
    async fn drive<E: Executor<InstructionBatch<bool>>>(
        &self,
        _: WrittenInItsDriver<bool, TwoReads>,
        executor: &mut E,
        own: &ShotSet,
    ) -> TwoReads {
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
    let ([first, second], _) = compose(job_of(ReadsTwice, shots(&[0, 1])), executor)
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
    let job = flip_and_toss_job(bits_set(), &[0]);
    compose(job >> logger("debugger", &log), executor)
        .expect("a debugger fits any executor")
        .wait();
    assert_eq!(seen(&record), [Seen::Call, Seen::Call, Seen::Close]);
}

/// A run waited for gives what its program gives, and the report its
/// executor gave when the run closed it, each shot's report folded.
#[test]
fn a_run_gives_the_programs_output_and_the_executors_report() {
    let job = flip_and_toss_job(bits_set(), &[0, 1, 2]);
    let (outcomes, report) = (job >> TestDoubleExecutor::with_model(Bits::default(), 7))
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

/// A device that loses shot 1's first qubit, so that its first readout has
/// no value, under an observer made with `adapter_fn`, running a static
/// program of two batches by the crate's driver: the run goes on to its
/// end, and gives the missing readout with that shot's other readouts and
/// the second batch's.
#[test]
fn a_missing_value_does_not_end_the_run_and_the_shots_other_readouts_stand() {
    let reads = |shot: ShotId, tag: u32| {
        if (shot, tag) == (1, 0) {
            Value::Missing(ReadoutFailure::Loss)
        } else {
            Value::One
        }
    };
    let device = executor_fn(
        Capabilities::default().with_resumable_shots(true),
        |context: ShotContext| Ok((context.shot(), 0)),
        move |(shot, tag): &mut (ShotId, u32),
              input: &CallInput<'_, bool>,
              outcomes: &mut Outcomes<'_>| {
            for _ in input.instructions().iter().filter(|&&read| read) {
                outcomes.push(Outcome::new(*tag, reads(*shot, *tag)));
                *tag += 1;
            }
            Ok(())
        },
    )
    .shots(OnePerShot);
    let observer = adapter_fn(
        |_| Ok(()),
        |(): &mut (),
         input: &CallInput<'_, bool>,
         below: &mut Below<'_, bool>,
         _: &mut Outcomes<'_>| below.send(input.instructions()),
    );
    let program = StaticProgram::new(Needs::default(), vec![vec![true, true], vec![true]]);
    let (output, _) = (driver() >> observer >> device)
        .expect("the observer sends the device what it accepts")
        .run(program, [0, 1, 2])
        .expect("three shots, which the device resumes")
        .wait();
    let mut read = values(&output.expect("a missing value fails no batch"));
    read.sort_unstable_by_key(|&(shot, tag, _)| (shot, tag));
    let expected: Vec<_> = (0..3)
        .flat_map(|shot| (0..3).map(move |tag| (shot, tag, reads(shot, tag))))
        .collect();
    assert_eq!(read, expected);
}

/// A chain composed with a probe that draws nothing and sends what it
/// accepts (a logger) gives the outcomes that its driver gives driving the
/// executor alone, built with the same seed, on the same shots, coins
/// included.
#[test]
fn a_run_draws_what_its_executor_alone_draws() {
    let (seed, ids) = (11, [0, 4, 9]);
    let alone = {
        let program = flip_and_toss_batches(bits_set());
        let mut executor = TestDoubleExecutor::with_model(Bits::default(), seed);
        block_on(ByHand.drive(program, &mut executor, &shots(&ids)))
    };
    let log = Log::default();
    let job = flip_and_toss_job(bits_set(), &ids);
    let (outcomes, _) =
        (job >> logger("debugger", &log) >> TestDoubleExecutor::with_model(Bits::default(), seed))
            .expect("a logger fits")
            .wait();
    assert_eq!(values(&outcomes), values(&alone));
}

/// The program of a simulator of [`Bits`] made with `executor_fn`: runs
/// the batch on the shot's bits.
fn run_bits(
    bits: &mut BitsState,
    input: &CallInput<'_, InstructionCall>,
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
        let program = flip_and_toss_batches(bits_set());
        let (outcomes, _) = (ByHand.into_chain() >> simulator)
            .expect("no adapter, no fault")
            .run(program, shots(ids))
            .expect("the simulator runs the bits' set")
            .wait();
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
    input: &CallInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    below.send(input.instructions())
}

/// The run's job: [`flip_and_toss`] as [`ByHand`] drives it, on shots
/// that are not a range.
fn job() -> ComposedJob<ByHand, Batches, NoAdapter<InstructionCall>> {
    flip_and_toss_job(bits_set(), &[2, 5, 6, 11])
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

/// The values the run of `job() >> simulator()` reads, no adapter
/// between them.
fn read_alone() -> Vec<(u32, u32, Value)> {
    let (alone, _) = (job() >> simulator()).expect("no adapter").wait();
    assert!(!alone.is_empty(), "the program reads bits");
    values(&alone)
}

/// `job >> observer >> simulator`, an observer made with `adapter_fn`
/// with one state: grouped either way with `>>`, the chain gives the
/// outcomes the simulator gives alone.
#[test]
fn a_job_and_an_adapter_fn_compose_in_either_grouping() {
    let observer = || adapter_fn(|_| Ok(()), send_as_is);
    let (left, _) = ((job() >> observer()) >> simulator())
        .expect("an observer fits")
        .wait();
    let downstream = (observer() >> simulator()).expect("an observer fits");
    let (right, _) = (job() >> downstream)
        .expect("the program needs what it offers")
        .wait();
    assert_eq!(values(&left), read_alone());
    assert_eq!(values(&right), read_alone());
}

/// `job >> profiler >> simulator`, a profiler made with `adapter_fn`
/// with a state per shot: grouped either way with `>>`, the chain gives
/// the outcomes the simulator gives alone.
#[test]
fn a_job_and_a_per_shot_adapter_fn_compose_in_either_grouping() {
    let profiler = || adapter_fn(|_| Ok(()), send_as_is).shots(OnePerShot);
    let (left, _) = ((job() >> profiler()) >> simulator())
        .expect("a profiler fits")
        .wait();
    let downstream = (profiler() >> simulator()).expect("a profiler fits");
    let (right, _) = (job() >> downstream)
        .expect("the program needs what it offers")
        .wait();
    assert_eq!(values(&left), read_alone());
    assert_eq!(values(&right), read_alone());
}

/// `job >> observer >> profiler >> simulator`, the profiler with a
/// state per shot, the observer with one: the adapters grouped either way
/// with `>>`, the chain gives the outcomes the simulator gives alone.
#[test]
fn two_adapter_fns_compose_in_either_grouping() {
    let observer = || adapter_fn(|_| Ok(()), send_as_is);
    let profiler = || adapter_fn(|_| Ok(()), send_as_is).shots(OnePerShot);
    let first = ((observer() >> profiler()) >> simulator()).expect("the adapters fit");
    let (left, _) = (job() >> first).expect("the job fits").wait();
    let downstream = (profiler() >> simulator()).expect("a profiler fits");
    let second = (observer() >> downstream).expect("an observer fits");
    let (right, _) = (job() >> second).expect("the job fits").wait();
    assert_eq!(values(&left), read_alone());
    assert_eq!(values(&right), read_alone());
}

/// A profiler's counts: the calls of each instruction it saw.
type Counts = BTreeMap<Opcode, u64>;

/// The logic of the target chain's profiler: counts what its shot runs,
/// then sends the call down as it is.
fn profile(
    counts: &mut Counts,
    input: &CallInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
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

/// What the profiler counts over the run of `job()`: each of its four
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

/// The report of the run of `job() >> simulator()`, no adapter between
/// them.
fn report_alone() -> ExecutorReport<()> {
    let (_, report) = (job() >> simulator()).expect("no adapter").wait();
    report
}

/// The target chain with its profiler: `run.wait()` gives the profiler's
/// counts, folded over the run's four shots, beside the simulator's
/// report.
#[test]
fn a_runs_wait_gives_the_adapters_report_beside_the_one_below() {
    let run = (job() >> profiler() >> simulator()).expect("a profiler fits");
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
    let run =
        (job() >> debugger >> profiler() >> transpiler >> simulator()).expect("the adapters fit");
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
            let run = job() >> profiler() >> lower() >> simulator();
            run.expect("the profilers fit").wait().1
        }),
        ("d >> (a >> b) >> e", {
            let run = job() >> (profiler() >> lower()) >> simulator();
            run.expect("the profilers fit").wait().1
        }),
        ("d >> a >> (b >> e)", {
            let below = (lower() >> simulator()).expect("a profiler fits");
            let run = job() >> profiler() >> below;
            run.expect("the profilers fit").wait().1
        }),
        ("d >> (a >> (b >> e))", {
            let below = (lower() >> simulator()).expect("a profiler fits");
            let both = (profiler() >> below).expect("a profiler fits");
            (job() >> both).expect("the job fits").wait().1
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

/// A driver that flips and reads, in the bits' set, on its job's shots,
/// then tosses and reads on `then`, some of them.
struct OnItsShotsThenSome {
    then: ShotSet,
}

impl Driver<WrittenInItsDriver<InstructionCall, ()>> for OnItsShotsThenSome {
    fn needs(&self, _: &WrittenInItsDriver<InstructionCall, ()>) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::new(bits_set())))
    }

    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        _: WrittenInItsDriver<InstructionCall, ()>,
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
                .expect("the bits run the set's batches on the job's shots");
            wait(reply, &mut Vec::new()).expect("the bits do not fail");
        }
    }
}

/// A job of shots 9, 1 and 4 that calls all of them, then shots 4 and 9:
/// the last executor sees each of these shots as often as the job named
/// it, none more and no other, with no adapter, one, or two.
#[test]
fn the_shots_a_job_names_reach_the_last_executor_through_every_adapter() {
    let job = || {
        let then = shots(&[4, 9]);
        job_of(OnItsShotsThenSome { then }, shots(&[9, 1, 4]))
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
    compose(job(), none).expect("no adapter, no fault").wait();
    let (one, seen_through_one) = executor();
    compose(job() >> logger("debugger", &log), one)
        .expect("a logger fits")
        .wait();
    let (two, seen_through_two) = executor();
    compose(job() >> passing() >> logger("debugger", &log), two)
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
        let job = flip_and_toss_job(bits_set(), &[0, 1]);
        (job >> TestDoubleExecutor::with_model(Bits::default(), 7)).expect("no adapter, no fault")
    };
    let awaited = block_on(async { run().await });
    let waited = run().wait();
    assert_eq!(awaited, waited);
}

/// A run whose driver, program and executor are `Send`, awaited: its future is
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
    let job = flip_and_toss_job(bits_set(), &[0, 1]);
    let run =
        (job >> TestDoubleExecutor::with_model(Bits::default(), 7)).expect("no adapter, no fault");
    assert_send(&run);
    NotSend::checked(&run.into_future());
}

/// A driver of `bool`s that reads once on its job's shots and awaits the
/// reply.
struct AwaitsARead;

impl Driver<WrittenInItsDriver<bool, Vec<ShotOutcome>>> for AwaitsARead {
    async fn drive<E: Executor<InstructionBatch<bool>>>(
        &self,
        _: WrittenInItsDriver<bool, Vec<ShotOutcome>>,
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
/// tasks while its driver awaits a reply: the run's read ends only when
/// another task, on the same thread, ends it. Run on a thread of its own,
/// so that a run that blocked its thread fails the test instead of hanging
/// it.
#[test]
fn a_run_awaited_leaves_its_thread_to_other_tasks_while_its_driver_awaits() {
    let log = ends_soon(|| {
        let model = TestDoubleModel {
            deferred: vec![0],
            ..TestDoubleModel::default()
        };
        let deferred = Rc::clone(&model.seen);
        let run = (job_of(AwaitsARead, shots(&[0])) >> TestDoubleExecutor::with_model(model, 7))
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

/// A synchronous driver, which parks its thread in `wait` for replies that
/// end on other threads, runs to its end when its run is waited for.
#[test]
fn a_run_waited_for_runs_a_driver_that_waits_for_replies_from_other_threads() {
    let outcomes = ends_soon(|| {
        let job = ComposedJob::driven(Waits, ThreeReads, shots(&[0, 1]));
        let (output, _) = (job >> TestDoubleExecutor::with_model(Workers, 7))
            .expect("no adapter, no fault")
            .wait();
        output
    });
    assert_eq!(outcomes, 6, "one outcome per shot and batch");
}

/// A driver that awaits one reply and, before it is polled again, takes
/// another with [`wait`]: the first reply ends while `wait` parks. It
/// gives the first reply's status, polled at the end.
struct WaitsBetweenAwaits;

impl Driver<WrittenInItsDriver<bool, ReplyStatus>> for WaitsBetweenAwaits {
    async fn drive<E: Executor<InstructionBatch<bool>>>(
        &self,
        _: WrittenInItsDriver<bool, ReplyStatus>,
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
        // parked, has the driver polled again.
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

/// A run's own parking loses none of its driver's wakes to the driver's
/// `wait`: a wake that comes while the driver parks in `wait` has the
/// driver polled again once `wait` returns.
#[test]
fn a_run_waited_for_keeps_the_wakes_that_come_while_its_driver_waits() {
    let first = ends_soon(|| {
        let job = job_of(WaitsBetweenAwaits, shots(&[0]));
        let (output, _) = (job >> TestDoubleExecutor::with_model(Workers, 7))
            .expect("no adapter, no fault")
            .wait();
        output
    });
    assert_eq!(first, ReplyStatus::Done, "the first reply ended");
}

/// A driver that calls shots 1 to 4 at once, in the bits' set, whatever
/// its job's shots.
struct CallsShotsOneToFour;

impl Driver<WrittenInItsDriver<InstructionCall, Result<(), Rejected>>> for CallsShotsOneToFour {
    fn needs(&self, _: &WrittenInItsDriver<InstructionCall, Result<(), Rejected>>) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(
        &self,
        _: WrittenInItsDriver<InstructionCall, Result<(), Rejected>>,
        executor: &mut E,
        _: &ShotSet,
    ) -> Result<(), Rejected>
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

/// A job of shots 1 and 3 that calls shots 2 and 4 too is refused before the
/// executor sees the call: the executor ran no batch and made no shot.
#[test]
fn a_call_on_a_shot_outside_the_jobs_is_refused_before_the_executor_sees_it() {
    let job = job_of(CallsShotsOneToFour, shots(&[1, 3]));
    let (refused, report) = (job >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    refused.expect_err("shots 2 and 4 are outside the job's shots");
    assert_eq!(report, ExecutorReport::default(), "the executor saw a call");
}

/// A job of shots 1 and 3 that calls shots 2 and 4 too, one in the gap
/// between its shots, is refused naming shot 2, the first outside its
/// shots.
#[test]
fn a_call_on_shots_outside_the_jobs_is_refused_naming_the_first() {
    let job = job_of(CallsShotsOneToFour, shots(&[1, 3]));
    let (refused, _) = (job >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(refused, Err(Rejected::ShotNotHeld(2)));
}

/// A driver that calls each of its sets of shots in turn, in the bits'
/// set, whatever its job's shots, and gives each call's result.
struct CallsEach(Vec<ShotSet>);

impl Driver<WrittenInItsDriver<InstructionCall, Vec<Result<(), Rejected>>>> for CallsEach {
    fn needs(&self, _: &WrittenInItsDriver<InstructionCall, Vec<Result<(), Rejected>>>) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(
        &self,
        _: WrittenInItsDriver<InstructionCall, Vec<Result<(), Rejected>>>,
        executor: &mut E,
        _: &ShotSet,
    ) -> Vec<Result<(), Rejected>>
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

/// A job given its shots out of order, 9, 1 and 4, holds each of them:
/// its calls on shot 4 alone, on shots 4 and 9, and on all three in another
/// order all run.
#[test]
fn a_job_whose_shots_are_out_of_order_runs_calls_on_them_in_any_order() {
    let calls = CallsEach(vec![shots(&[4]), shots(&[4, 9]), shots(&[1, 9, 4])]);
    let job = job_of(calls, shots(&[9, 1, 4]));
    let (ran, _) = (job >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(ran, [Ok(()), Ok(()), Ok(())]);
}

/// A job given its shots out of order, 9, 1 and 4, that calls shots 4, 7
/// and 2 is refused naming shot 7, the first in the call outside its shots,
/// though shot 2 is the least.
#[test]
fn a_call_outside_a_job_whose_shots_are_out_of_order_is_refused_naming_the_first() {
    let job = job_of(CallsEach(vec![shots(&[4, 7, 2])]), shots(&[9, 1, 4]));
    let (refused, _) = (job >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(refused, [Err(Rejected::ShotNotHeld(7))]);
}

/// A driver that calls shot 0, then shot 3, each alone, in the bits' set,
/// and gives both calls' results.
struct CallsShotZeroThenShotThree;

type TwoCalls = [Result<(), Rejected>; 2];

impl Driver<WrittenInItsDriver<InstructionCall, TwoCalls>> for CallsShotZeroThenShotThree {
    fn needs(&self, _: &WrittenInItsDriver<InstructionCall, TwoCalls>) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(
        &self,
        _: WrittenInItsDriver<InstructionCall, TwoCalls>,
        executor: &mut E,
        _: &ShotSet,
    ) -> TwoCalls
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

/// A job of the contiguous shots 1 and 2 that calls shot 0, just below
/// them, then shot 3, just above, is refused each time, naming the shot.
#[test]
fn a_call_on_a_shot_just_outside_a_jobs_range_is_refused() {
    let job = job_of(
        CallsShotZeroThenShotThree,
        ShotSet::range(1..=2).expect("1..=2 is not empty"),
    );
    let (refused, _) = (job >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(
        refused,
        [Err(Rejected::ShotNotHeld(0)), Err(Rejected::ShotNotHeld(3))]
    );
}

/// A driver that calls shots 1 to 3 at once, in the bits' set, then its
/// job's own shots.
struct CallsShotsOneToThreeThenItsOwn;

impl Driver<WrittenInItsDriver<InstructionCall, Result<(), Rejected>>>
    for CallsShotsOneToThreeThenItsOwn
{
    fn needs(&self, _: &WrittenInItsDriver<InstructionCall, Result<(), Rejected>>) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::new(bits_set())))
    }

    async fn drive<E>(
        &self,
        _: WrittenInItsDriver<InstructionCall, Result<(), Rejected>>,
        executor: &mut E,
        own: &ShotSet,
    ) -> Result<(), Rejected>
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
            .expect_err("shot 2 is outside the job's shots");
        executor
            .call(InstructionBatchRef {
                shots: own,
                instructions: &[],
            })
            .map(drop)
    }
}

/// A call refused for a shot outside the job's leaves the executor
/// usable: the driver's next call, on its job's shots, runs.
#[test]
fn a_call_refused_for_a_shot_outside_the_jobs_leaves_the_executor_usable() {
    let job = job_of(CallsShotsOneToThreeThenItsOwn, shots(&[1, 3]));
    let (own, _) = (job >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("no adapter, no fault")
        .wait();
    assert_eq!(own, Ok(()));
}

/// A driver of `bool`s that reads on its job's shots, then on shot `.0`,
/// and gives that call's result.
struct ReadsThenCalls(ShotId);

impl Driver<WrittenInItsDriver<bool, Result<(), Rejected>>> for ReadsThenCalls {
    async fn drive<E: Executor<InstructionBatch<bool>>>(
        &self,
        _: WrittenInItsDriver<bool, Result<(), Rejected>>,
        executor: &mut E,
        own: &ShotSet,
    ) -> Result<(), Rejected> {
        let read = InstructionBatchRef {
            shots: own,
            instructions: &[true],
        };
        let reply = executor
            .call(read)
            .expect("the executor runs the job's shot");
        wait(reply, &mut Vec::new()).expect("a test double state fails only when told to");
        let other = InstructionBatchRef {
            shots: &ShotSet::one(self.0),
            instructions: &[true],
        };
        executor.call(other).map(drop)
    }
}

/// A job of shot 5 on a device that holds one state (an executor made
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
        |(): &mut (), input: &CallInput<'_, bool>, outcomes: &mut Outcomes<'_>| {
            for (tag, &one) in (0..).zip(input.instructions()) {
                outcomes.push(Outcome::new(tag, one.into()));
            }
            Ok(())
        },
    );
    let job = job_of(ReadsThenCalls(6), ShotSet::one(5));
    let (refused, _) = (job >> device)
        .expect("one shot fits the one shot it holds")
        .wait();
    assert_eq!(refused, Err(Rejected::ShotNotHeld(6)));
    assert_eq!(made.get(), 1, "only shot 5's state is made");
}

/// A driver of `bool`s that reads on its job's shots, to the read's end,
/// then on shot `.0`, and gives how the read ended and that call's result.
struct ReadsToItsEndThenCalls(ShotId);

type ReadThenCall = (Result<Result<(), String>, Rejected>, Result<(), Rejected>);

impl Driver<WrittenInItsDriver<bool, ReadThenCall>> for ReadsToItsEndThenCalls {
    async fn drive<E: Executor<InstructionBatch<bool>>>(
        &self,
        _: WrittenInItsDriver<bool, ReadThenCall>,
        executor: &mut E,
        own: &ShotSet,
    ) -> ReadThenCall {
        let read = test_double::run(executor, own, &[true]).map(|(_, end)| end);
        let other = InstructionBatchRef {
            shots: &ShotSet::one(self.0),
            instructions: &[true],
        };
        (read, executor.call(other).map(drop))
    }
}

/// A job of shot 1, whose read fails, that then calls shot 2 is refused
/// naming shot 2, not the earlier failure: the run checks the job's
/// shots before the executor, which alone knows of the failure.
#[test]
fn a_call_on_a_shot_outside_the_jobs_is_refused_before_an_earlier_failure() {
    let failing = TestDoubleModel {
        failing: Some(1),
        ..TestDoubleModel::default()
    };
    let job = job_of(ReadsToItsEndThenCalls(2), ShotSet::one(1));
    let ((read, other), _) = (job >> TestDoubleExecutor::with_model(failing, 7))
        .expect("no adapter, no fault")
        .wait();
    read.expect("the executor accepts the read")
        .expect_err("shot 1's read fails");
    assert_eq!(other, Err(Rejected::ShotNotHeld(2)));
}
