//! A device that fails mid-run below `qodec >> profiler`: the run ends
//! failed, naming the device's reason; later calls are refused for it; and
//! a later call in flight ends by the protocol's rule: only once the
//! failure is recorded (failed inside `call`, or the failing reply polled)
//! does it end failed, with no events; refusing a new call for the failure
//! does not record it. Each test is bounded in time, so that a hang fails.
//!
//! The run's cases are c4c6's, its two layers over the profiler, over a
//! device answering within its call and over a device whose shots run on
//! threads of their own. The cases of calls in flight are repetition3's:
//! the layer serves a call within it (QD1, mode (c)), so the only reply
//! left in flight above the layer is that of an upper call whose lowering
//! reads nothing (a prepare), which goes below as a send; a reading call's
//! reply holds its events by the time `call` returns.

mod common;

use executor_protocol::{
    Driver, DriverExt, Executor, InstructionBatch, InstructionBatchRef, InstructionCall, Needs,
    Rejected, ReplyStatus, ShotId, ShotSet, StaticProgramError, static_driver, wait,
};
use qodec_demo::profiler::profiler;
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;

use common::devices::{FailAt, Gate, LOST, failing_coins, failing_coins_on_threads};
use common::waiting::{bounded, end_of};
use common::{CODE_SHOTS, SEED, bottom, c4c6, c4c6_program, repetition3, shot_set, stim_rz};

/// The shot of c4c6 whose device fails, and at which of its bottom batches
/// (of 69): one of the idles', mid-run.
const C4C6_FAILURE: (ShotId, u64) = (2, 40);

/// c4c6's chain, both layers over the profiler, over `$device`, driven by
/// `$driver` (the static driver when none is given).
macro_rules! c4c6_over {
    ($qodec:expr, $device:expr) => {
        c4c6_over!(static_driver(), $qodec, $device)
    };
    ($driver:expr, $qodec:expr, $device:expr) => {{
        let top = QodecLayer::new(&$qodec, 0).expect("c4c6 plans");
        let middle = QodecLayer::new(&$qodec, 1).expect("c4c6 plans");
        ($driver >> top.adapter() >> middle.adapter() >> profiler() >> $device)
            .expect("every boundary matches")
    }};
}

/// c4c6's run on shots 0 to 3 over `$device`: the program's output.
macro_rules! c4c6_run_over {
    ($device:expr) => {{
        let qodec = c4c6();
        let set = bottom(&qodec).set.clone();
        let chain = c4c6_over!(qodec, $device(set, SEED, failure()));
        let shots: Vec<_> = CODE_SHOTS.collect();
        let (outcomes, _) = chain
            .run(
                c4c6_program(&qodec).to_static(Needs::default()),
                shot_set(&shots),
            )
            .expect("the job fits the chain")
            .wait();
        outcomes
    }};
}

/// The device's failure in c4c6's runs.
fn failure() -> FailAt {
    FailAt::new(C4C6_FAILURE.0, C4C6_FAILURE.1)
}

/// Whether the run failed at a batch's reply, naming the device's reason.
fn names_the_devices_reason(
    outcomes: &Result<Vec<executor_protocol::ShotOutcome>, StaticProgramError>,
) -> bool {
    matches!(outcomes, Err(StaticProgramError::Failed { reason, .. }) if *reason == failure().reason())
}

/// A device losing a shot's qubits part-way through c4c6's idles, below
/// both layers and the profiler: the run ends failed, with the device's
/// own reason, not a layer's.
#[test]
fn a_device_failing_mid_run_fails_the_run_naming_its_reason() {
    let outcomes = bounded(|| c4c6_run_over!(failing_coins));
    assert!(names_the_devices_reason(&outcomes), "{outcomes:?}");
}

/// The same device, each shot on a thread of its own, so that its replies
/// come later: the run ends failed, with the device's own reason.
#[test]
fn a_device_on_threads_failing_mid_run_fails_the_run_naming_its_reason() {
    let outcomes = bounded(|| c4c6_run_over!(failing_coins_on_threads));
    assert!(names_the_devices_reason(&outcomes), "{outcomes:?}");
}

/// A program that runs its batches in turn until one fails, then calls
/// that batch again: the failure, and how the call after it ended.
struct CallsAgainAfterAFailure(Program);

