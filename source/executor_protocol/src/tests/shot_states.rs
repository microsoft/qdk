// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{RawWaker, RawWakerVTable, Wake, Waker};

use crate::reply::CANCELLATION;
use crate::tests::compose::Workers;
use crate::tests::test_double::{
    Seen, TestDoubleExecutor, TestDoubleModel, TestDoubleState, run, step,
};
use crate::tests::{
    Bits, Count, bits_set, call, counted, counting, ends_soon, run_and_close, wakes,
};
use crate::{
    Close, Executor, ImmediateReply, IncrementalReply, InstructionBatch, InstructionBatchRef,
    Operand, Rejected, Reply, ReplySender, ReplyStatus, ShotId, ShotOutcome, ShotSet, ShotStates,
    Value, conformance, reply_channel, wait,
};

/// An executor on `doubles`; with what their states ran, and the states made
/// so far.
fn executor(
    doubles: TestDoubleModel,
) -> (
    TestDoubleExecutor<TestDoubleModel>,
    Rc<RefCell<Seen>>,
    Rc<Cell<usize>>,
) {
    let (seen, made) = (Rc::clone(&doubles.seen), Rc::clone(&doubles.made));
    (TestDoubleExecutor::with_model(doubles, 1), seen, made)
}

/// The shots `ids`, in that order.
fn shot_set(ids: &[ShotId]) -> ShotSet {
    ShotSet::new(ids.to_vec()).expect("a test names distinct shots")
}

/// The calls shot `j`'s state ran; `None` when none was made for it.
fn calls(seen: &Rc<RefCell<Seen>>, j: ShotId) -> Option<u64> {
    seen.borrow().calls.get(&j).copied()
}

/// Shot `j`'s oldest pending reply, which the test ends.
fn pending_reply(seen: &Rc<RefCell<Seen>>, j: ShotId) -> ReplySender<ShotOutcome> {
    let mut seen = seen.borrow_mut();
    let replies = seen.replies.get_mut(&j).expect("shot j defers its replies");
    assert!(!replies.is_empty(), "shot {j} has a pending reply");
    replies.remove(0)
}

// Making the states.

/// An executor makes no state until it is called: each shot's is made at the
/// shot's first call, once, and kept for its later calls.
#[test]
fn a_shots_state_is_made_at_its_first_call_and_kept_for_the_next() {
    let (mut executor, seen, made) = executor(TestDoubleModel::default());
    assert_eq!(made.get(), 0, "built, not called");
    step(
        &mut executor,
        &shot_set(&[3, 1]),
        &[true],
        "shots 3 and 1 run",
    );
    assert_eq!(made.get(), 2, "shots 3 and 1 made theirs");
    step(
        &mut executor,
        &shot_set(&[1, 2]),
        &[true],
        "shots 1 and 2 run",
    );
    assert_eq!(made.get(), 3, "shot 1's was kept, shot 2's made");
    assert_eq!(
        [1, 2, 3].map(|j| calls(&seen, j)),
        [Some(2), Some(1), Some(1)],
        "shot 1 ran on the state kept"
    );
}

/// Each shot's state is its own: shot 1 ran one call, shot 2 two, and
/// shot 3, never called, has none.
#[test]
fn each_shot_has_its_own_state() {
    let (mut executor, _, _) = executor(TestDoubleModel::default());
    step(
        &mut executor,
        &shot_set(&[1, 2]),
        &[true],
        "shots 1 and 2 run",
    );
    step(&mut executor, &ShotSet::one(2), &[true], "shot 2 runs");
    let calls = [1, 2, 3].map(|j| executor.state(j).map(TestDoubleState::calls));
    assert_eq!(calls, [Some(1), Some(2), None]);
}

/// Each shot's state is its own, at the ends of the shot ids too: shots
/// `ShotId::MAX` and 0, made in that order, each run on their own state.
#[test]
fn the_last_shot_id_and_the_first_have_their_own_states() {
    let (mut executor, _, _) = executor(TestDoubleModel::default());
    let ends = shot_set(&[ShotId::MAX, 0]);
    step(&mut executor, &ends, &[true], "both ends run");
    step(
        &mut executor,
        &ShotSet::one(ShotId::MAX),
        &[true],
        "it runs",
    );
    let calls = [ShotId::MAX, 0].map(|j| executor.state(j).map(TestDoubleState::calls));
    assert_eq!(calls, [Some(2), Some(1)]);
}

/// A call refused by its first shot ran no shot, so it keeps no state of
/// it: the executor, closed then, reports no shot.
#[test]
fn a_call_refused_by_its_first_shot_keeps_no_state() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        probabilities: true,
        ..TestDoubleModel::default()
    });
    run(&mut executor, &shot_set(&[2, 1]), &[true]).expect_err("shot 2, called first, refuses");
    let report = executor.close();
    assert!(report.log_p.is_empty(), "a shot that never ran: {report:?}");
}

/// A call refused because a later shot's state cannot be made ran no
/// shot, so it keeps none of the states it made: the executor, closed then,
/// reports no shot.
#[test]
fn a_call_refused_because_a_state_could_not_be_made_keeps_none() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        unavailable_once: Cell::new(Some(2)),
        probabilities: true,
        ..TestDoubleModel::default()
    });
    run(&mut executor, &shot_set(&[1, 2]), &[true]).expect_err("shot 2's memory is in use");
    let report = executor.close();
    assert!(report.log_p.is_empty(), "a shot that never ran: {report:?}");
}

// Batches.

/// A batch may name any of the executor's shots: it runs those shots
/// only, and shot 2, kept from an earlier call, does not run.
#[test]
fn a_batch_runs_only_the_shots_it_names() {
    let (mut executor, seen, _) = executor(TestDoubleModel::default());
    step(
        &mut executor,
        &shot_set(&[1, 2, 3]),
        &[true],
        "shots 1 to 3 run",
    );
    step(
        &mut executor,
        &shot_set(&[3, 1]),
        &[true],
        "shots 3 and 1 run",
    );
    assert_eq!(
        [1, 2, 3].map(|j| calls(&seen, j)),
        [Some(2), Some(1), Some(2)]
    );
}

/// A batch names its shots in any order, and runs them shot by shot
/// in that order.
#[test]
fn a_batch_runs_its_shots_in_its_order() {
    let (mut executor, _, _) = executor(TestDoubleModel::default());
    let (outcomes, end) = run(&mut executor, &shot_set(&[3, 1]), &[true, true])
        .expect("the executor runs shots 3 and 1");
    assert_eq!(end, Ok(()), "both shots run");
    assert_eq!(
        outcomes.iter().map(|o| o.shot).collect::<Vec<_>>(),
        [3, 3, 1, 1]
    );
}

/// A batch on shots 1 and 4 calls each shot's state on that shot alone: a
/// state, an executor of its own shot, is never asked to run another.
#[test]
fn each_state_is_called_on_its_own_shot_alone() {
    /// A state that records the shots each of its calls names.
    struct SeesShots(Rc<RefCell<Vec<Vec<ShotId>>>>);
    impl Executor<InstructionBatch<bool>> for SeesShots {
        type Reply = ImmediateReply<ShotOutcome>;

        fn call(&mut self, batch: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
            self.0.borrow_mut().push(batch.shots.to_vec());
            Ok(ImmediateReply::default())
        }
    }
    let seen = Rc::default();
    let mut states = ShotStates::default();
    let shots = shot_set(&[1, 4]);
    let batch = InstructionBatchRef {
        shots: &shots,
        instructions: &[true],
    };
    let reply = states
        .call(batch, |_| Ok(()), |_| Ok(SeesShots(Rc::clone(&seen))), drop)
        .expect("shots 1 and 4 run");
    wait(reply, &mut Vec::new()).expect("both shots end");
    assert_eq!(*seen.borrow(), [vec![1], vec![4]]);
}

/// A refused call changes nothing, so the executor takes the next one, by
/// the conformance kit's check.
#[test]
fn a_refused_call_leaves_the_executor_usable() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &shot_set(&[2, 1]),
            instructions: &[true],
        },
        InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[true],
        },
    );
}

/// The first shot a batch calls refuses it: nothing ran, so the
/// batch is refused with that shot's rejection, and every other
/// shot's state is as its earlier calls left it.
#[test]
fn the_first_shots_rejection_refuses_the_batch() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    assert_eq!(
        run(&mut executor, &shot_set(&[2, 1]), &[true]).err(),
        Some(Rejected::Unavailable("busy".into()))
    );
    assert_eq!(executor.state(1).map(TestDoubleState::calls), Some(1));
}

/// The first shot a batch calls refuses it for its own earlier failure,
/// which its reply did not show yet: the executor failed, so a later call
/// on another shot alone is refused with that failure too.
#[test]
fn a_first_shot_refusing_for_its_earlier_failure_fails_the_executor() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failed_unseen: Some(1),
        ..TestDoubleModel::default()
    });
    run(&mut executor, &ShotSet::one(1), &[true])
        .expect_err("shot 1 refuses for its earlier failure");
    assert_eq!(
        run(&mut executor, &ShotSet::one(2), &[true]).err(),
        Some(Rejected::EarlierFailure("lost".into()))
    );
}

/// A shot after the first refuses: the shots before it already ran, so it
/// is too late to refuse, and the batch's later shots do not run.
#[test]
fn a_later_shots_rejection_runs_none_of_the_later_shots() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    let (_, end) = run(
        &mut executor,
        &ShotSet::range(1..=3).expect("1..=3 is not empty"),
        &[true],
    )
    .expect("shot 1, called first, accepts");
    end.expect_err("shot 2 refuses too late");
    assert_eq!(calls(&seen, 3).unwrap_or(0), 0, "shot 3 ran");
}

