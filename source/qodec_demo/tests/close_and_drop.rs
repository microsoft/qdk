//! A Qodec chain dropped or closed with many calls in flight: every reply
//! still ends, none hangs, nothing panics, and the backend is closed once
//! when the chain is closed, and dropped once, unclosed, when it is
//! dropped. The calls are repetition3's prepares and rotations on 64
//! shots, each shot on a thread of its own below the layer and the
//! profiler, every batch held at a gate until the test, or the backend's
//! close, opens it. Each test is bounded in time, so that a hang fails.

mod common;

use std::future::IntoFuture;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Waker};

use executor_protocol::{
    Close, Executor, InstructionBatchRef, Needs, ReplyStatus, ShotSet, compose, static_driver,
};
use qodec_demo::profiler::profiler;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::stack::on_the_stack;

use common::devices::{Counted, Ends, Gate, held_coins_on_threads};
use common::waiting::{bounded, end_of};
use common::{SEED, repetition3, shot_set, stim_rz};

/// The shots of the load: many more than the machine's cores, each with a
/// thread of its own below.
const LOAD: u32 = 64;

/// Repetition3's layer over the profiler, over the held coins counted in
/// `ends`, with two calls in flight on each of `$load` shots, each on its
/// shot alone: a prepare, which each shot's thread runs, held at `hold`,
/// and a rotation queued behind it. Both go below as sends, so `call`
/// returns at once. `$then` runs, once every shot's prepare is held at
/// `hold`, on the chain and the replies, in call order. Closing the chain
/// opens `hold` once the backend's close returned, or once it has waited
/// `OPEN_AFTER` (200 ms) in it ([`Counted::opening_at_close`]): the close
/// meets every call in flight. A close that waits for those calls always
/// opens `hold` through that wait, so each test that closes the chain
/// takes at least 200 ms; the gate opens as the close returns only for a
/// close that does not wait.
macro_rules! loaded {
    ($load:expr, |$chain:ident, $replies:ident, $hold:ident, $ends:ident| $then:block) => {{
        let qodec = repetition3();
        let program = Program::parse(&qodec, &["prepare_z 0", "rotate_z 0 theta=0.7"]);
        let $hold = Gate::default();
        let $ends = Arc::new(Ends::default());
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let coins = held_coins_on_threads(stim_rz().set, SEED, $hold.clone());
        #[allow(unused_mut)]
        let backend = Counted::new(coins, &$ends).opening_at_close(&$hold);
        let mut $chain =
            compose(layer.adapter() >> profiler(), backend).expect("every boundary matches");
        let mut $replies = Vec::new();
        for shot in 0..$load {
            let shots = shot_set(&[shot]);
            for instructions in &program.batches {
                let batch = InstructionBatchRef {
                    shots: &shots,
                    instructions,
                };
                $replies.push($chain.call(batch).expect("the chain takes the call"));
            }
        }
        $hold.wait_for_arrivals($load as usize);
        $then
    }};
}

/// How the replies ended, in call order.
fn ends_of<R: executor_protocol::Reply>(replies: &mut [R]) -> Vec<ReplyStatus> {
    (replies.iter_mut())
        .map(|reply| end_of(reply, &mut Vec::new()))
        .collect()
}

/// A program dropped the chain with every shot's prepare running, held at
/// the device, and its rotation queued: once the device lets the prepares
/// go, each reply ends, cancelled, done, or failed for a cancellation
/// below it (the adapter's rule: a send cancelled below fails the
/// adapter, and the calls after it fail for that), and none panics or
/// hangs.
#[test]
fn dropped_with_calls_in_flight_every_reply_ends() {
    let ends = bounded(|| {
        loaded!(LOAD, |chain, replies, hold, ends| {
            drop(chain);
            hold.open();
            ends_of(&mut replies)
        })
    });
    assert!(
        ends.contains(&ReplyStatus::Cancelled),
        "precondition: the drop cancelled a queued call: {ends:?}"
    );
    let unended: Vec<_> = (ends.iter())
        .filter(|end| match end {
            ReplyStatus::Done | ReplyStatus::Cancelled => false,
            ReplyStatus::Failed(reason) => !reason.contains("cancelled"),
            _ => true,
        })
        .collect();
    assert_eq!(unended, Vec::<&ReplyStatus>::new());
}

