// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use super::{
    Accepting, DecodeCalls, DescribedWorkers, Log, Program, Workers, bits_without_coin, entries,
    logger, missing_coin,
};
use crate::Need;
use crate::tests::executor_fn::{
    Freed, Op as DeviceOp, PowerDownFailed, freed_so_far, powering_down_badly, promise, runs,
};
use crate::tests::test_double::TestDoubleModel;
use crate::tests::test_double::{Model, Seen, TestDoubleExecutor, run};
use crate::tests::{
    Bits, BitsState, EVERY_BIT, Relabel, StatesItsOfferFromScratch, bits_set, call, declaring,
    flip_and_toss, reversed_set, run_and_close, shots, undeclaring, values,
};
use crate::tests::{PanicsOnClose, map_executor};
use crate::{
    Adapter, AdapterBoundary, AdapterSeeds, Apply, BuildError, Capabilities, Close, ComponentId,
    Describe, Driver, Executor, ExecutorReport, ImmediateReply, InstructionBatch,
    InstructionBatchRef, InstructionCall, InstructionMismatch, InstructionSet, OnePerShot, Opcode,
    Operand, OperandKind, Rejected, Reply, ReplyStatus, RngIds, ShotContext, ShotId, ShotOutcome,
    ShotSet, TypedInstruction, Value, compose, conformance, executor_fn, shot_seed, wait,
};

use OperandKind::Block;

/// An executor whose calls an adapter written by hand records, the shots
/// of each: two shots of a batch through it make one application of the
/// adapter, at compose, and one call, on both shots, as the executor alone
/// is called.
#[test]
fn an_adapter_written_by_hand_is_applied_once_to_the_whole_executor() {
    let (applied, seen) = (Arc::new(AtomicUsize::new(0)), Arc::default());
    let recording = {
        let (applied, seen) = (Arc::clone(&applied), Arc::clone(&seen));
        map_executor::<InstructionCall, _>(move |executor| {
            applied.fetch_add(1, Ordering::SeqCst);
            SeesShots { executor, seen }
        })
    };
    let executor = compose(
        recording,
        TestDoubleExecutor::with_model(Bits::default(), 7),
    )
    .expect("it passes what the bits run");
    run_and_close(executor, &[3, 5], &flip_and_toss(&bits_set())[0]);
    assert_eq!(applied.load(Ordering::SeqCst), 1);
    let seen = seen.lock().expect("no test panicked holding the record");
    assert_eq!(*seen, [shots(&[3, 5])]);
}

/// An adapter's executor that records the shots of each call it passes
/// down.
struct SeesShots<E> {
    executor: E,
    seen: Arc<Mutex<Vec<ShotSet>>>,
}

impl<I, E: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for SeesShots<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<E::Reply, Rejected> {
        let mut seen = self
            .seen
            .lock()
            .expect("no test panicked holding the record");
        seen.push(batch.shots.clone());
        drop(seen);
        self.executor.call(batch)
    }
}

impl<E: Close> Close for SeesShots<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// An adapter's executor that counts its closes, and closes the executor
/// downstream of it.
struct CountsCloses<E> {
    executor: E,
    closes: Arc<AtomicUsize>,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for CountsCloses<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        self.executor.call(batch)
    }
}

impl<E: Close> Close for CountsCloses<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.closes.fetch_add(1, Ordering::SeqCst);
        self.executor.close()
    }
}