/// A shot's batch fails as it is called: the executor fails before anyone
/// polls the batch's reply, and refuses the next call.
#[test]
fn a_failed_batch_ends_the_executor_even_unpolled() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(1),
        ..TestDoubleModel::default()
    });
    let _unpolled = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[true],
        })
        .expect("the batch fails after it was accepted");
    assert_eq!(
        run(&mut executor, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::EarlierFailure("lost".into()))
    );
}

/// A shot's batch is cancelled as it is called, below the executor (by an
/// executor its state places in turn, dropped): the executor fails, as
/// for a failure, and refuses the next call. Chosen because a cancelled
/// call counts as a failed one unless a rule says otherwise: the executor
/// holds its states, so the cancellation came from below, and the shot's
/// state has nothing left to run on.
#[test]
fn a_batch_cancelled_as_it_is_called_fails_the_executor() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        cancelling: Some(1),
        ..TestDoubleModel::default()
    });
    let _unpolled = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[true],
        })
        .expect("the batch is cancelled after it was accepted");
    assert_eq!(
        run(&mut executor, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::EarlierFailure(CANCELLATION.to_owned()))
    );
}

/// A shot's batch is cancelled after its call, while another shot's part
/// of it ran: the batch's reply ends `ReplyStatus::Cancelled`, after the
/// outcomes before it, so that the caller tells the call from one that
/// failed.
/// Typed, not a reason of `Failed`: any executor could fail with the same
/// text, and the compiler cannot check a match on a string; tokio
/// (`JoinError::is_cancelled`), Java (`CancellationException`) and Python
/// (`CancelledError`) type it too.
#[test]
fn a_part_cancelled_after_its_call_ends_the_reply_cancelled() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![2],
        ..TestDoubleModel::default()
    });
    let shots = shot_set(&[1, 2]);
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        })
        .expect("the executor runs shots 1 and 2");
    pending_reply(&seen, 2).cancel();
    let mut out = Vec::new();
    let end = reply.poll_events(&mut out, Waker::noop());
    assert_eq!(end, ReplyStatus::Cancelled);
    assert_eq!(out.len(), 2, "both shots' outcomes stand: {out:?}");
}

/// Shot 1's batch fails; shot 2's state, which the failure did not hit, is
/// sound, yet a call naming only shot 2 is refused: a failed batch fails
/// the whole executor.
//
// A failed batch leaves its shots' states part-way through it, so a later
// batch on them would run on a state that the program never made: those
// shots must refuse it. The whole executor refuses, not only those shots,
// for simplicity: no driver yet needs to go on with the other shots.
// A new case, such as a long sampling run that keeps its other shots after
// one shot's device fails, could narrow it to the failed shots;
// `lifecycle.rs` describes how.
#[test]
fn a_failed_batch_refuses_later_calls_on_shots_it_did_not_hit() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(1),
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(2), &[true], "shot 2 runs");
    let (_, end) = run(&mut executor, &ShotSet::one(1), &[true]).expect("shot 1 accepts");
    end.expect_err("shot 1's batch fails after it was accepted");
    assert_eq!(
        run(&mut executor, &ShotSet::one(2), &[true]).err(),
        Some(Rejected::EarlierFailure("lost".into()))
    );
}

/// A shot's batch fails: the executor refuses every later call on the shot
/// it hit, alone or with another, by the conformance kit's check of every
/// executor whose state carries over (a call on the other shot alone:
/// `a_failed_batch_refuses_later_calls_on_shots_it_did_not_hit`).
#[test]
fn a_failed_batch_refuses_every_later_call_on_its_shots() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(1),
        ..TestDoubleModel::default()
    });
    let both = ShotSet::range(1..=2).expect("1..=2 is not empty");
    conformance::refuses_calls_after_a_failure::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[true],
        },
        &[&ShotSet::one(1), &both].map(|shots| InstructionBatchRef {
            shots,
            instructions: &[true],
        }),
    );
}

/// A batch fails on a shot as the shot is called: the batch's later shots
/// do not run.
#[test]
fn a_shots_failed_call_stops_the_later_shots() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        failing: Some(1),
        ..TestDoubleModel::default()
    });
    let (_, end) = run(&mut executor, &shot_set(&[1, 2]), &[true]).expect("shot 1 accepts");
    end.expect_err("shot 1's batch fails after it was accepted");
    assert_eq!(calls(&seen, 2).unwrap_or(0), 0, "shot 2 ran");
}

/// Shot 2's batch fails as it is called, after its reads: the reply hands
/// over the outcomes of shots 1 and 2, then ends with the failure.
#[test]
fn a_failed_batch_hands_over_its_outcomes_before_its_failure() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(2),
        ..TestDoubleModel::default()
    });
    let (outcomes, end) =
        run(&mut executor, &shot_set(&[1, 2]), &[true, true]).expect("shot 1 accepts");
    assert_eq!(
        outcomes.iter().map(|o| (o.shot, o.tag)).collect::<Vec<_>>(),
        [(1, 0), (1, 1), (2, 0), (2, 1)]
    );
    assert_eq!(end, Err("lost".into()));
}

/// A call whose batch failed on a shot reports no state made for it,
/// whether its shot ran or not; a state made by an earlier call is
/// reported. Chosen so that every executor on these states, placed or not,
/// reports the same states after a failure, though a placed one may run
/// shots of the failed call that the others never reach; `futures`'
/// `try_join_all` and rayon's `collect::<Result<_, _>>()` drop the others'
/// results too. Shot 4 runs a call, then a batch on shots 4, 1, 2 and 3
/// fails as shot 2 runs it: at close, only shot 4's state is reported, not
/// shot 1's nor shot 2's, which ran, nor shot 3's, which did not.
#[test]
fn a_call_whose_batch_failed_reports_no_state_made_for_it() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(2),
        probabilities: true,
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(4), &[true], "shot 4 runs");
    let (_, end) = run(&mut executor, &shot_set(&[4, 1, 2, 3]), &[true]).expect("shot 4 accepts");
    end.expect_err("shot 2's batch fails after it was accepted");
    let report = executor.close();
    let log_p = [4, 1, 2, 3].map(|j| report.log_p.get(j));
    assert_eq!(log_p, [Some(-2.0), None, None, None]);
}

/// A call whose shot's reply failed after the call returned reports no
/// state made for it, as when its batch fails inside the call: the
/// executor learns of the failure when the reply is polled, and decides
/// at close. Chosen for the same report in every form of the executor.
/// Shot 4 runs a call, then a batch on shots 4, 1 and 2 is accepted;
/// shot 2's reply fails later: at close, only shot 4's state is reported.
#[test]
fn a_call_whose_reply_failed_later_reports_no_state_made_for_it() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![2],
        probabilities: true,
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(4), &[true], "shot 4 runs");
    let shots = shot_set(&[4, 1, 2]);
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        })
        .expect("the executor runs shots 4, 1 and 2");
    pending_reply(&seen, 2).fail("lost".into());
    let failed = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 2 failed");
    let report = executor.close();
    let log_p = [4, 1, 2].map(|j| report.log_p.get(j));
    assert_eq!(log_p, [Some(-2.0), None, None]);
}

/// A later shot's rejection fails the executor as the call returns,
/// before the batch's reply is polled: the shots before it ran, so the
/// next batch is refused even if the caller has not read the failure
/// yet.
#[test]
fn a_later_shots_rejection_fails_the_executor_before_its_reply_is_polled() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    let both = ShotSet::range(1..=2).expect("1..=2 is not empty");
    let _unread = executor
        .call(InstructionBatchRef {
            shots: &both,
            instructions: &[true],
        })
        .expect("shot 1, called first, accepts: shot 2 refuses too late");
    assert_eq!(
        run(&mut executor, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::EarlierFailure("shot 2: unavailable: busy".into()))
    );
}

/// A failed executor makes no state: nothing is made for a shot that cannot
/// run.
#[test]
fn a_failed_executor_makes_no_state() {
    let (mut executor, _, made) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    run(
        &mut executor,
        &ShotSet::range(1..=2).expect("1..=2 is not empty"),
        &[true],
    )
    .expect("shot 1, called first, accepts: shot 2 refuses too late")
    .1
    .expect_err("shot 2's refusal fails the batch and the executor");
    run(&mut executor, &ShotSet::one(9), &[true]).expect_err("the executor failed");
    assert_eq!(made.get(), 2, "none was made for shot 9");
}

/// A failed executor names its failure whatever the batch, even one that
/// its own check refuses: the caller learns first that the run is lost.
#[test]
fn a_failed_executor_names_its_failure_whatever_the_batch() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(1),
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    let (_, end) = run(&mut executor, &ShotSet::one(1), &[true]).expect("shot 1 accepts");
    end.expect_err("shot 1's batch fails after it was accepted");
    assert_eq!(
        run(&mut executor, &ShotSet::one(2), &[false]).err(),
        Some(Rejected::EarlierFailure("lost".into()))
    );
}

/// A model with a bug panics making shot 2's state, in a batch on shots 1
/// and 2: the panic reaches the caller; as a `Mutex` is poisoned, the
/// executor refuses every later call, naming the panic, even on shot 1,
/// whose state could be made.
#[test]
fn after_a_panic_inside_call_it_refuses_every_later_call() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        panicking: Some(2),
        ..TestDoubleModel::default()
    });
    let shots = shot_set(&[1, 2]);
    panic::catch_unwind(AssertUnwindSafe(|| run(&mut executor, &shots, &[true])))
        .expect_err("the model's panic reaches the caller");
    assert_eq!(
        run(&mut executor, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::EarlierFailure(
            "the executor panicked during the call".into()
        ))
    );
}