/// The device alone, in the scene of the finding below: two shots, each
/// with its prepare's bottom batch running, held at the device, and its
/// rotation's queued, the device dropped. Each prepare ends done and each
/// rotation cancelled: a cancelled call fails no later call (the
/// executors' rule, Q42, decided: the protocol's `docs/executors.md`,
/// `a_cancelled_call_fails_no_later_call` and
/// `dropped_its_calls_not_yet_started_end_cancelled_in_either_poll_order`).
#[test]
fn dropped_alone_the_device_ends_prepares_done_and_rotations_cancelled() {
    let ends = bounded(|| {
        let qodec = repetition3();
        let program = Program::parse(&qodec, &["prepare_z 0", "rotate_z 0 theta=0.7"]);
        let stack = on_the_stack(&qodec, &program, &ShotSet::one(0), SEED)
            .expect("the stack runs the program");
        let batches = stack.backends[0].1.sent().to_vec();
        assert_eq!(batches.len(), 2, "a prepare's batch, then a rotation's");
        let hold = Gate::default();
        let mut device = held_coins_on_threads(stim_rz().set, SEED, hold.clone());
        let mut replies = Vec::new();
        for shot in 0..2 {
            let shots = ShotSet::one(shot);
            for instructions in &batches {
                let batch = InstructionBatchRef {
                    shots: &shots,
                    instructions,
                };
                replies.push(device.call(batch).expect("the device takes the call"));
            }
        }
        hold.wait_for_arrivals(2);
        drop(device);
        hold.open();
        ends_of(&mut replies)
    });
    let expected = [ReplyStatus::Done, ReplyStatus::Cancelled];
    assert_eq!(ends, [expected.clone(), expected].concat());
}

/// FINDING (Q68): what a program dropping the chain expects, which an
/// adapter does not give. The same scene, minimal: two shots, each with
/// its prepare running, held at the device, and its rotation queued, the
/// chain dropped.
/// Below the layer, the placed device ends each prepare done and each
/// rotation cancelled, and a cancelled call fails no later call there
/// (the executors' rule, Q42, decided: the protocol's `docs/executors.md`,
/// `a_cancelled_call_fails_no_later_call`; the device alone,
/// `dropped_alone_the_device_ends_prepares_done_and_rotations_cancelled`).
/// Above it, polled in call order, the first rotation's cancellation fails
/// the adapter, so the second shot's prepare, which ran, ends failed for
/// "an earlier call", as does its rotation: a cancellation reported as a
/// failure. The protocol documents this rule (`docs/adapters.md`); whether
/// an adapter should end its reply cancelled instead is open (Q68, which
/// contradicts the executors' rule of Q42).
#[test]
#[ignore = "finding Q68 (contradicts the executors' rule of Q42): over an adapter, a cancelled send fails every later reply in flight, those of calls that ran included"]
fn dropped_with_calls_in_flight_every_reply_ends_cancelled_or_done() {
    let ends = bounded(|| {
        loaded!(2, |chain, replies, hold, ends| {
            let _ = &ends;
            drop(chain);
            hold.open();
            ends_of(&mut replies)
        })
    });
    let expected = [ReplyStatus::Done, ReplyStatus::Cancelled];
    assert_eq!(ends, [expected.clone(), expected].concat());
}

/// The same scene: dropped, the chain drops the backend once and never
/// closes it (closing at a drop goes with the planned cancellation).
#[test]
fn dropped_with_calls_in_flight_the_backend_is_dropped_once_unclosed() {
    let (closes, drops) = bounded(|| {
        loaded!(LOAD, |chain, replies, hold, ends| {
            drop(chain);
            hold.open();
            let _ = ends_of(&mut replies);
            (ends.closes(), ends.drops())
        })
    });
    assert_eq!((closes, drops), (0, 1));
}

