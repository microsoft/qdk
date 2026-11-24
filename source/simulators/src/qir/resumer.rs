// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What runs the shots of a [`QirMachine`]'s program on a quantum state
//! ([`QirResumer`]).
//!
//! The machine runs a program's classical part only. A resumer runs its
//! quantum calls on a target, such as a simulator or a device. It holds
//! each shot's quantum state and runs the batches the shot yields.

use std::fmt;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use super::{QirMachine, QirTrap};
#[cfg(doc)]
use super::{QirProgram, QirShotContext, QirStop};
use crate::OutputRecord;

/// The id of a shot: what a resumer derives the shot's randomness from
/// (its seed), so that, up to floating-point rounding, a shot's records
/// depend on its id alone (see [`QirResumer::run`]). Shots are named by
/// id, not by their position, so that a position cannot pass for an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QirShotId(pub u32);

impl QirShotId {
    /// The ids `0..count`, in order: the shots of a run of `count` shots.
    #[must_use]
    pub fn first(count: u32) -> Vec<QirShotId> {
        (0..count).map(QirShotId).collect()
    }
}

impl fmt::Display for QirShotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shot {}", self.0)
    }
}

/// Runs shots of a [`QirMachine`]'s program, each on its own quantum state.
/// It runs each batch a shot's context ([`QirShotContext`]) yields
/// ([`QirStop::Batch`]). It then resumes the context with the outcomes,
/// until the context ends ([`QirStop::Ended`]) or traps. A resumer chooses
/// where the state lives and in which order shots run.
pub trait QirResumer {
    /// What fails in the resumer's own work, apart from a trap of the
    /// program: a device's error, say.
    type Error;

    /// Runs the shots `shots` of `machine`'s program, whose ids are
    /// distinct, and gives their records, each shot's at its place in
    /// `shots`; no shots, no records. Up to floating-point rounding, a
    /// shot's records depend on its id alone. What the resumer needs to know
    /// of the program before a shot runs (its qubits, to make a state) it
    /// reads from [`QirMachine::program`] ([`QirProgram`]).
    ///
    /// If a shot fails, the run gives no records. Its error names the first
    /// failing shot in the order of `shots`.
    ///
    /// Async, because a resumer may wait on a device; a resumer that never
    /// waits returns a future that is ready at once, which
    /// [`expect_ready`] gives the output of.
    fn run(
        &mut self,
        machine: &QirMachine<'_>,
        shots: &[QirShotId],
    ) -> impl Future<Output = Result<Vec<Vec<OutputRecord>>, QirResumeError<Self::Error>>>;
}

/// Why a run of a [`QirResumer`] failed, and in which shot. Variants are
/// added as resumers need them (a job its device cancelled, say), hence
/// `#[non_exhaustive]`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QirResumeError<E> {
    /// The shot's context trapped.
    Trapped { shot: QirShotId, trap: QirTrap },
    /// The resumer's own work for the shot failed.
    Resumer { shot: QirShotId, error: E },
}

impl<E: fmt::Display> fmt::Display for QirResumeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trapped { shot, trap } => write!(f, "{shot} trapped: {trap}"),
            Self::Resumer { shot, error } => write!(f, "{shot} failed: {error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for QirResumeError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Trapped { trap, .. } => Some(trap),
            Self::Resumer { error, .. } => Some(error),
        }
    }
}

/// The output of `future`, which must be ready when first polled, as the
/// run of a [`QirResumer`] that never waits is.
///
/// # Panics
///
/// When `future` is pending: it waits on something, so its caller must
/// run it on an async executor instead.
pub fn expect_ready<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => {
            panic!(
                "the future is pending, so it waits on something: run it on an async executor instead"
            )
        }
    }
}