/// A debugger written by hand whose executor counts its closes, upstream of
/// an executor that ran two shots: closing the executor they compose closes
/// the debugger's executor once, for both shots.
#[test]
fn closing_the_executor_composed_with_an_adapter_written_by_hand_closes_its_executor() {
    let closes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&closes);
    let debugger = map_executor::<InstructionCall, _>(move |executor| CountsCloses {
        executor,
        closes: counted,
    });
    let executor = compose(debugger, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("a debugger fits any executor");
    run_and_close(executor, &[3, 5], &flip_and_toss(&bits_set())[0]);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

/// The executor composed with a debugger written by hand, closed twice, as
/// a careless caller might: the debugger's executor is closed once.
#[test]
fn an_executor_composed_with_an_adapter_written_by_hand_closes_it_once() {
    let closes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&closes);
    let debugger = map_executor::<InstructionCall, _>(move |executor| CountsCloses {
        executor,
        closes: counted,
    });
    let mut executor = compose(debugger, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("a debugger fits any executor");
    executor.close();
    executor.close();
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

/// The executor composed with a debugger written by hand whose executor
/// panics at close, closed again once the panic reached the caller: the
/// debugger's executor is closed once, the second close leaving it be.
#[test]
fn a_close_of_an_adapters_executor_that_panicked_is_not_run_again() {
    let closes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&closes);
    let debugger = map_executor::<InstructionCall, _>(move |executor| CountsCloses {
        executor: PanicsOnClose(executor),
        closes: counted,
    });
    let mut executor = compose(debugger, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("a debugger fits any executor");
    catch_unwind(AssertUnwindSafe(|| executor.close())).expect_err("the close panics");
    executor.close();
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

/// An adapter's executor that reports, at each close, the closes it has
/// seen, so that every close of it reports something.
struct ReportsItsCloses<E> {
    executor: E,
    closes: u32,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for ReportsItsCloses<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        self.executor.call(batch)
    }
}

impl<E: Close> Close for ReportsItsCloses<E> {
    type Report = u32;

    fn close(&mut self) -> u32 {
        self.executor.close();
        self.closes += 1;
        self.closes
    }
}

/// The executor composed with a debugger written by hand whose executor
/// reports something at each close, closed twice, as a careless caller
/// might: the first close reports the debugger's report, and the second
/// reports nothing, an empty report.
#[test]
fn a_second_close_of_an_executor_composed_with_an_adapter_written_by_hand_reports_nothing() {
    let debugger = map_executor::<InstructionCall, _>(|executor| ReportsItsCloses {
        executor,
        closes: 0,
    });
    let mut executor = compose(debugger, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("a debugger fits any executor");
    assert_eq!(executor.close(), 1, "precondition: the first close reports");
    assert_eq!(executor.close(), 0);
}

/// A debugger written by hand that passes each call down, upstream of a
/// device whose states fail to power down, recording in `freed` the shots
/// it frees, once it ran shots 3 and 5.
fn ran_shots_that_power_down_badly(
    freed: &Freed,
) -> impl Executor<InstructionBatch<DeviceOp>> + Close<Report = ExecutorReport<()>> {
    let device = executor_fn(promise(), powering_down_badly(freed, None), runs).shots(OnePerShot);
    let debugger = map_executor::<DeviceOp, _>(|below| below);
    let mut executor = compose(debugger, device).expect("a debugger fits any executor");
    let reply = executor
        .call(InstructionBatchRef {
            shots: &shots(&[3, 5]),
            instructions: &[DeviceOp::Flip],
        })
        .expect("an open executor runs the call");
    wait(reply, &mut Vec::new()).expect("the program runs");
    executor
}

/// A debugger written by hand, upstream of a device whose states fail to
/// power down, which ran shots 3 and 5: closing the executor they compose
/// frees both states, then raises the first one's panic, shot 3's, rather
/// than abort the process.
#[test]
fn closing_frees_each_state_below_an_adapter_written_by_hand_then_raises_the_first_drops_panic() {
    let freed = Freed::default();
    let mut executor = ran_shots_that_power_down_badly(&freed);
    let panicked =
        catch_unwind(AssertUnwindSafe(|| executor.close())).expect_err("the states' drops panic");
    assert_eq!(panicked.downcast_ref(), Some(&PowerDownFailed(3)));
    assert_eq!(freed_so_far(&freed), [3, 5]);
}

/// A debugger written by hand, upstream of a device whose states fail to
/// power down, which ran shots 3 and 5, dropped unclosed: its drop frees
/// both states and raises neither panic, which only a close reports.
#[test]
fn dropping_frees_each_state_below_an_adapter_written_by_hand_raising_no_panic_of_their_drops() {
    let freed = Freed::default();
    let executor = ran_shots_that_power_down_badly(&freed);
    catch_unwind(AssertUnwindSafe(|| drop(executor)))
        .expect("a drop raises no panic of the states' drops");
    assert_eq!(freed_so_far(&freed), [3, 5]);
}

/// An adapter written by hand whose executor counts the instructions it
/// passes down, and reports that count beside the report of the executor
/// downstream of it, as a profiler does.
struct CountInstructions;

impl Adapter for CountInstructions {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for CountInstructions {
    type Output = CountedInstructions<E>;

    fn apply(self, downstream: E) -> CountedInstructions<E> {
        CountedInstructions {
            executor: downstream,
            instructions: 0,
        }
    }
}

/// [`CountInstructions`] upstream of an executor.
struct CountedInstructions<E> {
    executor: E,
    instructions: u64,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for CountedInstructions<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        self.instructions += batch.instructions.len() as u64;
        self.executor.call(batch)
    }
}

impl<E: Close> Close for CountedInstructions<E> {
    type Report = (u64, E::Report);

    fn close(&mut self) -> Self::Report {
        (
            std::mem::take(&mut self.instructions),
            self.executor.close(),
        )
    }
}

/// A program of two batches on shots 0 and 3, through a profiler written by
/// hand that counts the instructions it passes down: the run's report is
/// the profiler's, its count of the program's instructions (each batch
/// counted once, for both shots) beside the report the executor gives
/// alone.
#[test]
fn a_run_reports_what_the_executor_of_an_adapter_written_by_hand_reports() {
    let program = || Program::flip_and_toss(bits_set());
    let instructions: usize = flip_and_toss(&bits_set()).iter().map(Vec::len).sum();
    let driver = || Driver::new(program(), shots(&[0, 3]));
    let doubles = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let (_, alone) = (driver() >> doubles())
        .expect("the bits run the program")
        .wait();
    let (_, (counted, below)) = (driver() >> CountInstructions >> doubles())
        .expect("it passes what the bits run")
        .wait();
    assert_eq!(counted, instructions as u64);
    assert_eq!(below, alone);
}

/// An adapter composed onto an executor that has run shots applies to the
/// shots it holds, which keep their state: a bit flipped before the
/// adapter came reads 1 through it, and the adapter sees the call.
#[test]
fn an_adapter_composed_onto_a_used_executor_applies_to_the_shots_it_holds() {
    let set = bits_set();
    let mut executor = TestDoubleExecutor::with_model(Bits::default(), 7);
    let flip = [call(&set, "x", &[Operand::Block(0)])];
    let reply = executor
        .call(InstructionBatchRef {
            shots: &shots(&[0]),
            instructions: &flip,
        })
        .expect("the bits run a flip");
    wait(reply, &mut Vec::new()).expect("the flip ends well");
    let log = Log::default();
    let logged = compose(logger("debugger", &log), executor).expect("a logger fits");
    let read = [call(&set, "measure", &[Operand::Block(0), Operand::Tag(0)])];
    let outcomes = run_and_close(logged, &[0], &read);
    assert_eq!(values(&outcomes), [(0, 0, Value::One)], "the flip held");
    assert_eq!(entries(&log), ["debugger 0"]);
}

/// The last executor refuses the first shot a batch calls, downstream of
/// an adapter that passes calls down: the batch is refused, with the
/// executor's reason.
#[test]
fn the_last_executors_refusal_of_the_first_shot_refuses_the_call_through_an_adapter() {
    let doubles = TestDoubleModel {
        refusing: Some(1),
        ..TestDoubleModel::default()
    };
    let passing = map_executor::<bool, _>(|below| below);
    let mut executor =
        compose(passing, TestDoubleExecutor::with_model(doubles, 7)).expect("it passes bools");
    let refused = executor
        .call(InstructionBatchRef {
            shots: &shots(&[1, 0]),
            instructions: &[true],
        })
        .err();
    assert!(
        matches!(&refused, Some(Rejected::Unavailable(why)) if why == "busy"),
        "{refused:?}"
    );
}

/// Shot 2's state, the last of a batch on shots 4, 9 and 2, cannot be
/// made at its first call, downstream of an adapter that passes calls
/// down: the call is refused as `Unavailable`. Refused, it ran nothing, so
/// the same batch, sent again once the memory is free, runs.
#[test]
fn a_state_that_cannot_be_made_refuses_the_call_through_an_adapter() {
    let doubles = TestDoubleModel {
        unavailable_once: std::cell::Cell::new(Some(2)),
        ..TestDoubleModel::default()
    };
    let passing = map_executor::<bool, _>(|below| below);
    let mut executor =
        compose(passing, TestDoubleExecutor::with_model(doubles, 7)).expect("it passes bools");
    let all = shots(&[4, 9, 2]);
    let refused = executor
        .call(InstructionBatchRef {
            shots: &all,
            instructions: &[true],
        })
        .err();
    assert!(
        matches!(&refused, Some(Rejected::Unavailable(_))),
        "{refused:?}"
    );
    let reply = executor
        .call(InstructionBatchRef {
            shots: &all,
            instructions: &[true],
        })
        .expect("shot 2's memory is free now, and the refusal failed nothing");
    wait(reply, &mut Vec::new()).expect("the batch runs when sent again");
}

/// A batch on shots 7 and 3 whose shot 7 fails after shot 3 read and
/// ended: through a passing adapter, the reply hands over what the executor
/// alone hands over, shot 3's read included, and ends as it ends.
#[test]
fn a_late_failure_ends_a_batch_through_an_adapter_as_without() {
    /// Shot 3 reads and ends, then shot 7 fails; what the reply gives.
    fn end<R: Reply<Item = ShotOutcome>>(
        reply: R,
        seen: &RefCell<Seen>,
    ) -> (Vec<ShotOutcome>, Result<(), String>) {
        let pending = |j| {
            seen.borrow_mut()
                .replies
                .get_mut(&j)
                .expect("deferred")
                .remove(0)
        };
        let shot_3 = pending(3);
        shot_3.push(ShotOutcome::new(3, 2, Value::One));
        shot_3.done();
        pending(7).fail("lost".into());
        let mut outcomes = Vec::new();
        let end = wait(reply, &mut outcomes);
        (outcomes, end)
    }
    let scene = |adapted: bool| {
        let doubles = TestDoubleModel {
            deferred: vec![7, 3],
            ..TestDoubleModel::default()
        };
        let seen = Rc::clone(&doubles.seen);
        let executor = TestDoubleExecutor::with_model(doubles, 7);
        let both = shots(&[7, 3]);
        let batch = InstructionBatchRef {
            shots: &both,
            instructions: &[true],
        };
        if adapted {
            let passing = map_executor::<bool, _>(|below| below);
            let mut executor = compose(passing, executor).expect("it passes bools");
            end(executor.call(batch).expect("shots 7 and 3 run"), &seen)
        } else {
            let mut executor = executor;
            end(executor.call(batch).expect("shots 7 and 3 run"), &seen)
        }
    };
    assert_eq!(scene(true), scene(false));
}

/// A passing adapter upstream of an executor whose shot 1's batch fails:
/// the executor they compose refuses every later call, on shot 1, on shot
/// 2 and on both, as the executor downstream does, by the conformance
/// kit's check.
#[test]
fn a_failure_downstream_is_final_through_an_adapter() {
    let failing = TestDoubleExecutor::with_model(
        TestDoubleModel {
            failing: Some(1),
            ..TestDoubleModel::default()
        },
        7,
    );
    let passing = map_executor::<bool, _>(|below| below);
    let mut executor = compose(passing, failing).expect("it passes bools");
    let (one, two, both) = (shots(&[1]), shots(&[2]), shots(&[1, 2]));
    conformance::refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &one,
            instructions: &[true],
        },
        &[&one, &two, &both].map(|shots| InstructionBatchRef {
            shots,
            instructions: &[true],
        }),
    );
}

/// An adapter's executor that answers each batch itself and never calls
/// the executor downstream of it, which it holds: a batch with a `false`
/// fails its reply, and an empty batch panics, a bug of the adapter's.
struct Answering<E> {
    _downstream: E,
}

impl<E> Executor<InstructionBatch<bool>> for Answering<E> {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        assert!(!batch.instructions.is_empty(), "the adapter has a bug");
        if batch.instructions.contains(&false) {
            let lost = "the adapter lost the batch".to_owned();
            return Ok(ImmediateReply::failed(Vec::new(), lost));
        }
        Ok(ImmediateReply::done(Vec::new()))
    }
}

