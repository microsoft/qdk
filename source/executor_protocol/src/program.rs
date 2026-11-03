// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a job runs ([`Program`]): data, its instructions, its output and
//! its needs, which a driver drives ([`Driver`]). A program is adaptive by
//! its content (a branch on a readout), which its driver reads; a
//! [`StaticProgram`] has none, and the crate's driver runs it.

#[cfg(doc)]
use crate::{Chain, Driver, StaticProgram};
use crate::{Instruction, Needs};

/// A program: what a job gives a chain to run ([`Chain::run`]), with its
/// shots. It is data: the instructions its driver sends, what the run gives
/// when it is done, and what its content needs of the chain. Its driver
/// ([`Driver`]) drives it: it sends the calls, reads their outcomes and,
/// for a program with a branch on a readout, chooses the next calls from
/// them. A [`StaticProgram`] (fixed batches, run in order on every shot)
/// decides nothing, and the crate's driver runs it; a program of another
/// form has a driver of its own.
pub trait Program {
    /// The instructions its driver sends.
    type Instruction: Instruction;
    /// What the run gives when it is done.
    type Output;

    /// What its content needs of the chain, checked when its job is given
    /// to a chain ([`Chain::run`]), combined with what its driver's way of
    /// sending adds ([`Driver::needs`]). The default needs nothing but the
    /// run's shots, held at once ([`Needs`]), and states no instruction set:
    /// enough for typed instructions, while for a program of calls by
    /// opcode ([`InstructionCall`](crate::InstructionCall)) the program or
    /// its driver states their set ([`Needs::instruction_set`]), or a chain
    /// refuses its job.
    #[must_use]
    fn needs(&self) -> Needs {
        Needs::default()
    }
}