/// The executor's check refuses a batch once, before any state is made:
/// nothing is made for a batch that cannot run.
#[test]
fn a_batch_the_check_refuses_makes_no_state() {
    let (mut executor, _, made) = executor(TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    assert_eq!(
        run(&mut executor, &shot_set(&[1, 2]), &[true, false]).err(),
        Some(Rejected::Instruction {
            at: 1,
            reason: "not a read".into()
        })
    );
    assert_eq!(made.get(), 0, "no state was made for the refused batch");
}

/// A batch the executor's check refuses fails nothing: the executor runs the
/// next, sound, batch, by the conformance kit's check.
#[test]
fn a_batch_the_check_refuses_leaves_the_executor_usable() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<bool>, _>(
        &mut executor,
        InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[false],
        },
        InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[true],
        },
    );
}

/// Shot 1's state cannot be made when it is first called (another run
/// holds its memory): the call is refused with `Unavailable`, naming the
/// shot, and no shot ran, not even shot 2, called before it.
#[test]
fn a_state_that_cannot_be_made_refuses_the_call_and_runs_no_shot() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        unavailable_once: Cell::new(Some(1)),
        ..TestDoubleModel::default()
    });
    assert_eq!(
        run(&mut executor, &shot_set(&[2, 1]), &[true]).err(),
        Some(Rejected::Unavailable("shot 1: memory in use".into()))
    );
    assert_eq!(calls(&seen, 2).unwrap_or(0), 0, "shot 2 ran");
}

/// Shot 1's state could not be made at its first call: the same
/// batch, sent again once its memory is free, makes it and runs.
#[test]
fn the_next_call_makes_a_state_that_could_not_be_made() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        unavailable_once: Cell::new(Some(1)),
        ..TestDoubleModel::default()
    });
    let both = shot_set(&[2, 1]);
    run(&mut executor, &both, &[true]).expect_err("shot 1's memory is in use");
    let (outcomes, end) = run(&mut executor, &both, &[true]).expect("shot 1's memory is free now");
    assert_eq!((outcomes.len(), end), (2, Ok(())));
}

/// Shot 2's state cannot be made, and shot 1, called first, would refuse
/// the batch: every state is made before any shot runs, so the call is
/// refused because shot 2's state cannot be made.
#[test]
fn a_state_that_cannot_be_made_comes_before_the_first_shots_rejection() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(1),
        unavailable_once: Cell::new(Some(2)),
        ..TestDoubleModel::default()
    });
    assert_eq!(
        run(&mut executor, &shot_set(&[1, 2]), &[true]).err(),
        Some(Rejected::Unavailable("shot 2: memory in use".into()))
    );
}

// Replies.

/// A batch's reply is pending while a shot's reply is, with the
/// outcomes so far, and done when the last shot's reply ends.
#[test]
fn a_batch_is_pending_until_its_last_shot_ends() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![2],
        ..TestDoubleModel::default()
    });
    let both = ShotSet::range(1..=2).expect("1..=2 is not empty");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &both,
            instructions: &[true],
        })
        .expect("the executor runs shots 1 and 2");
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending
    );
    assert_eq!(
        out.iter().map(|o| o.shot).collect::<Vec<_>>(),
        [1, 2],
        "shot 2's read was pushed before its end"
    );
    pending_reply(&seen, 2).done();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Done
    );
}

/// A driver gathers a run's outcomes in one buffer, which holds shot 7's
/// read already; shots 1 and 2 end inside `call`: the reply appends their
/// reads after shot 7's, which it keeps.
#[test]
fn a_reply_appends_its_outcomes_to_those_the_caller_holds() {
    let (mut executor, _, _) = executor(TestDoubleModel::default());
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[1, 2]),
            instructions: &[true],
        })
        .expect("the executor runs shots 1 and 2");
    let mut out = vec![read(7, 0)];
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Done
    );
    assert_eq!(out.iter().map(|o| o.shot).collect::<Vec<_>>(), [7, 1, 2]);
}

/// A driver gathers a run's outcomes in one buffer, which holds shot 7's
/// read already; shot 2's reply is still pending after `call`, so the
/// reply polls it: it appends the reads of shots 1 and 2 after shot 7's,
/// which it keeps.
#[test]
fn a_reply_with_a_part_pending_appends_its_outcomes_to_those_the_caller_holds() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        deferred: vec![2],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[1, 2]),
            instructions: &[true],
        })
        .expect("the executor runs shots 1 and 2");
    let mut out = vec![read(7, 0)];
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending,
        "shot 2's reply has not ended"
    );
    assert_eq!(out.iter().map(|o| o.shot).collect::<Vec<_>>(), [7, 1, 2]);
}

/// The outcome of a read on shot `shot`.
fn read(shot: ShotId, tag: u32) -> ShotOutcome {
    ShotOutcome::new(shot, tag, Value::One)
}

/// A state that names every outcome shot 99, as one copied from another
/// executor might: a call on shots 1 and 2 still gives each shot's outcome
/// named by that shot, since the executor names the outcomes of each
/// shot's state.
#[test]
fn each_outcome_is_named_by_the_shot_whose_state_gave_it() {
    /// A state whose outcomes all name shot 99.
    struct Mislabelled;
    impl Executor<InstructionBatch<bool>> for Mislabelled {
        type Reply = ImmediateReply<ShotOutcome>;

        fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<Self::Reply, Rejected> {
            let mut reply = ImmediateReply::default();
            reply.push(read(99, 0));
            Ok(reply)
        }
    }
    let mut states = ShotStates::default();
    let shots = shot_set(&[1, 2]);
    let batch = InstructionBatchRef {
        shots: &shots,
        instructions: &[true],
    };
    let reply = states
        .call(batch, |_| Ok(()), |_| Ok(Mislabelled), drop)
        .expect("shots 1 and 2 run");
    let mut out = Vec::new();
    wait(reply, &mut out).expect("both shots end");
    assert_eq!(out.iter().map(|o| o.shot).collect::<Vec<_>>(), [1, 2]);
}

/// Shot 2's state reads after its call returned, under shot 7's name: the
/// reply names the outcome by shot 2, the shot whose state gave it.
#[test]
fn each_late_outcome_is_named_by_the_shot_whose_state_gave_it() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![2],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(2),
            instructions: &[false],
        })
        .expect("the executor runs shot 2");
    let shot_2 = pending_reply(&seen, 2);
    shot_2.push(read(7, 0));
    shot_2.done();
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Done
    );
    assert_eq!(out.iter().map(|o| o.shot).collect::<Vec<_>>(), [2]);
}

/// An executor hands over a shot's events as soon as that shot has them,
/// before the batch ends: shot 1's read comes while shot 2 still
/// runs.
#[test]
fn an_executor_hands_over_each_shots_events_as_they_come() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2],
        ..TestDoubleModel::default()
    });
    let both = ShotSet::range(1..=2).expect("1..=2 is not empty");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &both,
            instructions: &[false],
        })
        .expect("the executor runs shots 1 and 2");
    let (shot_1, _shot_2) = (pending_reply(&seen, 1), pending_reply(&seen, 2));
    shot_1.push(read(1, 0));
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending,
        "shot 2 is still running"
    );
    assert_eq!(
        out.iter().map(|o| (o.shot, o.tag)).collect::<Vec<_>>(),
        [(1, 0)]
    );
}

/// An executor wakes its caller when any of its pending shots has news, not
/// only the first or the last: shot 2 reads while shots 1 and 3 are still
/// running.
#[test]
fn an_executor_wakes_its_caller_when_any_pending_shot_has_news() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2, 3],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[1, 2, 3]),
            instructions: &[false],
        })
        .expect("the executor runs shots 1 to 3");
    let (count, waker) = counting();
    let pending = reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(pending, ReplyStatus::Pending, "shots 1 to 3 have not ended");
    pending_reply(&seen, 2).push(read(2, 0));
    assert_eq!(wakes(&count), 1);
}

/// Two calls on shot 1 are in flight, each reading once as it is made and
/// once later, their late reads interleaved: each reply hands over only its
/// own call's outcomes.
#[test]
fn each_reply_hands_over_only_its_own_calls_outcomes() {
    fn tags(reply: &mut impl Reply<Item = ShotOutcome>) -> Vec<u32> {
        let mut out = Vec::new();
        assert_eq!(
            reply.poll_events(&mut out, Waker::noop()),
            ReplyStatus::Done,
            "both calls ended"
        );
        out.iter().map(|o| o.tag).collect()
    }
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut call = |batch: &[bool]| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(1),
                instructions: batch,
            })
            .expect("the executor runs shot 1")
    };
    let (mut first, mut second) = (call(&[true]), call(&[false, true]));
    let (shot_1_first, shot_1_second) = (pending_reply(&seen, 1), pending_reply(&seen, 1));
    shot_1_second.push(read(1, 21));
    shot_1_first.push(read(1, 11));
    shot_1_second.done();
    shot_1_first.done();
    assert_eq!(tags(&mut first), [0, 11]);
    assert_eq!(tags(&mut second), [1, 21]);
}

/// A shot's outcomes come in its reply's order across polls: shot 1 reads
/// 0 before its call returns and 1 after; the reply hands over 0, then 1.
#[test]
fn a_shots_outcomes_keep_their_order_across_its_reply() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[true],
        })
        .expect("the executor runs shot 1");
    let shot_1 = pending_reply(&seen, 1);
    shot_1.push(read(1, 1));
    shot_1.done();
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Done
    );
    assert_eq!(
        out.iter().map(|o| (o.shot, o.tag)).collect::<Vec<_>>(),
        [(1, 0), (1, 1)]
    );
}

