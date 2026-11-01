// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a call was refused, and what the caller may do next; and a call's
//! id, which names the call that a resend repeats.

use std::fmt;

use crate::{ShotId, UnmetNeed};

/// A call's number: what makes a resent call recognizable as the same one.
///
/// Planned, for layer L1 (an endpoint, called with requests, for an
/// executor across a network: a device behind a cloud service, a simulator
/// on another machine): a request carries a call under this id, and a
/// resend under the same id runs at most once. Who chooses the id, the
/// caller or the caller's end of the endpoint, is decided with the link
/// layers (the crate's `docs/rejections.md`, §A lost verdict (planned:
/// layers L1 and L0)). No message of this crate carries it yet; it is
/// defined so that [`Rejected::IdReused`] and its reason code keep their
/// meaning.
pub type CallId = u64;

/// Why a call was refused.
///
/// A rejection means that no work started, unless its class
/// ([`Rejected::retry`]) is [`RetryClass::Resend`]: then no verdict came over
/// a link, and the work may have run ([`Rejected::NoReply`], or an
/// [`Rejected::Unrecognized`] reason that the far end classed so). Matching
/// the class rather than the reason is what lets a caller act on a reason it
/// does not know. A reason of another class is given before any work started
/// (by the executor, or by [`ShotSet::new`](crate::ShotSet::new) when the
/// message is built), so a driver can match it and decide what to do. A run
/// that started and could not finish is not a rejection: its reply ends with
/// [`ReplyStatus::Failed`](crate::ReplyStatus::Failed).
///
/// Each reason has a fixed code ([`reason_code`](crate::reason_code)), so
/// that two ends of a link agree on it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rejected {
    /// A list of shots that should name at least one names none.
    NoShots,
    /// A shot of the batch that the executor does not hold: one whose
    /// shots are fixed when it is built, such as an executor of one shot
    /// ([`ShotSet::check_only`](crate::ShotSet::check_only)); an ensemble
    /// that runs a set of shots from one state; one that holds the state of
    /// the shot of its first call alone
    /// ([`executor_fn`](fn@crate::executor_fn) without a shot strategy), and
    /// that refuses that first call's first shot when promised no state
    /// ([`Capabilities::states`](crate::Capabilities::states) `Some(0)`); one
    /// that makes each shot's state at the shot's first call, for the first
    /// shot beyond the states it promised ([`OnePerShot`](crate::OnePerShot)),
    /// and never for its own sake with no limit promised; or a run, for a
    /// shot outside its driver's ([`Run`](crate::Run)). A run gives it before
    /// [`EarlierFailure`](Rejected::EarlierFailure), which only the executor
    /// knows: asking it would make a state for the foreign shot. Where
    /// opcodes are renumbered, the run's comes before the renumbering (see
    /// [`Instruction`](Rejected::Instruction)).
    ShotNotHeld(ShotId),
    /// A shot named twice in a list of shots.
    RepeatedShot(ShotId),
    /// A call id already used for other content. Planned: layer L1,
    /// whose requests carry the id to an executor across a network (a
    /// device behind a cloud service, a simulator on another machine),
    /// produces it; no component does yet.
    IdReused(CallId),
    /// The instruction at `at` in the message is not one the executor
    /// accepts: outside its instruction set, malformed, naming a qubit or a
    /// result the executor does not have, or repeating, within the message,
    /// the tag of a readout before it
    /// ([`check_unique_tags`](crate::check_unique_tags)).
    ///
    /// When several instructions are not accepted for one reason (several
    /// calls outside the set, several readouts that repeat a tag), `at` is
    /// the first of them in the message's order, as
    /// [`InstructionSet::check_batch`](crate::InstructionSet::check_batch)
    /// and [`check_unique_tags`](crate::check_unique_tags) give it: a check
    /// made at each call stops at its first fault. An executor that checks
    /// for several reasons in turn (its set, then its tags) refuses at the
    /// first fault of the first check that finds one, which may come after
    /// a fault that a later check would find.
    ///
    /// Where [`compose`](fn@crate::compose) renumbers opcodes, the
    /// renumbering sits between the sender of the calls, a driver or an
    /// adapter, and an executor that numbers its instructions otherwise. A
    /// batch the sender makes meets, in turn:
    /// - what sits between a driver and the renumbering, which checks only
    ///   the shots the batch names: a run refuses a shot outside its
    ///   driver's with [`ShotNotHeld`](Rejected::ShotNotHeld), whatever the
    ///   batch's calls; such a batch reaches neither the renumbering nor the
    ///   executor;
    /// - the renumbering: of the batches it is given, one with a call at an
    ///   opcode the sender's set lacks is refused there, at that call, or at
    ///   the first earlier call that the executor's set refuses, without
    ///   asking the executor, so before any refusal of the executor's own,
    ///   such as its [`ShotNotHeld`](Rejected::ShotNotHeld), its
    ///   [`Closed`](Rejected::Closed) (which a driver meets when the
    ///   executor was closed before `compose`) or its
    ///   [`EarlierFailure`](Rejected::EarlierFailure);
    /// - the executor: every other batch the renumbering is given reaches
    ///   it, even one outside its set only by its operands, and its own
    ///   order decides, as in the chain's numbering.
    Instruction {
        /// The instruction's index in the message.
        at: usize,
        /// Why it is not accepted, in the executor's words.
        reason: String,
    },
    /// The executor was closed.
    Closed,
    /// An earlier call of the executor failed, so the executor ends there:
    /// its shots' states are no longer what the caller's program says. The
    /// reason is that failure's.
    EarlierFailure(String),
    /// The executor cannot take the work now, though the work is within what
    /// it offers: its worker or its link is gone, or a resource the work
    /// needs is in use (device memory another run holds). Nothing ran,
    /// and nothing is wrong with the message, so the same message may run
    /// when sent again later. Work beyond what a component offers at all is
    /// refused whenever it is sent, by another variant: more shots than the
    /// states an executor declares it holds is [`Rejected::Unmet`] when
    /// composed (`>>`) or given to a chain
    /// ([`Chain::run`](crate::Chain::run)), and [`Rejected::ShotNotHeld`]
    /// from the executor called directly beyond its states.
    Unavailable(String),
    /// A frame that is not a well-formed request, or a sequence number resent
    /// with other bytes. Planned: layer L0, where requests cross a link to a
    /// remote executor as frames, produces it; no component does yet.
    Malformed(String),
    /// A need beyond what a component of the chain offers.
    Unmet(UnmetNeed),
    /// No verdict came over a link: the request may have run at the far end,
    /// or may never have arrived. Its class is [`RetryClass::Resend`].
    /// Planned: the caller's end of a link to a remote executor (layers L1,
    /// L0) produces it; no component does yet.
    NoReply(String),
    /// A rejection whose reason code this end does not know (a newer far
    /// end's), kept with the retry class and the text the far end sent: the
    /// class crosses a link before the reason, so the caller still knows
    /// whether the work may have run and whether to resend. Planned: a
    /// decoder of reason codes at a link's end produces it, and a relay
    /// sends it on as it came; there is no decoder yet.
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
            | Rejected::ShotNotHeld(_)
            | Rejected::RepeatedShot(_)
            | Rejected::IdReused(_)
            | Rejected::Instruction { .. }
            | Rejected::Closed
            | Rejected::EarlierFailure(_)
            | Rejected::Malformed(_)
            | Rejected::Unmet(_) => RetryClass::Never,
        }
    }

    /// This rejection as the caller upstream numbers its instructions: an
    /// [`Instruction`](Rejected::Instruction) rejection at `origin(at)`, any
    /// other unchanged. Its reason is kept, so it stays in the words of the
    /// executor downstream (its qubits, its instructions). Needed by an adapter
    /// that sends each instruction it receives down as other instructions (for
    /// instance, a code that sends each logical instruction as a gadget, or a
    /// program run region by region), since the downstream executor numbers
    /// what it receives, which its caller never saw.
    #[must_use]
    pub fn at_origin(self, origin: impl FnOnce(usize) -> usize) -> Self {
        match self {
            Rejected::Instruction { at, reason } => Rejected::Instruction {
                at: origin(at),
                reason,
            },
            other => other,
        }
    }
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejected::NoShots => write!(f, "no shots"),
            Rejected::ShotNotHeld(shot) => write!(f, "shot {shot} is not held by the executor"),
            Rejected::RepeatedShot(shot) => write!(f, "shot {shot} is repeated"),
            Rejected::IdReused(id) => write!(f, "call {id} already used for other content"),
            Rejected::Instruction { at, reason } => write!(f, "instruction {at}: {reason}"),
            Rejected::Closed => write!(f, "the executor is closed"),
            Rejected::EarlierFailure(reason) => write!(f, "an earlier call failed: {reason}"),
            Rejected::Unavailable(reason) => write!(f, "unavailable: {reason}"),
            Rejected::Malformed(reason) => write!(f, "malformed frame: {reason}"),
            Rejected::Unmet(unmet) => write!(f, "{unmet}"),
            Rejected::NoReply(reason) => write!(f, "no reply: {reason}"),
            Rejected::Unrecognized { reason, .. } => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for Rejected {}

/// What a caller may do after a rejection, and whether the work may have
/// run: only after [`Resend`](RetryClass::Resend).
///
/// The class, not the reason, answers that question, and it crosses a link
/// before the reason ([`retry_code`](crate::retry_code)), so a caller can act
/// on a reason it does not know ([`Rejected::Unrecognized`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RetryClass {
    /// Nothing ran, and the same message will be refused again: correct it
    /// before sending it again.
    Never,
    /// Nothing ran, and the message is not at fault: the executor cannot take
    /// it now. Send it again later; over a link, under the same
    /// [`CallId`], which a rejected request does not use up.
    //
    // Design question, open: an executor placed with `run_on` that refuses
    // with this class on its own thread fails, and refuses the call sent
    // again (`PlacedExecutor`, docs/rejections.md).
    Later,
    /// No verdict came, so the work may have run, or may never have arrived.
    /// Resend it under the same [`CallId`] and with the same content.
    /// That is safe because layer L1 (planned) runs each id at most once: it
    /// remembers each id with a fingerprint of its content, a repeat runs
    /// nothing and gets how the first run ended (layer L0 replays the run's
    /// events for a frame resent under its number), and other content under
    /// a used id is [`Rejected::IdReused`].
    Resend,
}