impl executor_protocol::Program for CallsAgainAfterAFailure {
    type Instruction = InstructionCall;
    type Output = Option<(String, Result<(), Rejected>)>;

    fn needs(&self) -> Needs {
        Needs::default().with_instruction_set(Some(std::sync::Arc::clone(&self.0.top.set)))
    }
}

/// The driver of a [`CallsAgainAfterAFailure`]: calls its batches in turn
/// on every shot until one fails, then calls that batch again.
struct CallsAgainAfterAFailureDriver;

impl Driver<CallsAgainAfterAFailure> for CallsAgainAfterAFailureDriver {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: CallsAgainAfterAFailure,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Option<(String, Result<(), Rejected>)> {
        let mut outcomes = Vec::new();
        for instructions in &program.0.batches {
            let batch = InstructionBatchRef {
                shots,
                instructions,
            };
            let reply = executor
                .call(batch)
                .expect("the chain takes each batch until one fails");
            if let Err(reason) = wait(reply, &mut outcomes) {
                return Some((reason, executor.call(batch).map(drop)));
            }
        }
        None
    }
}

/// c4c6's program over `$device`, called again after its failure: how the
/// call after the failure ended.
macro_rules! called_again_over {
    ($device:expr) => {{
        let qodec = c4c6();
        let set = bottom(&qodec).set.clone();
        let chain = c4c6_over!(
            CallsAgainAfterAFailureDriver.into_chain(),
            qodec,
            $device(set, SEED, failure())
        );
        let shots: Vec<_> = CODE_SHOTS.collect();
        let (ended, _) = chain
            .run(
                CallsAgainAfterAFailure(c4c6_program(&qodec)),
                shot_set(&shots),
            )
            .expect("the job fits the chain")
            .wait();
        let (reason, again) = ended.expect("a batch fails");
        assert_eq!(
            reason,
            failure().reason(),
            "the device's failure ends the batch"
        );
        again
    }};
}

/// After the device failed mid-run, a program that calls the chain again
/// is refused, for the earlier failure, which it names.
#[test]
fn after_a_device_failure_a_later_call_is_refused_naming_it() {
    let again = bounded(|| called_again_over!(failing_coins));
    assert_eq!(again, Err(Rejected::EarlierFailure(failure().reason())));
}

/// The same, over the device whose shots run on threads of their own.
#[test]
fn after_a_device_on_threads_failed_a_later_call_is_refused_naming_it() {
    let again = bounded(|| called_again_over!(failing_coins_on_threads));
    assert_eq!(again, Err(Rejected::EarlierFailure(failure().reason())));
}

/// The shot whose device fails in the cases of calls in flight, and the
/// shot of the later call.
const FAILING: ShotId = 0;
const OTHER: ShotId = 1;

/// Repetition3's layer over the profiler, over the device on threads,
/// whose shot [`FAILING`] fails its first batch once `gate` opens, then
/// opens `told` once it recorded the failure; with a prepare in flight on
/// [`FAILING`], running, held at the device until the gate opens, and on
/// [`OTHER`] a prepare in flight and a measure called: the replies of the
/// failing prepare and of the measure. `$then` runs, once `told` is open,
/// on the chain, the two replies and a way to make a prepare on any shot,
/// before the chain is dropped.
macro_rules! in_flight {
    (|$chain:ident, $failing:ident, $read:ident, $prepare_on:ident| $then:block) => {{
        let qodec = repetition3();
        let program = Program::parse(&qodec, &["prepare_z 0", "measure_z 0"]);
        let (prepare, measure) = (&program.batches[0], &program.batches[1]);
        let (gate, told) = (Gate::default(), Gate::default());
        let fail = FailAt::new(FAILING, 0).held_at(&gate).telling(&told);
        let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
        let mut $chain =
            (layer.adapter() >> profiler() >> failing_coins_on_threads(stim_rz().set, SEED, fail))
                .expect("every boundary matches");
        let (failing, other) = (shot_set(&[FAILING]), shot_set(&[OTHER]));
        let call = |shots| InstructionBatchRef {
            shots,
            instructions: prepare,
        };
        #[allow(unused_mut)]
        let mut $failing = $chain
            .call(call(&failing))
            .expect("the chain takes the prepare");
        let other_prepare = $chain
            .call(call(&other))
            .expect("the chain takes the prepare");
        let read = InstructionBatchRef {
            shots: &other,
            instructions: measure,
        };
        #[allow(unused_mut)]
        let mut $read = $chain.call(read).expect("the chain takes the measure");
        gate.wait_for_arrivals(1);
        gate.open();
        told.pass();
        let $prepare_on = |shot: ShotId| (shot_set(&[shot]), prepare.clone());
        let output = $then;
        drop(other_prepare);
        output
    }};
}

