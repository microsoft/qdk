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
//! [`Executor::call`] returns at once with either the reply or a typed
//! [`Rejected`], which means that no work started; a run that starts and
//! cannot finish ends its reply with [`Status::Failed`] instead. What a
//! component offers is described by [`Describe`], and a component that keeps
//! totals returns them as a composable report from [`Close`].
//!
//! The crate depends on `std` only, so that every part of qdk can use it.
//! The crate's `README.md` explains the model and its choices.

mod codes;
mod completion;
mod executor;
mod message;
mod rejected;

pub use codes::{need_code, need_of, reason_code, retry_code, retry_of, value_code, value_of};
pub use completion::{Completion, Immediate, Status, wait};
pub use executor::{Capabilities, Close, Describe, Executor, Fold};
pub use message::{Batch, ErrorKind, Message, Outcome, ShotId, SubmissionId, Value};
pub use rejected::{Need, Rejected, RetryClass, Unmet};

#[cfg(test)]
mod tests;