impl<E> Close for Answering<E> {
    type Report = ();

    fn close(&mut self) {}
}

/// The executor of an [`Answering`] adapter written by hand, over test
/// doubles that never fail.
fn answering() -> impl Executor<InstructionBatch<bool>> + Close {
    let answering = map_executor::<bool, _>(|downstream| Answering {
        _downstream: downstream,
    });
    let doubles = TestDoubleExecutor::with_model(TestDoubleModel::default(), 7);
    compose(answering, doubles).expect("it takes bools")
}

/// An adapter written by hand whose executor answers each call itself,
/// closed: the executor they compose refuses every call, by the
/// conformance kit's check, though the adapter's executor would answer it.
#[test]
fn a_closed_adapter_written_by_hand_refuses_every_call() {
    let mut executor = answering();
    conformance::rejects_calls_once_closed::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &shots(&[1]),
            instructions: &[true],
        },
    );
}

/// An adapter written by hand whose executor fails the reply of shot 1's
/// batch, though the executor downstream never fails: the executor they
/// compose refuses every later call, on shot 1, on shot 2 and on both, by
/// the conformance kit's check, though the adapter's executor would answer
/// them.
#[test]
fn a_failure_of_an_adapter_written_by_hand_is_final() {
    let mut executor = answering();
    let (one, two, both) = (shots(&[1]), shots(&[2]), shots(&[1, 2]));
    conformance::refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &one,
            instructions: &[false],
        },
        &[&one, &two, &both].map(|shots| InstructionBatchRef {
            shots,
            instructions: &[true],
        }),
    );
}

/// An adapter written by hand whose executor panics at a call, a bug: the
/// panic reaches the caller, and the executor they compose refuses every
/// later call, since the panic may have left the adapter part-way.
#[test]
fn an_adapter_written_by_hand_that_panics_fails_the_executor_they_compose() {
    let mut executor = answering();
    let one = shots(&[1]);
    catch_unwind(AssertUnwindSafe(|| run(&mut executor, &one, &[])))
        .expect_err("the panic reaches the caller");
    let later = run(&mut executor, &one, &[true]).expect_err("the adapter failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// An adapter's executor that answers a batch of one `false` itself, as a
/// cache answers what it holds, and passes every other batch down.
struct Caching<E> {
    executor: E,
}

impl<E: Executor<InstructionBatch<bool>>> Executor<InstructionBatch<bool>> for Caching<E> {
    type Reply = CachedOr<E::Reply>;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
        if batch.instructions == [false] {
            return Ok(CachedOr::Cached(ImmediateReply::done(Vec::new())));
        }
        self.executor.call(batch).map(CachedOr::Below)
    }
}

impl<E: Close> Close for Caching<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// The reply of a [`Caching`] executor: its own, or that of the executor
/// downstream of it.
enum CachedOr<R> {
    Cached(ImmediateReply<ShotOutcome>),
    Below(R),
}

impl<R: Reply<Item = ShotOutcome>> Reply for CachedOr<R> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        match self {
            CachedOr::Cached(reply) => reply.poll_events(out, waker),
            CachedOr::Below(reply) => reply.poll_events(out, waker),
        }
    }
}

/// A cache written by hand, upstream of an executor whose shot 1 failed in
/// a call whose reply did not show it yet: a call on shot 1, which the
/// cache passes down, is refused for that failure, and the executor they
/// compose refuses every later call, one the cache would answer included,
/// as an `adapter_fn`'s does.
#[test]
fn an_earlier_failure_downstream_fails_an_adapter_written_by_hand() {
    let failed = TestDoubleExecutor::with_model(
        TestDoubleModel {
            failed_unseen: Some(1),
            ..TestDoubleModel::default()
        },
        7,
    );
    let cache = map_executor::<bool, _>(|executor| Caching { executor });
    let mut executor = compose(cache, failed).expect("it takes bools");
    let refused = run(&mut executor, &shots(&[1]), &[true]).expect_err("shot 1 failed");
    assert!(
        matches!(refused, Rejected::EarlierFailure(_)),
        "{refused:?}"
    );
    let later = run(&mut executor, &shots(&[2]), &[false]).expect_err("the executor failed");
    assert!(
        matches!(&later, Rejected::EarlierFailure(why) if why.contains("lost")),
        "{later:?}"
    );
}

/// A cache written by hand, upstream of an executor whose shot 1's batches
/// fail as they are called: shot 1's reply, dropped unpolled, hides the
/// failure from the caller; the next call the cache passes down is refused
/// for it, and the executor they compose refuses every later call, one the
/// cache would answer included.
#[test]
fn a_failure_downstream_in_a_dropped_reply_fails_an_adapter_written_by_hand() {
    let failing = TestDoubleExecutor::with_model(
        TestDoubleModel {
            failing: Some(1),
            ..TestDoubleModel::default()
        },
        7,
    );
    let cache = map_executor::<bool, _>(|executor| Caching { executor });
    let mut executor = compose(cache, failing).expect("it takes bools");
    let dropped = executor
        .call(InstructionBatchRef {
            shots: &shots(&[1]),
            instructions: &[true],
        })
        .expect("shot 1's batch is accepted, and fails as it runs");
    drop(dropped);
    let refused = run(&mut executor, &shots(&[2]), &[true]).expect_err("shot 1 failed");
    assert!(
        matches!(refused, Rejected::EarlierFailure(_)),
        "{refused:?}"
    );
    let later = run(&mut executor, &shots(&[2]), &[false]).expect_err("the executor failed");
    assert!(matches!(later, Rejected::EarlierFailure(_)), "{later:?}");
}

/// An adapter's executor that refuses a batch with a `false` for its own
/// fault, an instruction it does not take, and passes every other batch
/// down.
struct RefusesFalse<E> {
    executor: E,
}

impl<E: Executor<InstructionBatch<bool>>> Executor<InstructionBatch<bool>> for RefusesFalse<E> {
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<E::Reply, Rejected> {
        if let Some(at) = batch.instructions.iter().position(|bit| !bit) {
            let reason = "not a read".to_owned();
            return Err(Rejected::Instruction { at, reason });
        }
        self.executor.call(batch)
    }
}

impl<E: Close> Close for RefusesFalse<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.executor.close()
    }
}

/// An adapter written by hand whose executor refuses a call for its own
/// fault: the executor they compose takes the next call, by the
/// conformance kit's check.
#[test]
fn an_adapter_written_by_hand_accepts_calls_after_its_refusal() {
    let refusing = map_executor::<bool, _>(|executor| RefusesFalse { executor });
    let doubles = TestDoubleExecutor::with_model(TestDoubleModel::default(), 7);
    let mut executor = compose(refusing, doubles).expect("it takes bools");
    let (one, two) = (shots(&[4]), shots(&[9, 2]));
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &one,
            instructions: &[true, false],
        },
        InstructionBatchRef {
            shots: &two,
            instructions: &[true],
        },
    );
}

