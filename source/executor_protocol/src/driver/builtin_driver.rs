// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's driver ([`BuiltinDriver`]), which [`driver()`](fn@driver)
//! puts at the head of a chain: it runs a [`StaticProgram`] and lets any
//! [`AdaptiveProgram`] drive, so that every job reads
//! `chain.run(program, shots)`. Its impl for a static program is with the
//! program ([`StaticProgram`]).

use crate::{
    AdaptiveProgram, ChainHead, Driver, Executor, Instruction, InstructionBatch, Needs, NoAdapter,
    ShotSet,
};
#[cfg(doc)]
use crate::{Chain, StaticProgram};

/// The crate's driver, at the head of a chain with no adapter yet: composed
/// with an executor, through adapters or not, it gives a [`Chain`] that
/// runs a [`StaticProgram`] of `I`s, or any [`AdaptiveProgram`] that sends
/// `I`s, `chain.run(program, shots)`. `I` is fixed by what it is composed
/// with.
#[must_use]
pub fn driver<I: Instruction>() -> ChainHead<BuiltinDriver, NoAdapter<I>> {
    ChainHead::new(BuiltinDriver)
}

/// The crate's driver, the one [`driver()`](fn@driver) puts at the head of
/// a chain: of a [`StaticProgram`] (each batch to every shot, in order), and
/// of any [`AdaptiveProgram`], which it lets drive. A chain of another
/// driver starts at `ChainHead::new(driver)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinDriver;

/// An adaptive program, given to a chain as any job is: its needs are its
/// own ([`AdaptiveProgram::needs`]), and it drives the chain's executor
/// itself.
// A program it does not drive is then reported as `Driver`'s fault, whose
// notes name what it runs, not as a missing `AdaptiveProgram`.
#[diagnostic::do_not_recommend]
impl<P: AdaptiveProgram> Driver<P> for BuiltinDriver {
    type Instruction = P::Instruction;
    type Output = P::Output;

    fn needs(&self, program: &P) -> Needs {
        program.needs()
    }

    async fn drive<E>(&self, program: P, executor: &mut E, shots: &ShotSet) -> P::Output
    where
        E: Executor<InstructionBatch<P::Instruction>>,
    {
        program.drive(executor, shots).await
    }
}