/// A shot's outcomes come in the order it gives them, not in their tags'
/// order: shot 3 reads tag 1, then tag 0, in one batch, and its reply hands
/// over 1, then 0.
#[test]
fn a_shots_outcomes_keep_the_order_of_its_reads_not_of_their_tags() {
    let set = bits_set();
    let reads = [
        call(&set, "measure", &[Operand::Block(0), Operand::Tag(1)]),
        call(&set, "measure", &[Operand::Block(1), Operand::Tag(0)]),
    ];
    let outcomes = run_and_close(
        TestDoubleExecutor::with_model(Bits::default(), 7),
        &[3],
        &reads,
    );
    assert_eq!(outcomes.iter().map(|o| o.tag).collect::<Vec<_>>(), [1, 0]);
}

/// A shot's late outcomes come in the order it gives them, not in their
/// tags' order: shot 1 reads tag 2, then tag 1, after its call returned,
/// and its reply hands over 2, then 1.
#[test]
fn a_shots_late_outcomes_keep_the_order_of_its_reads_not_of_their_tags() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[false],
        })
        .expect("the executor runs shot 1");
    let shot_1 = pending_reply(&seen, 1);
    shot_1.extend([read(1, 2), read(1, 1)]);
    shot_1.done();
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Done
    );
    assert_eq!(out.iter().map(|o| o.tag).collect::<Vec<_>>(), [2, 1]);
}

/// A shot's reply that fails after the call was accepted fails the
/// batch: its reply ends with the shot's failure.
#[test]
fn a_shots_failed_reply_fails_the_batch() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![2],
        ..TestDoubleModel::default()
    });
    let both = ShotSet::range(1..=2).expect("1..=2 is not empty");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &both,
            instructions: &[true],
        })
        .expect("the executor runs shots 1 and 2");
    pending_reply(&seen, 2).fail("lost".into());
    assert_eq!(
        reply.poll_events(&mut Vec::new(), Waker::noop()),
        ReplyStatus::Failed("lost".into())
    );
}

/// Shot 2's reply pushes a read, then fails: the batch's reply hands
/// over the read, then ends with the failure.
#[test]
fn a_shots_failed_reply_hands_over_its_outcomes_before_its_failure() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![2],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(2),
            instructions: &[false],
        })
        .expect("the executor runs shot 2");
    let shot_2 = pending_reply(&seen, 2);
    shot_2.push(read(2, 0));
    shot_2.fail("lost".into());
    let mut out = Vec::new();
    let status = reply.poll_events(&mut out, Waker::noop());
    assert_eq!(
        out.iter().map(|o| (o.shot, o.tag)).collect::<Vec<_>>(),
        [(2, 0)]
    );
    assert_eq!(status, ReplyStatus::Failed("lost".into()));
}

/// Shot 2's reply fails while shot 1's is still pending: the executor
/// fails as soon as the reply is polled, before the batch ends, and
/// refuses the next call.
#[test]
fn a_shots_failed_reply_fails_the_executor_while_other_shots_are_pending() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2],
        ..TestDoubleModel::default()
    });
    let both = ShotSet::range(1..=2).expect("1..=2 is not empty");
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &both,
            instructions: &[true],
        })
        .expect("the executor runs shots 1 and 2");
    pending_reply(&seen, 2).fail("lost".into());
    let status = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(status, ReplyStatus::Pending, "shot 1 is still running");
    assert_eq!(
        run(&mut executor, &ShotSet::one(3), &[true]).err(),
        Some(Rejected::EarlierFailure("lost".into()))
    );
}

/// Two batches are in flight on shot 1 and the first one fails: the
/// second ends too, with no more of its events, since it ran on the state
/// the failure left.
#[test]
fn a_failure_ends_the_batches_still_in_flight() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut call_on_1 = || {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(1),
                instructions: &[false],
            })
            .expect("the executor runs shot 1")
    };
    let (mut first, mut second) = (call_on_1(), call_on_1());
    let mut out = Vec::new();
    assert_eq!(
        second.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending
    );
    let (first_reply, second_reply) = (pending_reply(&seen, 1), pending_reply(&seen, 1));
    first_reply.fail("lost".into());
    assert_eq!(
        first.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Failed("lost".into())
    );
    second_reply.push(read(1, 0));
    assert_eq!(
        second.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Failed("an earlier call failed: lost".into())
    );
    assert!(out.is_empty(), "the later batch hands over no more events");
}

/// A caller collects the events of several replies in one list: a
/// batch on shot 1 fails, and the caller polls a later one, in
/// flight, into the list that holds an event already. The reply ends, and
/// the list keeps that event.
#[test]
fn a_reply_ended_by_an_earlier_failure_keeps_the_events_its_caller_holds() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut call_on_1 = || {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(1),
                instructions: &[false],
            })
            .expect("the executor runs shot 1")
    };
    let (mut first, mut second) = (call_on_1(), call_on_1());
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    let held = ShotOutcome::new(9, 0, Value::One);
    let mut out = vec![held];
    let ended = second.poll_events(&mut out, Waker::noop());
    let reason = ReplyStatus::Failed("an earlier call failed: lost".into());
    assert_eq!(ended, reason, "the earlier failure ended the reply");
    assert_eq!(out, [held]);
}

/// A batch on shot 1 is in flight when one on shots 5 and 7 fails at
/// shot 7, too late to refuse, while shot 5 still runs; then the earlier
/// one, on shot 1, fails, recorded as its reply is polled: the later reply ends
/// with the earlier failure, not its own, since shot 5 ran on the state that
/// failure left.
#[test]
fn a_reply_failed_at_a_later_shot_ends_with_an_earlier_failure_recorded_before_it_ends() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 5],
        refusing: Some(7),
        ..TestDoubleModel::default()
    });
    let mut first = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(1),
            instructions: &[true],
        })
        .expect("the executor runs shot 1");
    let mut second = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[5, 7]),
            instructions: &[true],
        })
        .expect("shot 5, called first, accepts: shot 7 refuses too late");
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    let ended = second.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        ended,
        ReplyStatus::Failed("an earlier call failed: lost".into())
    );
}

/// A batch on shot 1 is in flight when one on shot 2 runs at once;
/// then the first fails: the second ends too, with none of its events,
/// since the executor failed before its reply was read.
#[test]
fn a_failure_ends_the_batches_in_flight_on_other_shots_too() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut first, mut second) = (call_on(1), call_on(2));
    pending_reply(&seen, 1).fail("lost".into());
    let mut out = Vec::new();
    assert_eq!(
        first.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Failed("lost".into())
    );
    out.clear();
    assert_eq!(
        second.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Failed("an earlier call failed: lost".into())
    );
    assert!(out.is_empty(), "the later batch hands over no events");
}

/// Batches on shots 2 and 3, each polled by its own caller, wait on
/// their shots when an earlier one, on shot 1, fails: the executor wakes
/// each later batch's caller, whose reply now ends, though its shot
/// has no news.
#[test]
fn an_earlier_failure_wakes_the_callers_of_later_pending_batches() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2, 3],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut first, mut second, mut third) = (call_on(1), call_on(2), call_on(3));
    let (count_2, waker_2) = counting();
    let (count_3, waker_3) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &waker_2);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    let waiting = third.poll_events(&mut Vec::new(), &waker_3);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 3 has not ended");
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    assert_ne!(wakes(&count_2), 0, "the caller on shot 2 is woken");
    assert_ne!(wakes(&count_3), 0, "the caller on shot 3 is woken");
}

/// A caller polls its batch on shot 2 with one waker, then with
/// another, before an earlier batch, on shot 1, fails: the executor
/// wakes the waker of the last poll, not the one it replaced.
#[test]
fn a_failure_wakes_only_the_waker_of_the_replys_last_poll() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut first, mut second) = (call_on(1), call_on(2));
    let (old_count, old) = counting();
    let (new_count, new) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &old);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    let waiting = second.poll_events(&mut Vec::new(), &new);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has still not ended");
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    assert_eq!(wakes(&old_count), 0, "the replaced waker is not woken");
    assert_ne!(wakes(&new_count), 0);
}

/// A batch on shot 2 waits when a later one, on shot 3, fails: the
/// failure does not concern it, so the executor does not wake its caller.
#[test]
fn a_later_failure_does_not_wake_an_earlier_pending_batch() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![2, 3],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut second, mut third) = (call_on(2), call_on(3));
    let (count, waker) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    pending_reply(&seen, 3).fail("lost".into());
    let failed = third.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 3 failed");
    assert_eq!(wakes(&count), 0);
}

/// A batch on shot 2 waits while a later one, on shot 3, fails, then
/// an earlier one, on shot 1: the earlier failure ends it, so the executor
/// wakes its caller.
#[test]
fn a_batch_between_two_failures_is_woken_by_the_earlier_one() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2, 3],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut first, mut second, mut third) = (call_on(1), call_on(2), call_on(3));
    let (count, waker) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    pending_reply(&seen, 3).fail("lost later".into());
    let failed = third.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        failed,
        ReplyStatus::Failed("lost later".into()),
        "shot 3 failed"
    );
    let waiting = second.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has still not ended");
    let woken = wakes(&count);
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    assert!(wakes(&count) > woken);
}

