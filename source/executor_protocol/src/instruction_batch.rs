// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The message an executor is called with: instructions run on each shot of
//! a set of the run's shots ([`InstructionBatch`]), lent as an
//! [`InstructionBatchRef`].

use std::marker::PhantomData;

#[cfg(doc)]
use crate::Executor;
use crate::{Message, ShotOutcome, ShotSet};

/// The message an executor is called with ([`Executor`]): instructions of
/// type `I`, run in order on each shot of a set of the run's shots, each
/// shot's state carrying over from the batches before it.
///
/// Its reply's events are [`ShotOutcome`]s, one per readout of each shot,
/// each named by the shot that produced it; within a batch, a tag names one
/// of the readouts the batch asks for, which each shot reports once, so that
/// a shot and a tag name one outcome ([`ShotOutcome::tag`]).
///
/// The instruction type fixes the shape of the instructions the executor
/// accepts. Making it a type parameter checks, when a chain is compiled,
/// that each adapter sends instructions of the type the executor after it
/// accepts. The executor may also describe, in
/// [`Capabilities::instruction_set`](crate::Capabilities::instruction_set),
/// which of them it runs. A set known only at run time is
/// `InstructionBatch<InstructionCall>`, whose calls the executor checks
/// against its description (see [`InstructionSet`](crate::InstructionSet)).
///
/// This type is never constructed: it only names the message kind.
pub struct InstructionBatch<I> {
    instructions: PhantomData<fn() -> I>,
}

impl<I> Message for InstructionBatch<I> {
    type Ref<'a>
        = InstructionBatchRef<'a, I>
    where
        Self: 'a;
    type Event = ShotOutcome;
}

/// An instruction batch, as the caller lends it for `'a`: its shots, and its
/// instructions of type `I`.
///
/// The shots are a [`ShotSet`], so a list that may repeat a shot, or name
/// none, is not a batch's shots.
#[derive(Debug)]
pub struct InstructionBatchRef<'a, I> {
    /// The shots to run the instructions on, each once, in this order.
    pub shots: &'a ShotSet,
    /// The instructions, run in this order on each shot of `shots`.
    pub instructions: &'a [I],
}

// Not derived: a derive would ask `I: Clone`, and a reference is copied
// whatever it refers to.
impl<I> Clone for InstructionBatchRef<'_, I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for InstructionBatchRef<'_, I> {}
