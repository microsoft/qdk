// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Message kinds: what an executor is called with, and what its reply
//! carries.

#[cfg(doc)]
use crate::{Executor, InstructionBatch, ShotSet};

/// A message kind: what an executor is called with, lent as
/// [`Ref`](Message::Ref), and what its reply carries,
/// [`Event`](Message::Event). Every executor today is called with one kind,
/// [`InstructionBatch`]; the kind is what the compiler checks where two
/// executors meet, and the [`Executor`] trait and the conformance kit's
/// lifecycle checks are generic over it, so that a remote link's messages
/// can be added as kinds (planned).
///
/// The event type belongs to the kind, not to the executor, because the
/// caller and the executor must agree on the reply as they agree on the
/// message: an executor of one kind cannot reply with another kind's events.
pub trait Message {
    /// The message as the caller lends it. Borrowed and `Copy`, so that
    /// passing it down a chain of executors copies no instructions; nor does
    /// an executor of several shots, which passes each shot's executor the
    /// same instructions, on that shot alone ([`ShotSet::one`]).
    type Ref<'a>: Copy
    where
        Self: 'a;
    /// What the reply to a message of this kind carries.
    type Event;
}