/// A caller polls its batch on shot 2 from inside the wake that an
/// earlier batch's failure, on shot 1, sends: the failure is recorded
/// before anyone is woken, so the reply has ended.
#[test]
fn a_caller_woken_by_an_earlier_failure_finds_its_reply_ended() {
    ends_soon(|| {
        let (mut executor, seen, _) = executor(TestDoubleModel {
            deferred: vec![1, 2],
            ..TestDoubleModel::default()
        });
        let mut call_on = |j| {
            executor
                .call(InstructionBatchRef {
                    shots: &ShotSet::one(j),
                    instructions: &[true],
                })
                .expect("the executor runs the shot")
        };
        let (mut first, mut second) = (call_on(1), call_on(2));
        let polls = Arc::new(PollsOnWake(Mutex::new((None, None))));
        let waker = Waker::from(Arc::clone(&polls));
        let waiting = second.poll_events(&mut Vec::new(), &waker);
        assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
        polls.0.lock().expect("no wake has run").0 = Some(second);
        pending_reply(&seen, 1).fail("lost".into());
        let failed = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
        let polled = polls.0.lock().expect("the wake did not panic").1.take();
        let ended = ReplyStatus::Failed("an earlier call failed: lost".into());
        assert_eq!(polled, Some(ended));
    });
}

/// A batch on shots 1 and 2 waits on shot 2 after shot 1 failed: the
/// failure is the reply's own, so the executor does not wake its caller,
/// who would poll it in vain.
#[test]
fn a_reply_whose_own_shot_failed_waits_for_its_other_shots_without_waking_itself() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[1, 2]),
            instructions: &[true],
        })
        .expect("the executor runs shots 1 and 2");
    let (count, waker) = counting();
    let waiting = reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "no shot has ended");
    pending_reply(&seen, 1).fail("lost".into());
    let woken = wakes(&count);
    let waiting = reply.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    assert_eq!(wakes(&count), woken);
}

/// A reply on shot 2 has ended when an earlier batch, on shot 1,
/// fails: the executor does not wake the caller, who has moved on.
#[test]
fn an_ended_reply_is_not_woken_by_an_earlier_failure() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut first, mut second) = (call_on(1), call_on(2));
    let (count, waker) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    pending_reply(&seen, 2).done();
    let ended = second.poll_events(&mut Vec::new(), &waker);
    assert_eq!(ended, ReplyStatus::Done, "shot 2 ended");
    let woken = wakes(&count);
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    assert_eq!(wakes(&count), woken);
}

/// Batches on shots 4, 2 and 3 wait, polled in that order; the one on
/// shot 3 ends, then an earlier one, on shot 1, fails: the executor still
/// wakes the callers of the batches on shots 2 and 4, polled before and
/// after the ended one.
#[test]
fn an_ended_reply_leaves_the_other_pending_batches_to_be_woken() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2, 3, 4],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut first, mut second, mut third, mut fourth) =
        (call_on(1), call_on(2), call_on(3), call_on(4));
    let (count_4, waker_4) = counting();
    let waiting = fourth.poll_events(&mut Vec::new(), &waker_4);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 4 has not ended");
    let (count_2, waker_2) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &waker_2);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    let waiting = third.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(waiting, ReplyStatus::Pending, "shot 3 has not ended");
    pending_reply(&seen, 3).done();
    let ended = third.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(ended, ReplyStatus::Done, "shot 3 ended");
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    assert_ne!(wakes(&count_2), 0, "the caller on shot 2 is woken");
    assert_ne!(wakes(&count_4), 0, "the caller on shot 4 is woken");
}

/// An action, shared by the parts of every [`Hooked`] state, run once by the
/// next part polled.
type Hook = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// A shot's state whose reply runs the shared hook, if set, when polled:
/// so a test acts in the middle of another reply's poll.
struct Hooked {
    senders: Rc<RefCell<Vec<ReplySender<ShotOutcome>>>>,
    hook: Hook,
}

/// The reply of a [`Hooked`] state.
struct HookedPart {
    inner: IncrementalReply<ShotOutcome>,
    hook: Hook,
}

impl Reply for HookedPart {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        let hook = self.hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        self.inner.poll_events(out, waker)
    }
}

impl Executor<InstructionBatch<bool>> for Hooked {
    type Reply = HookedPart;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<HookedPart, Rejected> {
        let (sender, inner) = reply_channel();
        self.senders.borrow_mut().push(sender);
        Ok(HookedPart {
            inner,
            hook: Rc::clone(&self.hook),
        })
    }
}

/// A batch on shot 2 is being polled when an earlier one, on shot 1,
/// fails, recorded as its reply is polled: a driver's other task acts while
/// shot 2's part is polled, after the reply looked for an earlier failure. The
/// executor wakes the caller, whose next poll ends the reply.
#[test]
fn a_failure_recorded_while_a_later_reply_polls_wakes_its_caller() {
    let (senders, hook) = (Rc::<RefCell<Vec<_>>>::default(), Hook::default());
    let mut states = ShotStates::default();
    let mut call_on = |j| {
        let make = |_| {
            Ok(Hooked {
                senders: Rc::clone(&senders),
                hook: Rc::clone(&hook),
            })
        };
        let shots = ShotSet::one(j);
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        };
        states
            .call(batch, |_| Ok(()), make, drop)
            .expect("the shot runs")
    };
    let (mut first, mut second) = (call_on(1), call_on(2));
    let shot_1 = senders.borrow_mut().remove(0);
    *hook.borrow_mut() = Some(Box::new(move || {
        shot_1.fail("lost".into());
        let failed = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    }));
    let (count, waker) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    assert_ne!(wakes(&count), 0);
}

/// A batch on shot 2 read a qubit in its call and another since, and
/// ended; its caller polls it, holding an event of its own already. While
/// shot 2's part is polled, an earlier batch, on shot 1, fails and is
/// seen to. That poll is the reply's last, so it ends with the failure
/// there, and hands over nothing.
#[test]
fn a_failure_recorded_while_a_later_reply_ends_ends_it_without_its_outcomes() {
    let (senders, hook) = (Rc::<RefCell<Vec<_>>>::default(), Hook::default());
    let mut states = ShotStates::default();
    let mut call_on = |j| {
        let make = |_| {
            Ok(Hooked {
                senders: Rc::clone(&senders),
                hook: Rc::clone(&hook),
            })
        };
        let shots = ShotSet::one(j);
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        };
        states
            .call(batch, |_| Ok(()), make, drop)
            .expect("the shot runs")
    };
    let mut first = call_on(1);
    let in_call = Rc::clone(&senders);
    *hook.borrow_mut() = Some(Box::new(move || in_call.borrow()[1].push(read(2, 5))));
    let mut second = call_on(2);
    let shot_2 = senders.borrow_mut().remove(1);
    shot_2.push(read(2, 7));
    shot_2.done();
    let shot_1 = senders.borrow_mut().remove(0);
    *hook.borrow_mut() = Some(Box::new(move || {
        shot_1.fail("lost".into());
        let failed = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    }));
    let held = ShotOutcome::new(9, 0, Value::One);
    let mut out = vec![held];
    let ended = second.poll_events(&mut out, Waker::noop());
    let reason = ReplyStatus::Failed("an earlier call failed: lost".into());
    assert_eq!((ended, out), (reason, vec![held]));
}

/// Shot 3's part failed, and while the executor polls it, an earlier
/// batch, on shot 1, fails, recorded as its reply is polled: that poll is the
/// reply's last, so it ends there with the earlier failure, recorded first, not
/// its own, as it would at any later poll.
#[test]
fn a_failure_recorded_while_a_failed_later_reply_ends_ends_it_with_that_failure() {
    let (senders, hook) = (Rc::<RefCell<Vec<_>>>::default(), Hook::default());
    let mut states = ShotStates::default();
    let mut call_on = |j| {
        let make = |_| {
            Ok(Hooked {
                senders: Rc::clone(&senders),
                hook: Rc::clone(&hook),
            })
        };
        let shots = ShotSet::one(j);
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        };
        states
            .call(batch, |_| Ok(()), make, drop)
            .expect("the shot runs")
    };
    let (mut first, mut third) = (call_on(1), call_on(3));
    let shot_3 = senders.borrow_mut().remove(1);
    shot_3.fail("lost later".into());
    let shot_1 = senders.borrow_mut().remove(0);
    *hook.borrow_mut() = Some(Box::new(move || {
        shot_1.fail("lost".into());
        let failed = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    }));
    let ended = third.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        ended,
        ReplyStatus::Failed("an earlier call failed: lost".into())
    );
}

/// A shot's state whose reply ends by its sender, or, made `quiet`, stays
/// pending and keeps no waker: so the only clone of a caller's waker is the
/// one the executor lists to wake on a failure.
struct QuietOrNot {
    senders: Rc<RefCell<Vec<ReplySender<ShotOutcome>>>>,
    quiet: bool,
}

/// The reply of a [`QuietOrNot`] state.
struct QuietOrNotPart(Option<IncrementalReply<ShotOutcome>>);

impl Reply for QuietOrNotPart {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        match &mut self.0 {
            Some(inner) => inner.poll_events(out, waker),
            None => ReplyStatus::Pending,
        }
    }
}

impl Executor<InstructionBatch<bool>> for QuietOrNot {
    type Reply = QuietOrNotPart;

    fn call(&mut self, _: InstructionBatchRef<'_, bool>) -> Result<QuietOrNotPart, Rejected> {
        if self.quiet {
            return Ok(QuietOrNotPart(None));
        }
        let (sender, inner) = reply_channel();
        self.senders.borrow_mut().push(sender);
        Ok(QuietOrNotPart(Some(inner)))
    }
}

/// The data of a hand-made waker: a `Waker` is `Send` and `Sync`, so its
/// vtable may run on any thread, and the data it reads must be `Sync`.
fn waker_data<T: Sync>(data: &'static T) -> *const () {
    ptr::from_ref(data).cast()
}

