// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checks of the rules of an executor's life: a closed executor rejects
//! every call, a refusal leaves it usable, and a failure is final and ends
//! the later calls still in flight.

use std::task::Waker;

use crate::reply::blocking::wait_status;
use crate::{Close, Executor, Message, Rejected, Reply, ReplyStatus, RetryClass, wait};

/// Closes `executor`, then calls it with `message`: the call must be
/// rejected with [`Rejected::Closed`]. `message` should be one the executor
/// accepts while open, so that the rejection can only be the close's.
///
/// It checks only the answer. A closed executor must also do no work (an
/// adapter runs no logic and makes no state, though the closed executor
/// below would refuse what it sends), which the implementer's own tests
/// must pin.
///
/// # Panics
///
/// When the call is accepted, or rejected for another reason.
pub fn rejects_calls_once_closed<M: Message, E: Executor<M> + Close>(
    executor: &mut E,
    message: M::Ref<'_>,
) {
    executor.close();
    expect_closed(executor.call(message), "a call after close");
}

/// The rule of [`Close`] that both [`rejects_calls_once_closed`] and
/// [`refuses_calls_after_a_failure`] check: `answer`, the answer to `call`
/// made once the executor is closed, must be a rejection with
/// [`Rejected::Closed`].
///
/// # Panics
///
/// When `answer` is an accepted call, or a rejection for another reason;
/// the panic message starts with `call`.
fn expect_closed<R>(answer: Result<R, Rejected>, call: &str) {
    match answer {
        Err(Rejected::Closed) => {}
        Err(other) => panic!("{call} must be rejected with `Closed`, not `{other:?}`"),
        Ok(_) => panic!("{call} must be rejected with `Closed`, but it was accepted"),
    }
}

/// Calls `executor` with `refused`, a message it refuses for a fault of the
/// message's own (an instruction outside its set, a qubit it does not
/// have), then with `accepted`: the refusal must not be
/// [`Rejected::EarlierFailure`] or [`Rejected::Closed`], which would say that
/// the executor ended, and `accepted` must then be accepted and its reply
/// end well, since a refusal means that nothing ran, so the caller can
/// correct its call and send it again. `refused` must be refused with a
/// class that says so, not [`RetryClass::Resend`], after which the work may
/// have run. `accepted` should be a message the executor runs to its end
/// before the refusal, so that a refusal or a failure of it can only be the
/// earlier refusal's doing. The rule also asks that the refused message
/// changed no state, which this check cannot see; nor does it check that
/// `accepted`'s reply delivers its events, since it cannot know a message's
/// events, only that the reply ends well.
///
/// # Panics
///
/// When `refused` is accepted, or refused with a class of `Resend` (this
/// check needs a refusal after which nothing ran), or refused with
/// `EarlierFailure` or `Closed`; when `accepted` is then refused, or its
/// reply fails.
pub fn accepts_calls_after_a_refusal<M: Message, E: Executor<M>>(
    executor: &mut E,
    refused: M::Ref<'_>,
    accepted: M::Ref<'_>,
) {
    match executor.call(refused) {
        Err(rejected @ (Rejected::EarlierFailure(_) | Rejected::Closed)) => panic!(
            "a call refused for its own fault must not be refused with `{rejected:?}`, which \
             says that the executor ended"
        ),
        Err(rejected) if rejected.retry() == RetryClass::Resend => panic!(
            "the refused message must be refused with a class that says that nothing ran, not \
             `{rejected:?}`, after which its work may have run"
        ),
        Err(_) => {}
        Ok(_) => panic!("the refused message must be refused, so that this check sees a refusal"),
    }
    match executor.call(accepted) {
        Ok(reply) => {
            if let Err(reason) = wait(reply, &mut Vec::new()) {
                panic!("a call after a refusal must run, but its reply failed: `{reason}`");
            }
        }
        Err(rejected) => panic!(
            "a call after a refusal must be accepted, but it was refused with `{rejected:?}`"
        ),
    }
}

