// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Message kinds, and the events of level L3.

use std::marker::PhantomData;

/// A shot's number within a session. Shots are numbered by the caller.
pub type ShotId = u32;

/// A submission's number within a session, chosen by the caller: what makes a
/// resent submission recognizable as the same one.
pub type SubmissionId = u64;

/// A message kind: the borrowed form of one request at one level of a stack,
/// and the events of its reply.
///
/// The event type belongs to the kind, not to the executor, because the
/// caller and the executor of a level must agree on the reply as they agree
/// on the message: an executor of batches cannot reply with another level's
/// events.
pub trait Message {
    /// The message as the caller lends it. Borrowed, so that passing it down
    /// a chain of executors copies nothing.
    type Ref<'a>: Copy
    where
        Self: 'a;
    /// What the reply to a message of this kind carries.
    type Event;
}

/// Level L3: a batch of instructions of type `I`, run in order on one shot,
/// after the batches before it (the shot's state carries over).
///
/// The instruction type is the instruction set (ISA) the executor accepts.
/// Making it a type parameter checks, when a stack is compiled, that each
/// adapter sends the instructions the executor it wraps accepts.
///
/// This type is never constructed: it only names the message kind.
pub struct Batch<I> {
    instructions: PhantomData<fn() -> I>,
}

impl<I> Message for Batch<I> {
    type Ref<'a>
        = &'a [I]
    where
        Self: 'a;
    type Event = Outcome;
}

/// One readout of a shot: its tag and its value.
///
/// The tag is the batch's name for the readout (for example a result id), so
/// that a readout that arrives late, or from another shot, is still matched
/// to the instruction that asked for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Outcome {
    /// The batch's name for the readout.
    pub tag: u32,
    /// What was read.
    pub value: Value,
}

/// The value of a readout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// The qubit was found in |0⟩.
    Zero,
    /// The qubit was found in |1⟩.
    One,
    /// The readout has no value, for the reason given; it does not end the
    /// run, so the shot's other readouts stand.
    Error(ErrorKind),
}

impl From<bool> for Value {
    fn from(one: bool) -> Self {
        if one { Value::One } else { Value::Zero }
    }
}

/// Why a readout has no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The qubit was lost (for example, an atom left its trap).
    Loss,
    /// Any other reason the device gives.
    Other,
}