/// A program holds a prepare in flight on one shot and a measure on
/// another, called after it; the device fails the prepare's shot, and
/// nothing has polled the prepare's reply: the measure's reply, a later
/// call's, ends done with its outcome, since the failure is not recorded
/// above the device.
#[test]
fn a_later_call_in_flight_ends_done_while_the_failing_reply_is_unpolled() {
    let (end, read) = bounded(|| {
        in_flight!(|_chain, failing, read, _prepare_on| {
            let mut events = Vec::new();
            let end = end_of(&mut read, &mut events);
            let mut failing_events = Vec::new();
            let failing_end = end_of(&mut failing, &mut failing_events);
            assert!(
                matches!(&failing_end, ReplyStatus::Failed(r) if r.contains(LOST)),
                "the prepare's reply fails for the device: {failing_end:?}"
            );
            (end, events.len())
        })
    });
    assert_eq!((end, read), (ReplyStatus::Done, 1));
}

/// The same scene, but the program polls the failing prepare's reply
/// first, which records the failure: the measure's reply, a later call's,
/// then ends failed for it, with no events, though its outcome was read.
#[test]
fn once_the_failing_reply_is_polled_a_later_call_in_flight_fails_with_no_events() {
    let (end, events) = bounded(|| {
        in_flight!(|_chain, failing, read, _prepare_on| {
            let mut failing_events = Vec::new();
            let failing_end = end_of(&mut failing, &mut failing_events);
            assert!(
                matches!(&failing_end, ReplyStatus::Failed(r) if r.contains(LOST)),
                "the prepare's reply fails for the device: {failing_end:?}"
            );
            let mut events = Vec::new();
            (end_of(&mut read, &mut events), events)
        })
    });
    let earlier = Rejected::EarlierFailure(FailAt::new(FAILING, 0).reason()).to_string();
    assert_eq!((end, events), (ReplyStatus::Failed(earlier), vec![]));
}

/// The same scene, the failing prepare's reply unpolled: a prepare is
/// called on `$shot`, once the device recorded its failure; then the
/// measure's reply is polled to its end. The refusal, and the measure's
/// end and events.
macro_rules! refused_on {
    ($shot:expr) => {
        bounded(|| {
            in_flight!(|chain, _failing, read, prepare_on| {
                let (shots, instructions) = prepare_on($shot);
                let batch = InstructionBatchRef {
                    shots: &shots,
                    instructions: &instructions,
                };
                let Err(refusal) = chain.call(batch).map(drop) else {
                    panic!("a new call is refused once the device has the failure");
                };
                let reason = FailAt::new(FAILING, 0).reason();
                assert_eq!(
                    refusal,
                    Rejected::EarlierFailure(reason),
                    "the new call is refused for the failure"
                );
                let mut events = Vec::new();
                let end = end_of(&mut read, &mut events);
                (end, events.len())
            })
        })
    };
}

/// The same scene, the failing prepare's reply unpolled: a new call on
/// the failed shot, whose state below is gone, is refused for the
/// failure, and the measure's reply, a later call's than the failing one
/// but an earlier one's than the refused call, still ends done with its
/// outcome: refusing the call does not record the failure for the calls
/// in flight.
#[test]
fn refusing_a_call_on_the_failed_shot_leaves_a_later_call_in_flight_to_end_done() {
    let (end, read) = refused_on!(FAILING);
    assert_eq!((end, read), (ReplyStatus::Done, 1));
}

/// The same, the new call on a fresh shot, refused for the failure of
/// another shot: the measure's reply still ends done with its outcome.
#[test]
fn refusing_a_call_on_a_fresh_shot_leaves_a_later_call_in_flight_to_end_done() {
    let (end, read) = refused_on!(OTHER + 1);
    assert_eq!((end, read), (ReplyStatus::Done, 1));
}