/// A waker of a hand-made vtable that counts its wakes and runs its hook, if
/// set, at its next clone: so a test acts while a reply takes the waker.
struct HookedClone {
    hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    wakes: AtomicUsize,
}

impl HookedClone {
    const VTABLE: RawWakerVTable =
        RawWakerVTable::new(Self::clone_raw, Self::wake_raw, Self::wake_raw, |_| {});

    /// A waker over `uses`, which the test keeps to read its wakes.
    fn waker(uses: &'static Self) -> Waker {
        // SAFETY: the data is leaked, so it outlives every clone, and it is
        // `Sync` (`waker_data`), so the vtable may run on any thread.
        unsafe { Waker::from_raw(RawWaker::new(waker_data(uses), &Self::VTABLE)) }
    }

    fn clone_raw(data: *const ()) -> RawWaker {
        // SAFETY: `data` is the leaked value that `waker` was given.
        let uses = unsafe { &*data.cast::<Self>() };
        // Taken out before it runs, so that it runs without the lock.
        let hook = uses
            .hook
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(hook) = hook {
            hook();
        }
        RawWaker::new(data, &Self::VTABLE)
    }

    fn wake_raw(data: *const ()) {
        // SAFETY: as in `clone_raw`.
        let uses = unsafe { &*data.cast::<Self>() };
        uses.wakes.fetch_add(1, Ordering::Relaxed);
    }
}

/// A batch on shot 2 is left pending, and its caller's waker runs code
/// of its own when cloned: while the executor takes the waker, an earlier
/// batch, on shot 1, fails, recorded as its reply is polled. The failure came
/// before the waker was listed, so it cannot wake it: the reply wakes its
/// caller itself.
#[test]
fn a_failure_recorded_while_a_reply_takes_its_waker_wakes_its_caller() {
    ends_soon(|| {
        let senders = Rc::<RefCell<Vec<_>>>::default();
        let mut states = ShotStates::default();
        let mut call_on = |j| {
            let make = |_| {
                Ok(QuietOrNot {
                    senders: Rc::clone(&senders),
                    quiet: j == 2,
                })
            };
            let shots = ShotSet::one(j);
            let batch = InstructionBatchRef {
                shots: &shots,
                instructions: &[true],
            };
            states
                .call(batch, |_| Ok(()), make, drop)
                .expect("the shot runs")
        };
        let (mut first, mut second) = (call_on(1), call_on(2));
        let shot_1 = senders.borrow_mut().remove(0);
        let uses: &'static HookedClone = Box::leak(Box::new(HookedClone {
            hook: Mutex::default(),
            wakes: AtomicUsize::new(0),
        }));
        *uses.hook.lock().expect("no hook ran yet") = Some(Box::new(move || {
            shot_1.fail("lost".into());
            let failed = first.poll_events(&mut Vec::new(), Waker::noop());
            assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
        }));
        let waiting = second.poll_events(&mut Vec::new(), &HookedClone::waker(uses));
        assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
        assert_ne!(uses.wakes.load(Ordering::Relaxed), 0);
    });
}

/// A batch on shot 3 ends on the failure of an earlier one, on shot 2,
/// seen while shot 3's part was polled; then a still earlier one, on shot 1,
/// fails: the executor does not wake the caller, who has moved on.
#[test]
fn a_reply_ended_by_an_earlier_failure_is_not_woken_by_a_still_earlier_one() {
    let (senders, hook) = (Rc::<RefCell<Vec<_>>>::default(), Hook::default());
    let mut states = ShotStates::default();
    let mut call_on = |j| {
        let make = |_| {
            Ok(Hooked {
                senders: Rc::clone(&senders),
                hook: Rc::clone(&hook),
            })
        };
        let shots = ShotSet::one(j);
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        };
        states
            .call(batch, |_| Ok(()), make, drop)
            .expect("the shot runs")
    };
    let (mut first, mut second, mut third) = (call_on(1), call_on(2), call_on(3));
    let shot_2 = senders.borrow_mut().remove(1);
    *hook.borrow_mut() = Some(Box::new(move || {
        shot_2.fail("lost".into());
        let failed = second.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 2 failed");
    }));
    let (count, waker) = counting();
    let waiting = third.poll_events(&mut Vec::new(), &waker);
    assert_eq!(
        waiting,
        ReplyStatus::Pending,
        "shot 3's caller is woken to end it"
    );
    let ended = third.poll_events(&mut Vec::new(), &waker);
    let reason = ReplyStatus::Failed("an earlier call failed: lost".into());
    assert_eq!(ended, reason, "shot 2's failure ended shot 3's reply");
    let woken = wakes(&count);
    let shot_1 = senders.borrow_mut().remove(0);
    shot_1.fail("lost earlier".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    let reason = ReplyStatus::Failed("lost earlier".into());
    assert_eq!(failed, reason, "shot 1 failed");
    assert_eq!(wakes(&count), woken);
}

/// Shot 3's part fails, and while the executor polls it, an earlier
/// batch, on shot 1, fails, recorded first: the later failure does
/// not hide the earlier one from the batch on shot 2 between them,
/// which ends.
#[test]
fn a_later_failure_recorded_after_an_earlier_one_still_ends_the_batches_between() {
    let (senders, hook) = (Rc::<RefCell<Vec<_>>>::default(), Hook::default());
    let mut states = ShotStates::default();
    let mut call_on = |j| {
        let make = |_| {
            Ok(Hooked {
                senders: Rc::clone(&senders),
                hook: Rc::clone(&hook),
            })
        };
        let shots = ShotSet::one(j);
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        };
        states
            .call(batch, |_| Ok(()), make, drop)
            .expect("the shot runs")
    };
    let (mut first, mut second, mut third) = (call_on(1), call_on(2), call_on(3));
    let shot_3 = senders.borrow_mut().remove(2);
    shot_3.fail("lost later".into());
    let shot_1 = senders.borrow_mut().remove(0);
    *hook.borrow_mut() = Some(Box::new(move || {
        shot_1.fail("lost".into());
        let failed = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    }));
    let failed = third.poll_events(&mut Vec::new(), Waker::noop());
    let reason = ReplyStatus::Failed("an earlier call failed: lost".into());
    assert_eq!(
        failed, reason,
        "shot 1's failure, recorded first, ended shot 3's reply"
    );
    let ended = second.poll_events(&mut Vec::new(), Waker::noop());
    let reason = ReplyStatus::Failed("an earlier call failed: lost".into());
    assert_eq!(ended, reason);
}

/// A waker of a hand-made vtable whose every clone and every drop drops one
/// of the replies it holds: such a waker runs code of its own on each.
struct DropsAReplyPerUse(Mutex<Vec<Box<dyn Any + Send>>>);

impl DropsAReplyPerUse {
    const VTABLE: RawWakerVTable =
        RawWakerVTable::new(Self::clone_raw, Self::drop_one, |_| {}, Self::drop_one);

    /// A waker that holds `replies`, and leaks those it has not dropped.
    fn waker(replies: Vec<Box<dyn Any + Send>>) -> Waker {
        let uses: &'static Self = Box::leak(Box::new(Self(Mutex::new(replies))));
        // SAFETY: the data is never freed, so it outlives every clone, and
        // it is `Sync` (`waker_data`), so the vtable may run on any thread.
        unsafe { Waker::from_raw(RawWaker::new(waker_data(uses), &Self::VTABLE)) }
    }

    fn clone_raw(data: *const ()) -> RawWaker {
        Self::drop_one(data);
        RawWaker::new(data, &Self::VTABLE)
    }

    fn drop_one(data: *const ()) {
        // SAFETY: `data` is the leaked value that `waker` made.
        let uses = unsafe { &*data.cast::<Self>() };
        // Taken out before it is dropped, so that it is dropped without the
        // lock: it may drop a clone of this waker, which drops another.
        let reply = uses.0.lock().unwrap_or_else(PoisonError::into_inner).pop();
        drop(reply);
    }
}

/// Batches on shots 1 to 9, each polled once, so that each reply's
/// waker is registered: the first reply, and the others for a waker to hold.
fn nine_waiting_replies(
    executor: &mut TestDoubleExecutor<TestDoubleModel>,
) -> (impl Reply + 'static, Vec<Box<dyn Any + Send>>) {
    let mut replies = (1..=9).map(|j| {
        let mut reply = executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot");
        let waiting = reply.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(waiting, ReplyStatus::Pending, "shot {j} has not ended");
        reply
    });
    let first = replies.next().expect("shot 1 was called");
    let others = replies.map(|reply| Box::new(reply) as Box<dyn Any + Send>);
    (first, others.collect())
}

/// A waker that drops the reply it owns when woken.
struct DropsOnWake<R>(Mutex<Option<R>>);

impl<R: Send + 'static> Wake for DropsOnWake<R> {
    fn wake(self: Arc<Self>) {
        drop(self.0.lock().expect("no wake panicked").take());
    }
}

/// A waker that polls the reply it owns when woken, and keeps the status.
struct PollsOnWake<R>(Mutex<(Option<R>, Option<ReplyStatus>)>);

impl<R: Reply + Send + 'static> Wake for PollsOnWake<R> {
    fn wake(self: Arc<Self>) {
        let mut polled = self.0.lock().expect("no wake panicked");
        if let Some(mut reply) = polled.0.take() {
            polled.1 = Some(reply.poll_events(&mut Vec::new(), Waker::noop()));
        }
    }
}