/// Calls `executor`, an executor whose state carries over from one call to
/// the next, with `failing`, a message whose work fails once started, and
/// waits for its reply, which must end failed ([`ReplyStatus::Failed`],
/// not cancelled, which says that the work was stopped from outside),
/// saying why; then calls it
/// with each message of `later`, in order: each call must be refused with
/// [`Rejected::EarlierFailure`], whose reason contains the failure's, since
/// the failure left the executor's state part-way through `failing`, and
/// the caller learns which failure ended its run. Then closes `executor` and
/// calls it with `later[0]` again: the call must be refused with
/// [`Rejected::Closed`], which comes first, since a caller may not call a
/// closed executor at all. `later` should hold messages the executor accepts
/// before the failure, so that a refusal can only be the failure's; for an
/// executor of several shots, some on shots that `failing` did not name,
/// since the failure ends the whole executor. `failing`'s reply must end:
/// the check waits for it ([`wait`]), so it never returns if that reply
/// never ends.
///
/// # Panics
///
/// When `failing` is refused, or its reply does not end
/// [`ReplyStatus::Failed`] (it ends done, or cancelled), or fails with an
/// empty reason, which this check needs to name the failure; when `later`
/// is empty; when a message of `later` is accepted (the panic shows its
/// reply's first poll), or refused for another reason, or with a reason
/// that does not contain the failure's; when the call after close is not
/// refused with `Closed`.
pub fn refuses_calls_after_a_failure<M: Message, E: Executor<M> + Close>(
    executor: &mut E,
    failing: M::Ref<'_>,
    later: &[M::Ref<'_>],
) {
    assert!(!later.is_empty(), "`later` must hold a message to refuse");
    let reply = executor
        .call(failing)
        .expect("the failing message must be accepted, so that its work starts");
    let failure = failure_of(reply);
    for (k, &message) in later.iter().enumerate() {
        match executor.call(message) {
            Err(Rejected::EarlierFailure(reason)) => assert!(
                reason.contains(&failure),
                "a refusal after a failure must name the failure, `{failure}`, but the \
                 refusal of `later[{k}]` says `{reason}`"
            ),
            Err(other) => panic!(
                "a call after a failure must be rejected with `EarlierFailure`, not `{other:?}` \
                 (`later[{k}]`)"
            ),
            Ok(mut reply) => {
                // Polled once, not waited for: a reply that never ends must
                // not hang the check.
                let mut events = Vec::new();
                let status = reply.poll_events(&mut events, Waker::noop());
                panic!(
                    "a call after a failure must be rejected with `EarlierFailure`, but it was \
                     accepted (`later[{k}]`), and its reply, polled once, was `{status:?}` \
                     with {} events",
                    events.len()
                )
            }
        }
    }
    executor.close();
    expect_closed(executor.call(later[0]), "a call after a failure and close");
}

/// The reason why `reply`, the reply to the failing message of a check,
/// failed: it must end failed ([`ReplyStatus::Failed`], not cancelled,
/// which says that the work was stopped from outside), saying why, so that
/// a later call's refusal or failure can name it.
///
/// # Panics
///
/// When `reply` ends done or cancelled, or fails with an empty reason.
fn failure_of<R: Reply>(reply: R) -> String {
    let failure = match wait_status(reply, &mut Vec::new()) {
        ReplyStatus::Failed(reason) => reason,
        ReplyStatus::Cancelled => panic!(
            "the failing message's reply must end failed, not cancelled: a cancellation says \
             that its work was stopped from outside, not that it failed"
        ),
        end => panic!("the failing message's reply must end failed, not `{end:?}`"),
    };
    assert!(
        !failure.is_empty(),
        "the failing message's reply must say why it failed, so that a refusal can name it"
    );
    failure
}

/// Calls `executor`, an executor whose state carries over from one call to
/// the next, with `failing`, a message whose work fails once started, then
/// with `later`, before it waits for `failing`'s reply, which
/// must end failed ([`ReplyStatus::Failed`]), saying why: `later`, in
/// flight as the earlier call failed, must end at that failure, since it
/// would run on the state the failure left part-way through `failing`. It
/// is either refused with [`Rejected::EarlierFailure`], whose reason
/// contains the failure's, when the executor knew of the failure as it was
/// called (one that runs its calls inside `call`), or accepted, and then
/// its reply fails, with a reason that contains the failure's, and hands
/// over no event (one that runs its calls elsewhere, such as on a
/// placement, where `later` may even have run, on another shot). `later`
/// should be a message the executor accepts before the failure, so that a
/// refusal or a failure can only be the failure's; for an executor of
/// several shots, on shots that `failing` does not name, since the failure
/// ends the whole executor. It waits for both replies, so run it where a
/// reply that never ends shows (a test with a time limit).
///
/// # Panics
///
/// When `failing` is refused, or its reply does not end failed, or fails
/// with an empty reason; when `later` is refused for another reason, or
/// with a reason that does not contain the failure's; when `later` is
/// accepted and its reply ends done or cancelled, fails with a reason that
/// does not contain the failure's, or hands over an event.
pub fn ends_calls_in_flight_at_an_earlier_failure<M: Message, E: Executor<M>>(
    executor: &mut E,
    failing: M::Ref<'_>,
    later: M::Ref<'_>,
) {
    let failing = executor
        .call(failing)
        .expect("the failing message must be accepted, so that its work starts");
    let later = executor.call(later);
    let failure = failure_of(failing);
    match later {
        Err(Rejected::EarlierFailure(reason)) => assert!(
            reason.contains(&failure),
            "a refusal after a failure must name the failure, `{failure}`, but the refusal of \
             `later` says `{reason}`"
        ),
        Err(other) => panic!(
            "a call in flight at an earlier failure must be refused with `EarlierFailure`, or \
             end failed, not be refused with `{other:?}`"
        ),
        Ok(reply) => {
            let mut events = Vec::new();
            match wait_status(reply, &mut events) {
                ReplyStatus::Failed(reason) => assert!(
                    reason.contains(&failure),
                    "a call in flight at an earlier failure must end with that failure, \
                     `{failure}`, but its reply says `{reason}`"
                ),
                end => panic!(
                    "a call in flight at an earlier failure must end failed, with that \
                     failure, `{failure}`, not `{end:?}`"
                ),
            }
            assert!(
                events.is_empty(),
                "a call in flight at an earlier failure must hand over no event, since it would \
                 read the state the failure left, but it handed over {}",
                events.len()
            );
        }
    }
}
