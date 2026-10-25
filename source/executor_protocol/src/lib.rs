// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The execution protocol: how a caller hands quantum work to an executor
//! and reads its reply, the same way whatever runs the work (a simulator, an
//! adapter that runs a program's classical part, a remote device).
//!
//! An executor accepts one kind of [`Message`] and replies with a
//! [`Completion`] that delivers the message kind's events. The kind fixes
//! the meaning: a [`Batch`] (level L3) is a list of instructions run in order
//! on one shot, whose state carries over from the batches before it. Every
//! component of a stack is an [`Executor`]: a terminal runs the message, an
//! adapter keeps its meaning and passes the work on (possibly in another
//! instruction set). So a chain of adapters is again an executor of the same
//! kind, and a driver written once runs on every chain.
//!
//! A [`Submission`] (level L2) runs a batch on each shot of a set of shots
//! of a session, which an [`Open`] opens on the shots J with the caller's
//! [`Needs`]. Every maker of one shot's executor ([`MakeShot`]) opens
//! per-shot sessions, one L3 executor per shot, whose shot `j` draws from
//! seeds that depend only on the session's seed, `j` and the component that
//! draws ([`ShotContext`], [`shot_seed`]), so that a shot's outcomes do not
//! depend on how J is split or on which adapters are in the stack.
//!
//! [`Executor::call`] returns at once with either the reply or a typed
//! [`Rejected`], which means that no work started; a run that starts and
//! cannot finish ends its reply with [`Status::Failed`] instead, and fails
//! its session ([`Lifecycle`]). A reply is polled, never blocks, and wakes
//! its caller when it can make progress; [`wait`] and [`wait_async`] take a
//! whole reply, [`async_events`] its events as they come, and a reply made
//! elsewhere is a [`completer`]. What a component offers is described by
//! [`Describe`], and a component that keeps totals returns them as a
//! composable report from [`Close`].
//!
//! The crate depends on `std` and `futures-core` (for [`AsyncEvents`]'s
//! `Stream`) only, so that every part of qdk can use it. The crate's
//! `README.md` explains the model and its choices.

mod asynchronous;
mod codes;
mod completion;
mod executor;
mod lifecycle;
mod message;
mod rejected;
mod session;
mod shot;

pub use asynchronous::{AsyncEvents, WaitAsync, async_events, wait_async};
pub use codes::{need_code, need_of, reason_code, retry_code, retry_of, value_code, value_of};
pub use completion::{
    Completer, Completion, Immediate, Incremental, Map, Status, completer, relay, wait,
};
pub use executor::{Capabilities, Close, Describe, Executor, Fold, Needs};
pub use lifecycle::{Lifecycle, Watch, Watched};
pub use message::{Batch, ErrorKind, Message, Outcome, ShotId, SubmissionId, Value};
pub use rejected::{Need, Rejected, RetryClass, Unmet};
pub use session::{
    Gather, LogP, MakeShot, Open, SessionReport, ShotOutcome, Submission, SubmissionRef,
    SubmissionUnwrapper, SubmissionWrapper, open,
};
pub use shot::{BuildError, ComponentId, ComponentRngs, ShotContext, ShotSet, shot_seed};

#[cfg(test)]
mod tests;