/// A program closed the chain with every shot's prepare held and its
/// rotation queued, the device letting them go while the backend's close
/// waits: `close` waits for every call queued, so each reply ends done.
/// Below, the placed device waits twice on this path: once for every
/// shot's calls before it reads the shots' states, and once in each
/// shot's own close. Either wait alone keeps this rule, so the test fails
/// only when both are removed; why the first wait comes first (which
/// states are read after a shot's failure) is the protocol's to pin.
#[test]
fn closed_with_calls_in_flight_every_reply_ends_done() {
    let (held, ends) = bounded(|| {
        loaded!(LOAD, |chain, replies, hold, ends| {
            let _ = chain.close();
            (ends.held_at_close(), ends_of(&mut replies))
        })
    });
    assert_eq!(
        held, LOAD as usize,
        "precondition: the close met every shot's prepare held"
    );
    assert_eq!(ends, vec![ReplyStatus::Done; 2 * LOAD as usize]);
}

/// The same scene, closed twice and then dropped: the backend is closed
/// once, by the first close, and dropped once.
#[test]
fn closed_twice_with_calls_in_flight_the_backend_is_closed_once() {
    let (held, closes, drops) = bounded(|| {
        loaded!(LOAD, |chain, replies, hold, ends| {
            let _ = chain.close();
            let _ = chain.close();
            drop(chain);
            let _ = ends_of(&mut replies);
            (ends.held_at_close(), ends.closes(), ends.drops())
        })
    });
    assert_eq!(
        held, LOAD as usize,
        "precondition: the first close met every shot's prepare held"
    );
    assert_eq!((closes, drops), (1, 1));
}

/// A run of repetition3's prepare on [`LOAD`] shots, awaited by a caller
/// that gives up after its first poll, as a timeout does: dropped
/// mid-flight, it drops the backend once, unclosed, with no panic.
#[test]
fn a_run_dropped_mid_flight_drops_the_backend_once_unclosed() {
    let (polled, closes, drops) = bounded(|| {
        let qodec = repetition3();
        let program = Program::parse(&qodec, &["prepare_z 0", "measure_z 0"]);
        let hold = Gate::default();
        let ends = Arc::new(Ends::default());
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let coins = held_coins_on_threads(stim_rz().set, SEED, hold.clone());
        let chain = compose(
            static_driver() >> layer.adapter() >> profiler(),
            Counted::new(coins, &ends),
        )
        .expect("every boundary matches");
        let shots: Vec<_> = (0..LOAD).collect();
        let run = chain
            .run(program.to_static(Needs::default()), shot_set(&shots))
            .expect("the job fits the chain");
        let polled = {
            let mut run = pin!(run.into_future());
            let mut context = Context::from_waker(Waker::noop());
            run.as_mut().poll(&mut context).is_pending()
        };
        hold.open();
        (polled, ends.closes(), ends.drops())
    });
    assert!(polled, "the run was in flight when dropped");
    assert_eq!((closes, drops), (0, 1));
}

/// A run of repetition3's program on [`LOAD`] shots that ends: the run
/// closes the backend once, and drops it once.
#[test]
fn a_run_of_many_shots_closes_the_backend_once() {
    let (closes, drops) = bounded(|| {
        let qodec = repetition3();
        let program = common::repetition3_program(&qodec);
        let hold = Gate::default();
        hold.open();
        let ends = Arc::new(Ends::default());
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let coins = held_coins_on_threads(stim_rz().set, SEED, hold.clone());
        let chain = compose(
            static_driver() >> layer.adapter() >> profiler(),
            Counted::new(coins, &ends),
        )
        .expect("every boundary matches");
        let shots: Vec<_> = (0..LOAD).collect();
        let (outcomes, _) = chain
            .run(program.to_static(Needs::default()), shot_set(&shots))
            .expect("the job fits the chain")
            .wait();
        outcomes.expect("the run succeeds");
        (ends.closes(), ends.drops())
    });
    assert_eq!((closes, drops), (1, 1));
}