/// A passing adapter upstream of an executor that refuses a call for its
/// own fault: the executor they compose takes the next call, as the
/// executor downstream does, by the conformance kit's check.
#[test]
fn a_refusal_downstream_for_the_calls_fault_leaves_an_adapter_open() {
    let doubles = TestDoubleExecutor::with_model(
        TestDoubleModel {
            refuses_false: true,
            ..TestDoubleModel::default()
        },
        7,
    );
    let passing = map_executor::<bool, _>(|below| below);
    let mut executor = compose(passing, doubles).expect("it passes bools");
    let (one, two) = (shots(&[4]), shots(&[9, 2]));
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &one,
            instructions: &[true, false],
        },
        InstructionBatchRef {
            shots: &two,
            instructions: &[true],
        },
    );
}

/// A program of two batches on shots 0 and 3, through a passing adapter:
/// the executor downstream counts the program's two calls, as it does
/// without the adapter.
#[test]
fn the_executor_downstream_of_an_adapter_counts_the_calls_made_without_it() {
    let driver = || Driver::new(Program::flip_and_toss(bits_set()), shots(&[0, 3]));
    let doubles = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let (_, alone) = (driver() >> doubles())
        .expect("the bits run the program")
        .wait();
    let passing = map_executor::<InstructionCall, _>(|below| below);
    let (_, through) = (driver() >> passing >> doubles())
        .expect("it passes what the bits run")
        .wait();
    assert_eq!(alone.calls, 2, "the program has two batches");
    assert_eq!(through.calls, alone.calls);
}

/// A chain of two faulty adapters upstream of an executor: a code that sends an
/// instruction the executor lacks, upstream of a noise model that draws with
/// the executor's id. Both are reported at once, the upstream one first, each
/// named.
#[test]
fn a_chain_reports_every_fault_at_once_upstream_first() {
    let sends_h = bits_set().with("h", [Block]);
    let noise =
        map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
    let Err(faults) = compose(
        compose(Relabel::new(sends_h, false), noise),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("both adapters are faulty");
    };
    let missing = InstructionMismatch::Missing {
        mnemonic: "h".to_owned(),
    };
    assert_eq!(faults.0.len(), 2, "{faults}");
    assert!(faults.0[0].component.ends_with("Relabel"), "{faults}");
    assert_eq!(faults.0[0].error, BuildError::Instructions(vec![missing]));
    assert!(faults.0[1].component.contains("MapExecutor"), "{faults}");
    assert_eq!(
        faults.0[1].error,
        BuildError::RngInUse(ComponentId::LAST_EXECUTOR)
    );
}