/// A reply's caller polls it with a waker whose every clone and every drop
/// drops another waiting reply of the same executor: the executor takes the
/// waker without waiting on itself.
#[test]
fn a_reply_takes_a_waker_that_drops_a_reply_of_its_executor_on_each_use() {
    ends_soon(|| {
        let (mut executor, _seen, _) = executor(TestDoubleModel {
            deferred: (1..=9).collect(),
            ..TestDoubleModel::default()
        });
        let (mut first, others) = nine_waiting_replies(&mut executor);
        let waker = DropsAReplyPerUse::waker(others);
        let waiting = first.poll_events(&mut Vec::new(), &waker);
        assert_eq!(waiting, ReplyStatus::Pending, "shot 1 has not ended");
    });
}

/// A reply's caller polls it with a waker whose every clone and every drop
/// drops another waiting reply of the same executor, then polls it with
/// another waker: the executor lets go of the old waker without waiting on
/// itself.
#[test]
fn a_reply_lets_go_of_a_waker_that_drops_a_reply_of_its_executor_on_each_use() {
    ends_soon(|| {
        let (mut executor, _seen, _) = executor(TestDoubleModel {
            deferred: (1..=9).collect(),
            ..TestDoubleModel::default()
        });
        let (mut first, others) = nine_waiting_replies(&mut executor);
        let waker = DropsAReplyPerUse::waker(others);
        let waiting = first.poll_events(&mut Vec::new(), &waker);
        assert_eq!(waiting, ReplyStatus::Pending, "shot 1 has not ended");
        drop(waker);
        let waiting = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(waiting, ReplyStatus::Pending, "shot 1 has still not ended");
    });
}

/// A reply's caller polls it with a waker whose every clone and every drop
/// drops another waiting reply of the same executor, then the reply ends:
/// the executor lets go of the waker without waiting on itself.
#[test]
fn an_ended_reply_lets_go_of_a_waker_that_drops_a_reply_of_its_executor_on_each_use() {
    ends_soon(|| {
        let (mut executor, seen, _) = executor(TestDoubleModel {
            deferred: (1..=9).collect(),
            ..TestDoubleModel::default()
        });
        let (mut first, others) = nine_waiting_replies(&mut executor);
        let waker = DropsAReplyPerUse::waker(others);
        let waiting = first.poll_events(&mut Vec::new(), &waker);
        assert_eq!(waiting, ReplyStatus::Pending, "shot 1 has not ended");
        drop(waker);
        pending_reply(&seen, 1).done();
        let ended = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(ended, ReplyStatus::Done, "shot 1 ended");
    });
}

/// An earlier batch's failure, on shot 1, wakes the caller of a
/// batch on shot 2 with a waker that drops a third reply of the same
/// executor: the executor wakes it without waiting on itself.
#[test]
fn a_failure_wakes_a_waker_that_drops_another_reply_of_its_executor() {
    ends_soon(|| {
        let (mut executor, seen, _) = executor(TestDoubleModel {
            deferred: vec![1, 2, 3],
            ..TestDoubleModel::default()
        });
        let mut call_on = |j| {
            executor
                .call(InstructionBatchRef {
                    shots: &ShotSet::one(j),
                    instructions: &[true],
                })
                .expect("the executor runs the shot")
        };
        let (mut first, mut second, mut third) = (call_on(1), call_on(2), call_on(3));
        let waiting = third.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(waiting, ReplyStatus::Pending, "shot 3 has not ended");
        let dropper = Waker::from(Arc::new(DropsOnWake(Mutex::new(Some(third)))));
        let waiting = second.poll_events(&mut Vec::new(), &dropper);
        assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
        pending_reply(&seen, 1).fail("lost".into());
        let failed = first.poll_events(&mut Vec::new(), Waker::noop());
        assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    });
}

/// A caller drops its pending reply on shot 2, then an earlier batch,
/// on shot 1, fails: the executor lets go of the dropped reply's waker, and
/// does not wake it.
#[test]
fn a_dropped_reply_is_not_woken_by_an_earlier_failure() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2],
        ..TestDoubleModel::default()
    });
    let mut call_on = |j| {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(j),
                instructions: &[true],
            })
            .expect("the executor runs the shot")
    };
    let (mut first, mut second) = (call_on(1), call_on(2));
    let (count, waker) = counting();
    let waiting = second.poll_events(&mut Vec::new(), &waker);
    assert_eq!(waiting, ReplyStatus::Pending, "shot 2 has not ended");
    drop(second);
    pending_reply(&seen, 1).fail("lost".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("lost".into()), "shot 1 failed");
    assert_eq!(wakes(&count), 0);
}

/// Failures seen out of order keep the earliest batch's reason: the
/// later one ran on the state the earlier one left.
#[test]
fn the_earliest_failed_batch_is_the_executors_failure() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut call_on_1 = || {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(1),
                instructions: &[false],
            })
            .expect("the executor runs shot 1")
    };
    let (mut first, mut second) = (call_on_1(), call_on_1());
    let (first_reply, second_reply) = (pending_reply(&seen, 1), pending_reply(&seen, 1));
    second_reply.fail("second".into());
    let failed = second.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        failed,
        ReplyStatus::Failed("second".into()),
        "call 2 failed"
    );
    first_reply.fail("first".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("first".into()), "call 1 failed");
    assert_eq!(
        run(&mut executor, &ShotSet::one(2), &[true]).err(),
        Some(Rejected::EarlierFailure("first".into()))
    );
}

/// Calls 1 to 3 run on shot 1; call 3 fails first, then call 1: call 2,
/// still running, ends with call 1's failure, the earliest.
#[test]
fn a_batch_between_two_failed_ones_ends_with_the_earliest_failure() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1],
        ..TestDoubleModel::default()
    });
    let mut call_on_1 = || {
        executor
            .call(InstructionBatchRef {
                shots: &ShotSet::one(1),
                instructions: &[false],
            })
            .expect("the executor runs shot 1")
    };
    let (mut first, mut second, mut third) = (call_on_1(), call_on_1(), call_on_1());
    let [first_reply, _second_reply, third_reply] = [1, 2, 3].map(|_| pending_reply(&seen, 1));
    third_reply.fail("third".into());
    let failed = third.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("third".into()), "call 3 failed");
    first_reply.fail("first".into());
    let failed = first.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(failed, ReplyStatus::Failed("first".into()), "call 1 failed");
    assert_eq!(
        second.poll_events(&mut Vec::new(), Waker::noop()),
        ReplyStatus::Failed("an earlier call failed: first".into())
    );
}

/// A batch that failed at a later shot ends its reply only once the
/// shots that ran have ended: their outcomes come first, in their order,
/// then the failure.
#[test]
fn a_later_shots_rejection_ends_the_reply_after_the_shots_that_ran() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![5],
        refusing: Some(7),
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[5, 2, 7]),
            instructions: &[true, true],
        })
        .expect("shot 5, called first, accepts: shot 7 refuses too late");
    let mut out = Vec::new();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Pending,
        "shot 5 is still running"
    );
    pending_reply(&seen, 5).done();
    assert_eq!(
        reply.poll_events(&mut out, Waker::noop()),
        ReplyStatus::Failed("shot 7: unavailable: busy".into())
    );
    assert_eq!(
        out.iter().map(|o| (o.shot, o.tag)).collect::<Vec<_>>(),
        [(5, 0), (5, 1), (2, 0), (2, 1)]
    );
}

/// Shots 1 and 2 of one call both fail, 1 first: the reply ends with
/// shot 1's failure, and the executor's refusal names the same.
#[test]
fn the_executors_refusal_names_the_failure_its_reply_ended_with() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![1, 2],
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[1, 2]),
            instructions: &[false],
        })
        .expect("the executor runs shots 1 and 2");
    pending_reply(&seen, 1).fail("first".into());
    pending_reply(&seen, 2).fail("second".into());
    let ended = reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        ended,
        ReplyStatus::Failed("first".into()),
        "the reply's end"
    );
    assert_eq!(
        run(&mut executor, &ShotSet::one(3), &[true]).err(),
        Some(Rejected::EarlierFailure("first".into()))
    );
}

/// When a batch failed at a later shot, and a shot that ran fails
/// too, the reply ends with the batch's failure: it came first, when
/// the executor called the shots.
#[test]
fn a_reply_ends_with_the_later_shots_rejection_when_a_shot_that_ran_fails_too() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![5],
        refusing: Some(7),
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[5, 7]),
            instructions: &[true],
        })
        .expect("shot 5, called first, accepts: shot 7 refuses too late");
    pending_reply(&seen, 5).fail("lost".into());
    assert_eq!(
        reply.poll_events(&mut Vec::new(), Waker::noop()),
        ReplyStatus::Failed("shot 7: unavailable: busy".into())
    );
}

/// When a batch failed at a later shot, and a shot that ran fails
/// too, the executor's refusal names the batch's failure: it was
/// recorded first, when the executor called the shots.
#[test]
fn the_executors_refusal_names_the_later_shots_rejection_when_a_shot_that_ran_fails_too() {
    let (mut executor, seen, _) = executor(TestDoubleModel {
        deferred: vec![5],
        refusing: Some(7),
        ..TestDoubleModel::default()
    });
    let mut reply = executor
        .call(InstructionBatchRef {
            shots: &shot_set(&[5, 7]),
            instructions: &[true],
        })
        .expect("shot 5, called first, accepts: shot 7 refuses too late");
    pending_reply(&seen, 5).fail("lost".into());
    reply.poll_events(&mut Vec::new(), Waker::noop());
    assert_eq!(
        run(&mut executor, &ShotSet::one(5), &[true]).err(),
        Some(Rejected::EarlierFailure("shot 7: unavailable: busy".into()))
    );
}

// Closing.

