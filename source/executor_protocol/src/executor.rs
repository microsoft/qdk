// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Executors ([`Executor`]).
//!
//! - [`report`]: how an executor is closed, the report it returns, and how
//!   reports fold.

pub(crate) mod report;

#[cfg(doc)]
use crate::{Capabilities, Close, Describe, InstructionBatch, ShotContext};
use crate::{Message, Rejected, Reply};

/// An executor of messages of kind `M`: one that runs them, or the executor an
/// adapter gives upstream of another. The crate's
/// `docs/internals/composition.md` lists the roles.
///
/// Its reply carries `M`'s events, so an executor cannot reply with another
/// message kind's events. The reply's type is the executor's own, not a boxed
/// one, so chaining executors adds no boxing or dynamic dispatch per component;
/// what a reply allocates is its own choice
/// ([`IncrementalReply`](crate::IncrementalReply) shares a buffer with its
/// producer, [`ImmediateReply`](crate::ImmediateReply) holds its events).
///
/// # Contract
///
/// Every executor a caller reaches, the last one of a chain or one an adapter
/// gives, keeps these rules, which the type system cannot check and
/// composition relies on:
///
/// - One reply per call: [`Executor::call`] returns the reply of this
///   message, or refuses it.
/// - A refusal means nothing ran: no state of the executor, nor of any
///   shot it runs, moved, so the call can be corrected and sent again
///   (the `conformance` module checks that the executor then takes a call,
///   `accepts_calls_after_a_refusal`). The exception is a refusal of class
///   [`RetryClass::Resend`](crate::RetryClass::Resend), such as
///   [`Rejected::NoReply`]: no verdict arrived over a link, so work may
///   have run ([`Executor::call`]).
/// - A failure once work has started fails the reply, not the call: the reply
///   ends with [`ReplyStatus::Failed`](crate::ReplyStatus::Failed). An executor
///   whose state carries over from one call to the next (a shot's state, or the
///   states of its shots) then refuses its later calls
///   ([`Rejected::EarlierFailure`]), since that state is no longer the one its
///   caller expects. A later call in flight as the failure happens ends with it
///   too once the failure is recorded as the executor's (on an
///   [`ExecutorLifecycle`](crate::ExecutorLifecycle): with `fail` when it
///   failed inside `call`, or by a reply bound with `bind` as that reply is
///   polled): refused, or failed with no more events, since it would run on
///   that state; a poll that runs as the failure is recorded may still leave
///   the reply pending and hand over that poll's own events, and the next poll
///   ends it (a known issue, noted at
///   [`LifecycleBoundReply`](crate::LifecycleBoundReply)'s source). Refusing a
///   new call ends no call already in flight: placed per shot, the executor
///   refuses a call once a shot's state failed, yet a call in flight on other
///   shots may still end done. Until the failure is recorded, such a call may
///   end as its own work does: an executor of several shots may have handed
///   over the events of shots that ran before, and a call on other shots may
///   end done (a design question, open, recorded at
///   [`ShotReplies`](crate::ShotReplies)'s source). The `conformance` check
///   `ends_calls_in_flight_at_an_earlier_failure` polls the failing call's
///   reply first.
/// - Once closed ([`Close`]), it refuses every call.
/// - A wrapper never forgets a refusal. A wrapper is an executor that
///   wraps another, or is made of one, and continues it, as `.shots(..)`
///   and `.run_on(..)` put one on an executor that
///   [`executor_fn`](fn@crate::executor_fn) makes (given a state per shot,
///   placed on a thread); it never forgets that the executor it continues
///   is closed or failed: it keeps refusing its calls, since it continues
///   what it wraps, failures included, as iterator adapters and Tower's
///   `Buffer` do. Known issue, open (blocks neither this commit nor a later
///   one): the `conformance` module has no check of this rule, which would
///   need a function that wraps the executor under test; the crate's own
///   wrappers are tested.
///
/// How an executor keeps the failure and close rules, which the
/// `conformance` module checks (`refuses_calls_after_a_failure` and
/// `ends_calls_in_flight_at_an_earlier_failure` on an executor whose state
/// carries over, and `rejects_calls_once_closed`):
///
/// - an executor with state of its own can keep them on an
///   [`ExecutorLifecycle`](crate::ExecutorLifecycle), or be built by
///   [`executor_fn`](fn@crate::executor_fn), which keeps them;
/// - one that passes its calls on (an adapter's executor) keeps them as the
///   executor downstream of it does;
/// - one with no state between calls has no state a failure leaves
///   part-way, so it needs only a flag that it is closed.
///
/// The executor an adapter written by hand gives ([`Apply`](crate::Apply))
/// is reached only through the executor composed with it, which keeps the
/// refusals after a failure, a panic and close for it, so it need not: its
/// own `call` only runs or refuses its batch.
///
/// # The last executor of a chain
///
/// The executor at the end of a chain (a simulator, a machine) is called
/// with a batch on a set of shots ([`InstructionBatch`]), describes what it
/// offers ([`Describe`]) and is closed once, with a report ([`Close`]):
/// [`compose`](fn@crate::compose) requires the three of any executor on its
/// right, this one or adapters composed with it. The chain's driver calls
/// it, and the chain's run closes it.
///
/// It holds its shots' states its own way, and states what that allows in
/// its capabilities, which a chain checks against a job's needs before
/// anything runs ([`Chain::run`](crate::Chain::run)): how many shots it
/// holds at once ([`Capabilities::max_shots`]), whether a shot continues
/// from one call to the next (`resumable_shots`), whether a shot replays
/// from the seed (`replayable`). So a new way of holding shots is a new
/// executor, and its callers do not change:
///
/// | Executor                       | How it holds the shots                                                   | Replayable          |
/// | ------------------------------ | ------------------------------------------------------------------------ | ------------------- |
/// | a simulator, a device          | each in its own state, made at the shot's first call                     | as it offers        |
/// | an MPS ensemble (other crates) | together, from one state and one seed; a shot in one call                | no                  |
/// | grouped (planned)              | on shared states, split where their values differ                        | yes                 |
/// | sharded (planned)              | the shots split over several executors, their replies merged             | as the inner ones   |
///
/// Grouped and sharded executors are planned, with no milestone yet: a
/// grouped one for many shots of one program on a state vector, whose
/// common work then runs once; a sharded one for a run of many shots that
/// should use every core.
///
/// If it draws random numbers, it takes the run's seed when it is built, and
/// shot `j`'s components draw from the seeds of
/// [`ShotContext::last_executor`]`(seed, j)`, each under its own id (an
/// adapter's from [`AdapterSeeds::shot`](crate::AdapterSeeds::shot)), so one
/// number replays the whole run; hardware takes none. Only an
/// [`executor_fn`](fn@crate::executor_fn) passes that seed up to the
/// adapters above it today ([`ExecutorFn::seed`](crate::ExecutorFn::seed));
/// one written by hand cannot state it yet (planned), so an adapter that
/// draws is refused over it
/// ([`BuildError::NoRunSeed`](crate::BuildError::NoRunSeed)). Each shot's
/// state carries over from one call to the next, until the executor is
/// closed.
pub trait Executor<M: Message> {
    /// The reply to one call, which delivers `M`'s events.
    type Reply: Reply<Item = M::Event>;

    /// Starts the work of `message` and returns its reply.
    ///
    /// # Errors
    ///
    /// [`Rejected`] when the message is refused. An executor's refusal means
    /// that no work was started, so the caller may correct the message and
    /// send it again without undoing anything. A rejection of class
    /// [`RetryClass::Resend`](crate::RetryClass::Resend) (such as
    /// [`Rejected::NoReply`]) instead means that no verdict arrived over a
    /// link, so work may have run. A run that starts and cannot finish is
    /// reported through the reply instead.
    fn call(&mut self, message: M::Ref<'_>) -> Result<Self::Reply, Rejected>;
}
