// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a call was refused, and what the caller may do next.

use std::fmt;

use crate::{ShotId, SubmissionId};

/// Why a call, or the opening of a session, was refused.
///
/// Every reason but [`Rejected::NoReply`] is the executor's verdict, given
/// before any work started, so a driver can match it and decide what to do
/// (see [`Rejected::retry`]). `NoReply` is the caller's own end of a link
/// saying that no verdict came, so the work may have run. A run that started
/// and could not finish is not a rejection: its reply ends with
/// [`Status::Failed`](crate::Status::Failed).
///
/// Each reason has a fixed code ([`reason_code`](crate::reason_code)), so
/// that two ends of a link agree on it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rejected {
    /// A list of shots that should name at least one names none.
    NoShots,
    /// A shot of the submission that the session does not hold.
    ShotNotInSession(ShotId),
    /// A shot named twice in a list of shots.
    RepeatedShot(ShotId),
    /// A submission id already used for other content.
    IdReused(SubmissionId),
    /// The instruction at `at` in the message is not one the executor
    /// accepts: outside its instruction set, malformed, or naming a qubit or
    /// a result the executor does not have.
    Instruction {
        /// The instruction's index in the message.
        at: usize,
        /// Why it is not accepted, in the executor's words.
        reason: String,
    },
    /// The executor was closed.
    Closed,
    /// An earlier call of the session failed, so the session ends there: its
    /// shots' states are no longer what the caller's program says. The
    /// reason is that failure's.
    SessionFailed(String),
    /// The executor cannot take work now (its worker or its link is gone, or
    /// it cannot hold the states the work needs). Nothing ran, and nothing is
    /// wrong with the message.
    Unavailable(String),
    /// A frame that is not a well-formed request.
    Malformed(String),
    /// A need beyond what a component of the stack offers.
    Unmet(Unmet),
    /// No verdict came over a link: the request may have run at the far end,
    /// or may never have arrived. The only rejection after which work may
    /// have started.
    NoReply(String),
    /// A rejection whose reason this end does not know, kept with the retry
    /// class and the text the far end sent, so the caller can still act on
    /// it.
    Unrecognized {
        /// What the caller may do next, as the far end said.
        retry: RetryClass,
        /// The far end's account of it.
        reason: String,
    },
}

impl Rejected {
    /// What the caller may do next. Needed because a resend is safe after
    /// some rejections and wasteful or wrong after others, and the two ends
    /// of a link must agree on which.
    #[must_use]
    pub fn retry(&self) -> RetryClass {
        match self {
            Rejected::Unavailable(_) => RetryClass::Later,
            Rejected::NoReply(_) => RetryClass::Resend,
            Rejected::Unrecognized { retry, .. } => *retry,
            Rejected::NoShots
            | Rejected::ShotNotInSession(_)
            | Rejected::RepeatedShot(_)
            | Rejected::IdReused(_)
            | Rejected::Instruction { .. }
            | Rejected::Closed
            | Rejected::SessionFailed(_)
            | Rejected::Malformed(_)
            | Rejected::Unmet(_) => RetryClass::Never,
        }
    }
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejected::NoShots => write!(f, "no shots"),
            Rejected::ShotNotInSession(shot) => write!(f, "shot {shot} is not in the session"),
            Rejected::RepeatedShot(shot) => write!(f, "shot {shot} is repeated"),
            Rejected::IdReused(id) => write!(f, "submission {id} already used for other content"),
            Rejected::Instruction { at, reason } => write!(f, "instruction {at}: {reason}"),
            Rejected::Closed => write!(f, "the executor is closed"),
            Rejected::SessionFailed(reason) => write!(f, "the session failed: {reason}"),
            Rejected::Unavailable(reason) => write!(f, "unavailable: {reason}"),
            Rejected::Malformed(reason) => write!(f, "malformed frame: {reason}"),
            Rejected::Unmet(unmet) => write!(f, "{unmet}"),
            Rejected::NoReply(reason) => write!(f, "no reply: {reason}"),
            Rejected::Unrecognized { reason, .. } => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for Rejected {}

/// What a caller may do after a rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RetryClass {
    /// Nothing ran, and the same request will be refused again. Do not resend
    /// it unchanged.
    Never,
    /// Nothing ran, and the request is not at fault: the executor cannot take
    /// it now. Resend it later, under the same id.
    Later,
    /// No verdict came, so it may have run. Resend it under the same id and
    /// with the same content; an executor that keeps ids runs it at most
    /// once.
    Resend,
}

/// A property a session may need from the components of a stack, and which a
/// component offers or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Need {
    /// The qubits each shot uses.
    Qubits,
    /// An outcome read before the end that decides what runs next, so a
    /// state per shot must be kept between calls.
    Feedback,
    /// Qubits prepared again during the run.
    Resets,
    /// One state per shot of the session, at once.
    States,
    /// Each shot draws only from its own random numbers, so its outcomes do
    /// not depend on the other shots of its session.
    Replayable,
    /// The probability of each shot's draws, reported when the session
    /// closes.
    Probabilities,
}

/// A need that a component cannot honour, and which component.
///
/// Needed because a stack is several components, and the caller must know
/// which one to change (a bigger simulator, a different terminal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmet {
    /// The component that sets the limit, by its Rust type name, which also
    /// names what it wraps.
    pub component: String,
    /// The need it cannot honour.
    pub need: Need,
    /// How much was needed (1 for a yes-or-no need).
    pub needed: u64,
    /// How much it offers (0 for a yes-or-no need it lacks).
    pub offered: u64,
}

impl fmt::Display for Unmet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} offers {} {:?}, {} needed",
            self.component, self.offered, self.need, self.needed
        )
    }
}