/// A closed executor makes no state: nothing is made for a shot that
/// cannot run.
#[test]
fn a_closed_executor_makes_no_state() {
    let (mut executor, _, made) = executor(TestDoubleModel::default());
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    executor.close();
    run(&mut executor, &ShotSet::one(9), &[true]).expect_err("the executor is closed");
    assert_eq!(made.get(), 1, "none was made for shot 9");
}

/// A closed executor refuses every call with `Closed`, whatever the batch,
/// even one that its own check refuses.
#[test]
fn a_closed_executor_says_closed_whatever_the_batch() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    executor.close();
    for (shots, batch) in [(ShotSet::one(1), [true]), (ShotSet::one(9), [false])] {
        assert_eq!(
            run(&mut executor, &shots, &batch).err(),
            Some(Rejected::Closed)
        );
    }
}

/// An executor closed after it failed refuses every call with `Closed`,
/// not with its failure: it was closed last.
#[test]
fn a_closed_executor_says_closed_even_after_it_failed() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    run(&mut executor, &shot_set(&[1, 2]), &[true])
        .expect("shot 1, called first, accepts: shot 2 refuses too late")
        .1
        .expect_err("shot 2's refusal fails the executor");
    executor.close();
    assert_eq!(
        run(&mut executor, &ShotSet::one(1), &[true]).err(),
        Some(Rejected::Closed)
    );
}

/// The report counts the calls the executor accepted, not the ones it
/// refused.
#[test]
fn the_report_counts_the_accepted_calls() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    run(&mut executor, &ShotSet::one(2), &[true]).expect_err("shot 2 refuses");
    step(
        &mut executor,
        &ShotSet::one(1),
        &[true],
        "shot 1 runs again",
    );
    assert_eq!(executor.close().calls, 2);
}

/// A call accepted, then failed by a later shot's rejection, was accepted:
/// the report counts it.
#[test]
fn the_report_counts_a_call_that_failed_after_it_was_accepted() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        ..TestDoubleModel::default()
    });
    let (_, end) = run(&mut executor, &shot_set(&[1, 2]), &[true]).expect("shot 1 accepts");
    end.expect_err("shot 2 refuses too late");
    assert_eq!(executor.close().calls, 1);
}

/// A call refused because a state could not be made was not accepted: the
/// report does not count it.
#[test]
fn the_report_does_not_count_a_call_refused_because_a_state_could_not_be_made() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        unavailable_once: Cell::new(Some(1)),
        ..TestDoubleModel::default()
    });
    run(&mut executor, &ShotSet::one(1), &[true]).expect_err("shot 1's memory is in use");
    step(
        &mut executor,
        &ShotSet::one(1),
        &[true],
        "shot 1's memory is free now",
    );
    assert_eq!(executor.close().calls, 1);
}

/// A batch the executor's check refuses was not accepted: the report does
/// not count it.
#[test]
fn the_report_does_not_count_a_call_the_check_refuses() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refuses_false: true,
        ..TestDoubleModel::default()
    });
    run(&mut executor, &ShotSet::one(1), &[false])
        .expect_err("the check refuses a batch holding false");
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    assert_eq!(executor.close().calls, 1);
}

/// Closing folds the report of each shot's state: shot 1 ran one call,
/// shot 2 two, three in all.
#[test]
fn closing_folds_the_report_of_each_shots_state() {
    let (mut executor, _, _) = executor(TestDoubleModel::default());
    step(
        &mut executor,
        &shot_set(&[1, 2]),
        &[true],
        "shots 1 and 2 run",
    );
    step(&mut executor, &ShotSet::one(2), &[true], "shot 2 runs");
    assert_eq!(executor.close().states, Count(3));
}

/// Closing gives each shot the log probability of its own state: shot 1
/// ran one call (ln p = -1), shot 2 two (-2).
#[test]
fn closing_gives_each_shot_its_states_log_probability() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        probabilities: true,
        ..TestDoubleModel::default()
    });
    step(
        &mut executor,
        &shot_set(&[1, 2]),
        &[true],
        "shots 1 and 2 run",
    );
    step(&mut executor, &ShotSet::one(2), &[true], "shot 2 runs");
    let report = executor.close();
    assert_eq!(
        (report.log_p.get(1), report.log_p.get(2)),
        (Some(-1.0), Some(-2.0))
    );
}

/// Closing drops the states: none is left to read.
#[test]
fn closing_drops_the_states() {
    let (mut executor, _, _) = executor(TestDoubleModel::default());
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    executor.close();
    assert!(executor.state(1).is_none(), "shot 1's state is left");
}

/// An executor closed twice reports its calls and states once: the
/// second report is empty.
#[test]
fn a_second_close_reports_nothing() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        probabilities: true,
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    conformance::reports_once(&mut executor);
}

/// A call failed by a later shot's rejection reports no state made for it,
/// as when its batch fails: not shot 1's, which ran, nor shot 2's, which
/// refused, nor shot 3's, which never ran; shot 4's, made by an earlier
/// call, is reported. Chosen for the same report in every form of the
/// executor.
#[test]
fn a_call_a_later_shot_refused_reports_no_state_made_for_it() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        refusing: Some(2),
        probabilities: true,
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(4), &[true], "shot 4 runs");
    let (_, end) = run(&mut executor, &shot_set(&[4, 1, 2, 3]), &[true]).expect("shot 4 accepts");
    end.expect_err("shot 2 refuses too late");
    let report = executor.close();
    let log_p = [4, 1, 2, 3].map(|j| report.log_p.get(j));
    assert_eq!(log_p, [Some(-2.0), None, None, None]);
}

/// Shot 1 runs, then a batch on shots 1 and 2 fails at shot 2: the failed
/// executor still holds both states, as their calls left them, until it
/// closes. Keeping them is the crate's choice, not the protocol's: it can
/// change to dropping them at the failure, when a case needs their memory
/// back sooner, such as a state of gigabytes.
#[test]
fn a_failed_executor_keeps_its_states_until_close() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(2),
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    let (_, end) = run(&mut executor, &shot_set(&[1, 2]), &[true]).expect("shot 1 accepts");
    end.expect_err("shot 2's batch fails after it was accepted");
    let kept = [1, 2].map(|j| executor.state(j).map(TestDoubleState::calls));
    assert_eq!(kept, [Some(2), Some(1)], "after the failure");
    executor.close();
    let kept = [1, 2].map(|j| executor.state(j).is_some());
    assert_eq!(kept, [false, false], "after close");
}

/// Shot 1 runs, then a batch on shots 1 and 2 fails at shot 2: closing the
/// failed executor folds the report of shot 1's state, made by the earlier
/// call, which ran two calls; not shot 2's, made for the failed call. A
/// failed call reports no state made for it, so that every form of the
/// executor reports the same states.
#[test]
fn a_failed_executor_folds_only_the_reports_of_states_made_before_the_failed_call() {
    let (mut executor, _, _) = executor(TestDoubleModel {
        failing: Some(2),
        ..TestDoubleModel::default()
    });
    step(&mut executor, &ShotSet::one(1), &[true], "shot 1 runs");
    let (_, end) = run(&mut executor, &shot_set(&[1, 2]), &[true]).expect("shot 1 accepts");
    end.expect_err("shot 2's batch fails after it was accepted");
    assert_eq!(executor.close().states, Count(2));
}

/// An executor whose replies all end inside `call` runs a batch on the states
/// it keeps without allocating memory: no failure can come after the call,
/// so it shares nothing with the reply.
#[test]
fn a_call_whose_replies_end_inside_it_allocates_nothing_on_kept_states() {
    /// A shot's state whose replies end at once, with no outcome.
    struct Quiet;
    impl Executor<InstructionBatch<bool>> for Quiet {
        type Reply = ImmediateReply<ShotOutcome>;

        fn call(
            &mut self,
            _: InstructionBatchRef<'_, bool>,
        ) -> Result<ImmediateReply<ShotOutcome>, Rejected> {
            Ok(ImmediateReply::default())
        }
    }
    let (mut states, shots) = (ShotStates::default(), shot_set(&[1, 2]));
    let mut run = || {
        let batch = InstructionBatchRef {
            shots: &shots,
            instructions: &[true],
        };
        let mut reply =
            (states.call(batch, |_| Ok(()), |_| Ok(Quiet), drop)).expect("the shots run");
        reply.poll_events(&mut Vec::new(), Waker::noop())
    };
    assert_eq!(run(), ReplyStatus::Done, "shots 1 and 2 made their states");
    let mut ended = None;
    let made = crate::tests::allocations(|| ended = Some(run()));
    assert_eq!(ended, Some(ReplyStatus::Done), "shots 1 and 2 ran again");
    assert_eq!(made, 0);
}

/// `wait` on an executor whose shots' states run on threads of their own
/// polls at most twice per shot (its events, its end), and twice more (the
/// first poll, the last): the executor wakes its caller only when a shot
/// has something for it.
#[test]
fn wait_on_an_executor_polls_at_most_twice_per_shot() {
    let parked = ends_soon(|| {
        let shots = ShotSet::range(1..=4).expect("1..=4 is not empty");
        let mut executor = TestDoubleExecutor::with_model(Workers, 0);
        let (reply, polls) = counted(
            executor
                .call(InstructionBatchRef {
                    shots: &shots,
                    instructions: &[],
                })
                .expect("the executor runs shots 1 to 4"),
        );
        let mut out = Vec::new();
        assert_eq!(wait(reply, &mut out), Ok(()), "the shots end");
        assert_eq!(out.len(), 4, "each shot gives one outcome");
        polls.load(Ordering::SeqCst)
    });
    assert!(parked <= 2 * 4 + 2, "{parked} polls for 4 shots");
}