/// Two noise models that draw with the same id, each fine against the
/// executor alone: the upstream one is refused, since the downstream one
/// draws with it already.
#[test]
fn adapters_drawing_with_the_same_id_are_refused() {
    let id = ComponentId::named("readout");
    let noise = || map_executor::<InstructionCall, _>(|below| below).drawing(id);
    compose(noise(), TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("one noise model fits");
    let upstream = map_executor::<InstructionCall, _>(|below| below).drawing(id);
    let Err(faults) = compose(
        compose(upstream, noise()),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("the two draw the same numbers");
    };
    assert_eq!(faults.0.len(), 1);
    assert_eq!(faults.0[0].error, BuildError::RngInUse(id));
}

/// Two noise models that draw with the same id, with a code between them
/// that states its offer whole, a logical device that lists no draws: the
/// upstream one is still refused, since the ids drawn with below an
/// adapter are kept whatever it offers.
#[test]
fn adapters_drawing_with_the_same_id_are_refused_through_one_that_states_its_offer_whole() {
    let id = ComponentId::named("readout");
    let upstream = map_executor::<InstructionCall, _>(|below| below).drawing(id);
    let downstream = map_executor::<InstructionCall, _>(|below| below).drawing(id);
    let Err(faults) = compose(
        compose(compose(upstream, StatesItsOfferFromScratch), downstream),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("the two draw the same numbers");
    };
    assert_eq!(faults.0.len(), 1);
    assert_eq!(faults.0[0].error, BuildError::RngInUse(id));
}

/// A noise model that writes into its offer the ids of a stream of its own
/// and of crosstalk, leaving out the readout error below it: it still
/// offers the ids drawn with below it and its own, the last executor's, the
/// readout error's and its own, since `compose` derives them.
#[test]
fn an_adapter_writing_other_ids_into_its_offer_offers_those_below_and_its_own() {
    struct WritesOtherIds;
    impl Adapter for WritesOtherIds {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;

        fn draws(&self) -> Option<ComponentId> {
            Some(NOISE)
        }

        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            boundary.offered_upstream().rng_ids = (RngIds::last_executor())
                .with(ComponentId::named("crosstalk"))
                .expect("the offer holds only the last executor's id");
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for WritesOtherIds {
        type Output = E;

        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    const NOISE: ComponentId = ComponentId::named("noise");
    let chain = compose(
        compose(WritesOtherIds, ReadoutError::default()),
        TestDoubleExecutor::with_model(Bits::default(), 7),
    )
    .expect("each component draws with an id of its own");
    assert_eq!(
        chain.capabilities().rng_ids.as_slice(),
        [ComponentId::LAST_EXECUTOR, READOUT, NOISE]
    );
}

/// A noise model that, before each batch, draws for each of its shots
/// whether to put an error on it, an error of probability ½ that never
/// changes the batch here, so that the bits' outcomes stay those
/// downstream. Shot `j` draws from the seed of `seeds.shot(j)`, `seeds`
/// its own, which it keeps when built ([`AdapterBoundary::drawing_seeds`]).
#[derive(Default)]
struct Noise {
    seeds: Option<AdapterSeeds>,
}

impl Adapter for Noise {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn draws(&self) -> Option<ComponentId> {
        Some(ComponentId::named("noise"))
    }

    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        Self {
            seeds: boundary.drawing_seeds(),
        }
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Noise {
    type Output = Noisy<E>;

    fn apply(self, downstream: E) -> Noisy<E> {
        Noisy {
            seeds: self
                .seeds
                .expect("a chain built has seeds for an adapter that draws"),
            rngs: BTreeMap::new(),
            inner: downstream,
        }
    }
}

struct Noisy<E> {
    seeds: AdapterSeeds,
    /// Each shot's random number generator, as its draws left it.
    rngs: BTreeMap<ShotId, u64>,
    inner: E,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for Noisy<E>
{
    type Reply = E::Reply;

    /// Draws for each shot once the batch is accepted below, so that a
    /// refused batch draws nothing.
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        let reply = self.inner.call(batch)?;
        let seeds = self.seeds;
        for &j in batch.shots {
            let rng = self.rngs.entry(j).or_insert_with(|| seeds.shot(j).seed());
            *rng = shot_seed(*rng, 0);
        }
        Ok(reply)
    }
}

impl<E: Close> Close for Noisy<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// The id [`ReadoutError`] draws with.
const READOUT: ComponentId = ComponentId::named("readout error");

/// A readout error written by hand: each readout of shot `j` is flipped
/// when the next draw of the shot's stream is odd, a stream seeded from the
/// seed of `seeds.shot(j)`, `seeds` its own, which it keeps when built
/// ([`AdapterBoundary::drawing_seeds`]).
#[derive(Default)]
struct ReadoutError {
    seeds: Option<AdapterSeeds>,
}

impl Adapter for ReadoutError {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn draws(&self) -> Option<ComponentId> {
        Some(READOUT)
    }

    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        Self {
            seeds: boundary.drawing_seeds(),
        }
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for ReadoutError {
    type Output = ReadoutErrors<E>;

    fn apply(self, downstream: E) -> ReadoutErrors<E> {
        ReadoutErrors {
            seeds: self
                .seeds
                .expect("a chain built has seeds for an adapter that draws"),
            streams: BTreeMap::new(),
            inner: downstream,
        }
    }
}

/// The executor of a [`ReadoutError`].
struct ReadoutErrors<E> {
    seeds: AdapterSeeds,
    /// Each shot's stream, as its draws left it.
    streams: BTreeMap<ShotId, u64>,
    inner: E,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for ReadoutErrors<E>
{
    type Reply = ImmediateReply<ShotOutcome>;

    /// Waits for the reply below, which the tests' executors give whole
    /// when called, then flips its readouts.
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        let mut outcomes = Vec::new();
        if let Err(reason) = wait(self.inner.call(batch)?, &mut outcomes) {
            return Ok(ImmediateReply::failed(outcomes, reason));
        }
        for outcome in &mut outcomes {
            let j = outcome.shot;
            let seeds = self.seeds;
            let stream = (self.streams.entry(j)).or_insert_with(|| seeds.shot(j).seed());
            *stream = shot_seed(*stream, 0);
            if *stream & 1 == 1 {
                outcome.value = match outcome.value {
                    Value::Zero => Value::One,
                    Value::One => Value::Zero,
                    error @ Value::Error(_) => error,
                };
            }
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

impl<E: Close> Close for ReadoutErrors<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// A readout error written by hand over bits whose readouts the program
/// fixes (a flip then a read, three times, no coin), the run's seed given
/// to the bits only: two runs from one seed read the same, the readout
/// error's draws included, and a run from another seed reads otherwise,
/// so that the one seed replays the whole chain.
#[test]
fn an_adapter_written_by_hand_replays_its_draws_from_the_runs_seed() {
    let flip_and_read = flip_and_toss(&bits_set())[0].clone();
    let run_from = |seed: u64, readout_error: bool| {
        let mut program = Program::flip_and_toss(bits_set());
        program.batches = vec![flip_and_read.clone(); 3];
        let driver = Driver::new(program, shots(&[0, 1, 2, 3, 4, 5, 6, 7]));
        let bits = TestDoubleExecutor::with_model(Bits::default(), seed);
        let (outcomes, _) = if readout_error {
            (driver >> ReadoutError::default() >> bits)
                .expect("the readout error draws with its own id")
                .wait()
        } else {
            (driver >> bits).expect("the bits run the program").wait()
        };
        values(&outcomes)
    };
    let first = run_from(7, true);
    assert_eq!(
        run_from(7, false),
        run_from(8, false),
        "the bits read what the program fixes"
    );
    assert_ne!(first, run_from(7, false), "the readout error flips reads");
    assert_eq!(run_from(7, true), first);
    assert_ne!(run_from(8, true), first);
}

/// A readout error of ½ written by hand, over bits that toss a fair coin
/// and read it, in 64 shots: about half the readouts are flipped, and half
/// read One, as the readout error draws from its own seeds, not the
/// coins'. Drawing from the last executor's, it would flip exactly the
/// coins that come up One, and every readout would read Zero.
#[test]
fn an_adapter_written_by_hand_draws_otherwise_than_the_last_executor() {
    let toss = flip_and_toss(&bits_set())[1].clone();
    let run = |readout_error: bool| {
        let mut program = Program::flip_and_toss(bits_set());
        program.batches = vec![toss.clone()];
        let driver = Driver::new(
            program,
            ShotSet::range(0..=63).expect("the range holds shots"),
        );
        let bits = TestDoubleExecutor::with_model(Bits::default(), 7);
        let (outcomes, _) = if readout_error {
            (driver >> ReadoutError::default() >> bits)
                .expect("the readout error draws with its own id")
                .wait()
        } else {
            (driver >> bits).expect("the bits run the program").wait()
        };
        outcomes
    };
    let (coins, read) = (run(false), run(true));
    let ones = read.iter().filter(|o| o.value == Value::One).count();
    let flipped = (coins.iter().zip(&read))
        .filter(|(coin, read)| coin.value != read.value)
        .count();
    assert!((16..=48).contains(&ones), "{ones} of 64 read One");
    assert!((16..=48).contains(&flipped), "{flipped} of 64 flipped");
}

/// A driver that needs each shot's log probability, upstream of a debugger
/// upstream of an executor that states its own: it runs, and each shot's is
/// the last executor's (a coin, ln ½), passed on by the debugger.
#[test]
fn a_run_reports_the_last_executors_log_probability_through_its_adapters() {
    let downstream = Bits {
        probabilities: true,
        ..Bits::default()
    };
    let mut program = Program::flip_and_toss(bits_set());
    program.needs = program.needs.with_probabilities(true);
    let log = Log::default();
    let (_, report) = (Driver::new(program, shots(&[0, 3]))
        >> logger("debugger", &log)
        >> TestDoubleExecutor::with_model(downstream, 7))
    .expect("the chain states log probabilities")
    .wait();
    for j in [0, 3] {
        let log_p = report.log_p.get(j).expect("each shot's is known");
        assert!((log_p - 0.5_f64.ln()).abs() < 1e-12, "shot {j}: {log_p}");
    }
}

/// A driver that needs each shot's log probability is refused upstream of a
/// noise model, an adapter that draws, though the executor downstream of it
/// states its own: the log probability that executor reports does not hold
/// the noise model's draws.
#[test]
fn a_driver_needing_probabilities_is_refused_upstream_of_an_adapter_that_draws() {
    let downstream = Bits {
        probabilities: true,
        ..Bits::default()
    };
    let mut program = Program::flip_and_toss(bits_set());
    program.needs = program.needs.with_probabilities(true);
    let refused = (Driver::new(program, shots(&[0]))
        >> Noise::default()
        >> TestDoubleExecutor::with_model(downstream, 7))
    .err();
    let errors: Option<Vec<_>> = refused.map(|f| f.0.into_iter().map(|f| f.error).collect());
    let Some([BuildError::Unmet(unmet)]) = errors.as_deref() else {
        panic!("{errors:?}")
    };
    assert_eq!(unmet.need, Need::Probabilities);
}

/// A noise model written by hand that states its offer whole as what
/// downstream offers, probabilities included, in place of the offer
/// `compose` gave it, which has none.
struct NoiseOffersDownstreamsWhole;

impl Adapter for NoiseOffersDownstreamsWhole {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn draws(&self) -> Option<ComponentId> {
        Some(ComponentId::named("noise"))
    }

    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        *boundary.offered_upstream() = boundary.downstream().clone();
        self
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E>
    for NoiseOffersDownstreamsWhole
{
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// A driver that needs each shot's log probability is refused upstream of
/// a noise model that states its offer whole, as what downstream offers,
/// though the executor downstream states its own: an adapter that draws
/// offers no probabilities, whatever it offers.
#[test]
fn a_driver_needing_probabilities_is_refused_upstream_of_an_adapter_that_draws_and_states_its_offer_whole()
 {
    let downstream = Bits {
        probabilities: true,
        ..Bits::default()
    };
    let mut program = Program::flip_and_toss(bits_set());
    program.needs = program.needs.with_probabilities(true);
    let refused = (Driver::new(program, shots(&[0]))
        >> NoiseOffersDownstreamsWhole
        >> TestDoubleExecutor::with_model(downstream, 7))
    .err();
    let errors: Option<Vec<_>> = refused.map(|f| f.0.into_iter().map(|f| f.error).collect());
    let Some([BuildError::Unmet(unmet)]) = errors.as_deref() else {
        panic!("{errors:?}")
    };
    assert_eq!(unmet.need, Need::Probabilities);
}

/// A driver that needs each shot's log probability, upstream of a code
/// that draws nothing and claims log probabilities, upstream of a noise
/// model over an executor that states its own: the driver is refused,
/// since an adapter may turn probabilities off but never on, and the
/// executor's log probability leaves out the noise model's draws.
#[test]
fn a_driver_needing_probabilities_is_refused_through_an_adapter_offering_them_above_one_that_draws()
{
    struct ClaimsProbabilities;
    impl Adapter for ClaimsProbabilities {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;

        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            boundary.offered_upstream().probabilities = true;
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for ClaimsProbabilities {
        type Output = E;

        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    let downstream = Bits {
        probabilities: true,
        ..Bits::default()
    };
    let mut program = Program::flip_and_toss(bits_set());
    program.needs = program.needs.with_probabilities(true);
    let refused = (Driver::new(program, shots(&[0]))
        >> ClaimsProbabilities
        >> Noise::default()
        >> TestDoubleExecutor::with_model(downstream, 7))
    .err();
    let errors: Option<Vec<_>> = refused.map(|f| f.0.into_iter().map(|f| f.error).collect());
    let Some([BuildError::Unmet(unmet)]) = errors.as_deref() else {
        panic!("{errors:?}")
    };
    assert_eq!(unmet.need, Need::Probabilities);
}

/// A sampler that returns values only, over a simulator that states each
/// shot's log probability: it offers none, since an adapter may turn
/// probabilities off.
#[test]
fn an_adapter_turning_probabilities_off_offers_none() {
    struct TurnsProbabilitiesOff;
    impl Adapter for TurnsProbabilitiesOff {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;

        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            boundary.offered_upstream().probabilities = false;
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for TurnsProbabilitiesOff {
        type Output = E;

        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    let downstream = Bits {
        probabilities: true,
        ..Bits::default()
    };
    let executor = compose(
        TurnsProbabilitiesOff,
        TestDoubleExecutor::with_model(downstream, 7),
    )
    .expect("the sampler passes the bits' calls through");
    assert!(!executor.capabilities().probabilities);
}

/// A run through a noise model, which offers no probabilities, reports
/// none, though the executor downstream of it states its own: a log
/// probability without the noise model's draws would not be the shot's.
#[test]
#[ignore = "the report below passes up as it is, its log probabilities included; goes with \
            `.log_probability` on adapters (planned)"]
fn a_run_through_an_adapter_that_draws_reports_no_log_probability() {
    let downstream = Bits {
        probabilities: true,
        ..Bits::default()
    };
    let program = Program::flip_and_toss(bits_set());
    let (_, report) = (Driver::new(program, shots(&[0]))
        >> Noise::default()
        >> TestDoubleExecutor::with_model(downstream, 7))
    .expect("the driver needs no probabilities")
    .wait();
    assert!(report.log_p.is_empty(), "{:?}", report.log_p);
}

/// A driver whose program sends two batches on four shots, through a
/// recorder that passes every call down: the run reports the two calls
/// that the executor alone reports for the same driver.
#[test]
fn a_run_through_an_adapter_reports_the_calls_its_executor_alone_reports() {
    let log = Log::default();
    let driver = || Driver::new(Program::flip_and_toss(bits_set()), shots(&[0, 1, 2, 3]));
    let (_, alone) = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run the program")
        .wait();
    let (_, through) = (driver()
        >> logger("recorder", &log)
        >> TestDoubleExecutor::with_model(Bits::default(), 7))
    .expect("a recorder fits any executor")
    .wait();
    assert_eq!(through.calls, alone.calls);
}

/// A noise model drawing with its own id, upstream of an executor drawing
/// with the last executor's: the run gives the outcomes the last executor gives
/// alone, each shot's draws its own.
#[test]
fn a_drawing_adapter_leaves_the_last_executors_draws_as_they_are() {
    let driver = || Driver::new(Program::flip_and_toss(bits_set()), shots(&[0, 3]));
    let (alone, _) = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run the program")
        .wait();
    let (noisy, _) =
        (driver() >> Noise::default() >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the noise model draws with its own id")
            .wait();
    assert_eq!(values(&noisy), values(&alone));
}

/// An adapter that sends calls in the numbering of the downstream shot: they
/// reach it as they are.
#[test]
fn calls_in_the_downstream_numbering_reach_the_shot_as_they_are() {
    let relabel = Relabel::new(bits_set(), false);
    let watch = relabel.with(&relabel.target);
    let downstream = Bits::default();
    let received = Arc::clone(&downstream.received);
    let executor = compose(relabel, TestDoubleExecutor::with_model(downstream, 7))
        .expect("it sends the downstream set");
    run_and_close(executor, &[0], &flip_and_toss(&bits_set())[0]);
    assert_eq!(
        watch.sent.load(Ordering::SeqCst),
        received.load(Ordering::SeqCst)
    );
}

/// An adapter that sends calls in another numbering, and cannot send in
/// the shot's: each batch is renumbered on its way down, so the shot runs
/// what the adapter meant.
#[test]
fn calls_in_another_numbering_are_renumbered_on_their_way_down() {
    let relabel = Relabel::new(reversed_set(), false);
    let executor = compose(relabel, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("it sends the downstream set");
    let outcomes = run_and_close(executor, &[0], &flip_and_toss(&bits_set())[0]);
    assert_eq!(values(&outcomes), [(0, 0, Value::One)], "x, then measure");
}

/// Each batch an adapter sends in another numbering runs only its own calls
/// downstream, though a longer one came before: none of the earlier batch's
/// calls is run again.
#[test]
fn each_batch_renumbered_on_its_way_down_runs_only_its_own_calls() {
    let relabel = Relabel::new(reversed_set(), false);
    let mut executor = compose(relabel, TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("it sends the downstream set");
    let set = bits_set();
    let measure = |tag| call(&set, "measure", &[Operand::Block(0), Operand::Tag(tag)]);
    let longer = [
        call(&set, "x", &[Operand::Block(0)]),
        measure(0),
        measure(1),
    ];
    let shots = shots(&[0]);
    let mut run = |batch: &[InstructionCall]| {
        let reply = executor
            .call(InstructionBatchRef {
                shots: &shots,
                instructions: batch,
            })
            .expect("the batch is valid");
        let mut outcomes = Vec::new();
        wait(reply, &mut outcomes).expect("the shots do not fail");
        values(&outcomes)
    };
    run(&longer);
    assert_eq!(run(&[measure(2)]), [(0, 2, Value::One)]);
}

/// An adapter that accepts a coin, a flip and a read, and states nothing it
/// sends, upstream of bits that also reset, numbered before the flip: it sends
/// what it accepts, renumbered into the set downstream, so its flip flips the
/// bit and does not reset it.
#[test]
fn an_adapter_that_states_no_sends_sends_what_it_accepts() {
    let accepting = Accepting::new(bits_set());
    let downstream = Bits {
        resets: true,
        ..Bits::default()
    };
    let executor = compose(accepting, TestDoubleExecutor::with_model(downstream, 7))
        .expect("the bits run what it accepts");
    let outcomes = run_and_close(executor, &[0], &flip_and_toss(&bits_set())[0]);
    assert_eq!(values(&outcomes), [(0, 0, Value::One)], "x, then measure");
}

/// An adapter that states only the set it accepts, which has an `h` the
/// bits lack: it is refused when composed, the `h` named.
#[test]
fn an_adapter_accepting_what_the_downstream_set_lacks_is_refused_when_built() {
    let accepting = Accepting::new(bits_set().with("h", [Block]));
    let Err(faults) = compose(
        accepting,
        TestDoubleExecutor::with_model(Bits::default(), 7),
    ) else {
        panic!("the bits have no h");
    };
    let missing = InstructionMismatch::Missing {
        mnemonic: "h".to_owned(),
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::Instructions(vec![missing]));
}

/// Makes the shots of [`Bits`], described as the capabilities it holds.
struct Described(Bits, Capabilities);

impl Describe for Described {
    fn capabilities(&self) -> Capabilities {
        self.1.clone()
    }
}

impl Model for Described {
    type Instruction = InstructionCall;
    type State = BitsState;

    fn state(&self, ctx: &ShotContext) -> Result<BitsState, String> {
        self.0.state(ctx)
    }
}

/// An adapter upstream of shots that describe no instruction set, though
/// they run what it sends: it is refused when composed, since what it
/// sends cannot be checked to fit.
#[test]
fn an_adapter_upstream_of_a_component_with_no_instruction_set_is_refused_when_built() {
    let accepting = Accepting::new(bits_set());
    let unstated = Bits::default().capabilities().with_instruction_set(None);
    let Err(faults) = compose(
        accepting,
        TestDoubleExecutor::with_model(Described(Bits::default(), unstated), 7),
    ) else {
        panic!("the shots describe no set");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::NoInstructionSetDownstream);
}

/// A driver of calls by opcode that states an empty set, over shots that
/// describe none: nothing downstream numbers its calls, so the run is
/// refused, as the driver's need of instructions unmet.
#[test]
fn a_driver_stating_an_empty_set_over_shots_describing_none_is_refused_when_composed() {
    let mut program = Program::flip_and_toss(bits_set());
    let empty = InstructionSet::new("empty");
    program.needs = program.needs.with_instructions(Some(Arc::new(empty)));
    let unstated = Bits::default().capabilities().with_instruction_set(None);
    let executor = TestDoubleExecutor::with_model(Described(Bits::default(), unstated), 7);
    let Err(faults) = Driver::new(program, shots(&[0])) >> executor else {
        panic!("nothing downstream of the driver describes a set");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    let BuildError::Unmet(unmet) = &faults.0[0].error else {
        panic!("the driver's need is unmet: {faults}");
    };
    assert_eq!((unmet.need, unmet.offered), (Need::Instructions, 0));
}

/// A code that accepts the bits numbered otherwise and sends
/// [`bits_set`], reading each call it is called with by its mnemonic in
/// the set it accepts, and that states its offer whole from downstream's
/// ([`AdapterBoundary::offered_upstream`]).
struct TranslatesOffersDownstreamsWhole {
    accepts: Arc<InstructionSet>,
}

impl Adapter for TranslatesOffersDownstreamsWhole {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.accepts))
    }

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::new(bits_set()))
    }

    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        *boundary.offered_upstream() = boundary.downstream().clone();
        self
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E>
    for TranslatesOffersDownstreamsWhole
{
    type Output = Translated<E>;

    fn apply(self, downstream: E) -> Translated<E> {
        Translated {
            accepts: self.accepts,
            sends: bits_set(),
            inner: downstream,
        }
    }
}

/// The executor of a [`TranslatesOffersDownstreamsWhole`].
struct Translated<E> {
    accepts: Arc<InstructionSet>,
    sends: InstructionSet,
    inner: E,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for Translated<E>
{
    type Reply = E::Reply;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<E::Reply, Rejected> {
        self.accepts.check_batch(batch.instructions)?;
        let sent: Vec<_> = (batch.instructions.iter())
            .map(|call| {
                let mnemonic = (self.accepts.mnemonic(call.opcode)).expect("checked above");
                let opcode = self.sends.opcode(mnemonic).expect("the sets hold the same");
                InstructionCall::new(opcode, call.operands.iter().copied())
            })
            .collect();
        self.inner.call(InstructionBatchRef {
            shots: batch.shots,
            instructions: &sent,
        })
    }
}

impl<E: Close> Close for Translated<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// A driver that flips a bit and reads it in eight shots, in the numbering
/// of a code that accepts the bits with the flip before the coin, and
/// states its offer whole from downstream's: every shot reads One, since
/// the chain offers the set the code accepts, whatever its offer says.
/// Offering downstream's, the driver's flip would be numbered as the coin
/// there, which the code reads as its coin, and its shots would read
/// tosses.
#[test]
fn a_code_stating_its_offer_whole_from_downstreams_offers_the_set_it_accepts() {
    let flip_first = InstructionSet::new("flip first")
        .with("x", [Block])
        .with("coin", [Block])
        .with("measure", [Block, OperandKind::Tag]);
    let flip_first = declaring(flip_first, EVERY_BIT);
    let mut program = Program::flip_and_toss(flip_first.clone());
    program.batches.truncate(1);
    let code = TranslatesOffersDownstreamsWhole {
        accepts: Arc::new(flip_first),
    };
    let (outcomes, _) = (Driver::new(program, shots(&[0, 1, 2, 3, 4, 5, 6, 7]))
        >> code
        >> TestDoubleExecutor::with_model(Bits::default(), 7))
    .expect("the bits run what the code sends")
    .wait();
    let read: Vec<_> = values(&outcomes).into_iter().map(|(_, _, v)| v).collect();
    assert_eq!(read, [Value::One; 8], "x, then measure");
}

/// An observer that states no set and passes its calls through as they
/// are, clearing the set in its offer: it still offers the bits' set, the
/// one its calls are checked against.
#[test]
fn an_adapter_passing_its_calls_through_offers_the_set_downstream_whatever_its_offer() {
    struct ClearsItsSet;
    impl Adapter for ClearsItsSet {
        type Accepts = InstructionCall;
        type Sends = InstructionCall;

        fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
            boundary.offered_upstream().instruction_set = None;
            self
        }
    }
    impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for ClearsItsSet {
        type Output = E;

        fn apply(self, downstream: E) -> E {
            downstream
        }
    }
    let executor = compose(
        ClearsItsSet,
        TestDoubleExecutor::with_model(Bits::default(), 7),
    )
    .expect("the observer passes the bits' calls through");
    assert_eq!(
        executor.capabilities().instruction_set.as_deref(),
        Some(&bits_set())
    );
}

/// An adapter that accepts the bits numbered otherwise changes only the
/// instruction set: it keeps every other capability of the shots
/// downstream (their qubits, their limit of states, no feedback and no
/// resets), since it runs on them.
#[test]
fn an_adapter_keeps_the_capabilities_it_does_not_change() {
    let limited = Capabilities {
        states: Some(4),
        feedback: false,
        resets: false,
        ..Bits::default().capabilities()
    };
    let downstream = Described(Bits::default(), limited.clone());
    let accepting = Accepting::new(reversed_set());
    let executor = compose(accepting, TestDoubleExecutor::with_model(downstream, 7))
        .expect("the bits run the reversed set");
    let reversed = Some(Arc::new(reversed_set()));
    assert_eq!(
        executor.capabilities(),
        limited.with_instruction_set(reversed)
    );
}

/// An adapter that accepts and sends one set declaring no action, upstream
/// of bits that describe the same set, as an adapter between QEP's driver
/// and executor does: the sets share a name, so the adapter's calls are matched
/// by mnemonic, and it is composed.
#[test]
fn an_adapter_sending_the_set_downstream_without_actions_is_composed() {
    let vendors = undeclaring(bits_set(), EVERY_BIT);
    let downstream = Bits {
        undeclared: EVERY_BIT,
        ..Bits::default()
    };
    let composed = compose(
        Accepting::new(vendors),
        TestDoubleExecutor::with_model(downstream, 7),
    );
    if let Err(faults) = composed {
        panic!("one set at the adapter's boundary, matched by mnemonic: {faults}");
    }
}

/// An adapter that sends a coin, a flip and a read, each declaring what it
/// does, upstream of bits whose coin is a vendor's instruction, declaring
/// nothing, in the vendor's set of another name: what the coin does cannot
/// be checked, so the adapter is refused, the coin named.
#[test]
fn an_instruction_whose_action_the_downstream_set_leaves_undeclared_is_refused() {
    let downstream = Bits {
        undeclared: &["coin"],
        ..Bits::default()
    };
    let Err(faults) = compose(
        Accepting::new(bits_set()),
        TestDoubleExecutor::with_model(downstream, 7),
    ) else {
        panic!("the bits' coin declares nothing");
    };
    let undeclared = InstructionMismatch::UndeclaredAction {
        mnemonic: "coin".to_owned(),
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(
        faults.0[0].error,
        BuildError::Instructions(vec![undeclared])
    );
}

/// An adapter that passes calls by opcode through as they are, stating no
/// set, upstream of bits that describe none: an opcode is just a number
/// without a set, so it is refused.
#[test]
fn a_passing_adapter_of_calls_by_opcode_over_no_set_is_refused() {
    let passing = map_executor::<InstructionCall, _>(|below| below);
    let downstream = Bits {
        undescribed: true,
        ..Bits::default()
    };
    let Err(faults) = compose(passing, TestDoubleExecutor::with_model(downstream, 7)) else {
        panic!("no set is described");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::NoInstructionSetDownstream);
}

/// A typed instruction: flip the bit.
#[derive(Clone, Copy)]
struct Flip;

impl TypedInstruction for Flip {}

/// An adapter that lowers each [`Flip`] to a call of opcode 1, `x` in
/// [`bits_set`], the set it states it sends, if any; it may state a set it
/// accepts, describing its flips.
struct LowerFlips {
    sends: Option<Arc<InstructionSet>>,
    accepts: Option<Arc<InstructionSet>>,
}

impl Adapter for LowerFlips {
    type Accepts = Flip;
    type Sends = InstructionCall;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        self.sends.clone()
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        self.accepts.clone()
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for LowerFlips {
    type Output = LoweredFlips<E>;

    fn apply(self, downstream: E) -> LoweredFlips<E> {
        LoweredFlips(downstream)
    }
}

/// [`LowerFlips`] upstream of an executor.
struct LoweredFlips<E>(E);

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<Flip>>
    for LoweredFlips<E>
{
    type Reply = E::Reply;

    fn call(&mut self, batch: InstructionBatchRef<'_, Flip>) -> Result<E::Reply, Rejected> {
        let x = |_: &Flip| InstructionCall::new(Opcode::new(1), [Operand::Block(0)]);
        let calls: Vec<InstructionCall> = batch.instructions.iter().map(x).collect();
        self.0.call(InstructionBatchRef {
            shots: batch.shots,
            instructions: &calls,
        })
    }
}

impl<E: Close> Close for LoweredFlips<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.0.close()
    }
}

/// An adapter that makes calls by opcode from typed instructions, stating
/// no set, upstream of bits that describe theirs: nothing numbers its
/// calls but the adapter, so their opcodes cannot be checked, and it is
/// refused.
#[test]
fn an_adapter_that_makes_calls_by_opcode_and_states_no_set_is_refused() {
    let lower = LowerFlips {
        sends: None,
        accepts: None,
    };
    let Err(faults) = compose(lower, TestDoubleExecutor::with_model(Bits::default(), 7)) else {
        panic!("the adapter states no set");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::UnstatedSends);
}

/// The same adapter, stating the set of flips it accepts, a flip then a
/// coin, but not the set it sends: the flips it is called with are not the
/// calls it sends, so the set it accepts does not number them (taken as
/// such, its `x` at opcode 1 would reach the bits' coin), and it is refused.
#[test]
fn an_adapter_that_makes_calls_by_opcode_and_states_only_the_set_it_accepts_is_refused() {
    let flips = InstructionSet::new("flips")
        .with("x", [Block])
        .with("coin", [Block]);
    let lower = LowerFlips {
        sends: None,
        accepts: Some(Arc::new(declaring(flips, &["x", "coin"]))),
    };
    let Err(faults) = compose(lower, TestDoubleExecutor::with_model(Bits::default(), 7)) else {
        panic!("the adapter states no set it sends");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::UnstatedSends);
}

/// The same adapter, stating the set it sends, upstream of bits that
/// describe the same set: its calls are checked against the bits' set, and
/// it composes.
#[test]
fn an_adapter_that_makes_calls_by_opcode_and_states_the_set_it_sends_is_composed() {
    let lower = LowerFlips {
        sends: Some(Arc::new(bits_set())),
        accepts: None,
    };
    if let Err(faults) = compose(lower, TestDoubleExecutor::with_model(Bits::default(), 7)) {
        panic!("the adapter states the set it sends: {faults}");
    }
}

/// A decoder stating only the set it accepts, a coin among them, upstream
/// of workers whose set has no coin: what it sends is typed, so its set is
/// not matched downstream, and the executor composed describes it in place
/// of the workers'.
#[test]
fn a_decoder_stating_only_the_set_it_accepts_describes_it_whatever_the_set_downstream() {
    let decoder = DecodeCalls::accepting(bits_set());
    let executor = compose(
        decoder,
        TestDoubleExecutor::with_model(DescribedWorkers(bits_without_coin()), 7),
    )
    .expect("the decoder's downstream boundary is typed");
    assert_eq!(
        executor.capabilities().instruction_set.as_deref(),
        Some(&bits_set())
    );
}

/// A decoder stating the set it sends, a coin among them, upstream of
/// workers whose set has no coin: the set it sends is checked against the
/// workers', so it is refused, the coin named.
#[test]
fn a_decoder_sending_what_the_downstream_set_lacks_is_refused_when_built() {
    let decoder = DecodeCalls {
        sends: Some(Arc::new(bits_set())),
        accepts: Some(Arc::new(bits_set())),
    };
    let Err(faults) = compose(
        decoder,
        TestDoubleExecutor::with_model(DescribedWorkers(bits_without_coin()), 7),
    ) else {
        panic!("the workers have no coin");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(
        faults.0[0].error,
        BuildError::Instructions(vec![missing_coin()])
    );
}

/// A decoder stating the set it sends, upstream of workers that describe
/// none: what it sends cannot be checked, so it is refused.
#[test]
fn a_decoder_stating_the_set_it_sends_over_no_set_is_refused() {
    let decoder = DecodeCalls {
        sends: Some(Arc::new(bits_set())),
        accepts: Some(Arc::new(bits_set())),
    };
    let Err(faults) = compose(decoder, TestDoubleExecutor::with_model(Workers, 7)) else {
        panic!("the workers describe no set");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::NoInstructionSetDownstream);
}

/// An adapter of `bool`s that states only the set it accepts: it passes
/// the calls it is called with down, so it sends what it accepts.
struct AcceptingBools(InstructionSet);

impl Adapter for AcceptingBools {
    type Accepts = bool;
    type Sends = bool;

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::new(self.0.clone()))
    }
}

impl<E: Executor<InstructionBatch<bool>> + Close> Apply<E> for AcceptingBools {
    type Output = E;

    fn apply(self, downstream: E) -> E {
        downstream
    }
}

/// A typed adapter stating only the set it accepts, a coin among them,
/// upstream of workers whose set has no coin: it sends what it accepts, so
/// its set is checked against the workers', and it is refused, the coin
/// named.
#[test]
fn a_typed_adapter_accepting_what_the_downstream_set_lacks_is_refused_when_built() {
    let accepting = AcceptingBools(bits_set());
    let Err(faults) = compose(
        accepting,
        TestDoubleExecutor::with_model(DescribedWorkers(bits_without_coin()), 7),
    ) else {
        panic!("the workers have no coin");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(
        faults.0[0].error,
        BuildError::Instructions(vec![missing_coin()])
    );
}

/// A typed adapter stating only the set it accepts, upstream of workers
/// that describe none: a typed boundary needs no set, but one stated on its
/// upstream side cannot be checked without one downstream, so it is
/// refused.
#[test]
fn a_typed_adapter_stating_the_set_it_accepts_over_no_set_is_refused() {
    let Err(faults) = compose(
        AcceptingBools(bits_set()),
        TestDoubleExecutor::with_model(Workers, 7),
    ) else {
        panic!("the workers describe no set");
    };
    assert_eq!(faults.0.len(), 1, "{faults}");
    assert_eq!(faults.0[0].error, BuildError::NoInstructionSetDownstream);
}
