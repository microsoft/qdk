// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a run runs ([`AdaptiveProgram`], [`Run`], [`Chain::run`]): every job
//! becomes one through its driver, since a driver with its program
//! ([`DrivenProgram`]) implements `AdaptiveProgram`, so that a
//! [`Driver<P>`](Driver) is an adaptive program parameterised by its
//! program.
//!
//! A user writes one for a program that drives the executor itself: it sends
//! each batch and can choose the next from the outcomes so far. The crate's
//! driver, [`driver()`](fn@crate::driver), runs it ([`BuiltinDriver`]).

use std::future::Future;

#[cfg(doc)]
use crate::{BuiltinDriver, Chain, DrivenProgram, Driver, Run};
use crate::{Executor, Instruction, InstructionBatch, Needs, ShotSet};

/// A program that drives the executor itself: it sends each batch and can
/// choose the next from the outcomes so far. A static program, given to a
/// driver, is one that decides nothing. An adaptive program states what it
/// needs of the chain, and how it drives any executor of its instructions,
/// through one generic method. The crate's driver lets it drive
/// ([`BuiltinDriver`]).
///
/// A synchronous program writes `drive` as an `async fn` that takes each
/// reply with [`wait`](crate::wait); an asynchronous one awaits them
/// ([`wait_async`](crate::wait_async)). Either runs on a [`Run`], waited for
/// or awaited.
pub trait AdaptiveProgram {
    /// The instructions it sends.
    type Instruction: Instruction;
    /// What it gives when it is done.
    type Output;

    /// What it needs of the chain, checked when it is given to a chain
    /// ([`Chain::run`]). The default needs nothing but the run's shots, held
    /// at once ([`Needs`]), and states no instruction set: enough for typed
    /// instructions, while a program of calls by opcode
    /// ([`InstructionCall`](crate::InstructionCall)) also states their set,
    /// or a chain refuses its job. A program whose later calls continue a
    /// shot also states `resumable_shots` ([`Needs::resumable_shots`]),
    /// which no call checks.
    #[must_use]
    fn needs(&self) -> Needs {
        Needs::default()
    }

    /// Drives `executor` on `shots` to the end: each call names the shots
    /// it runs on, a part of `shots`; a call naming another shot is refused
    /// by the run ([`Rejected::ShotNotHeld`](crate::Rejected::ShotNotHeld)).
    /// The program only calls `executor`: what the chain offers was checked
    /// when the program was given to it, and the run closes it once the
    /// program's future has ended.
    fn drive<E>(self, executor: &mut E, shots: &ShotSet) -> impl Future<Output = Self::Output>
    where
        E: Executor<InstructionBatch<Self::Instruction>>;
}
