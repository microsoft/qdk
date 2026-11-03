// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver ([`Driver`]): the component at the top of a chain, which runs
//! any program of its form given to the chain, so that programs are data (a
//! list of batches, a circuit, a QIR program) and one driver runs every
//! program of its form. In a chain being built it is in the chain's prefix
//! ([`ChainPrefix`]), with the adapters composed below it so far, which
//! composes with adapters and an executor into a [`Chain`], built and
//! checked, which runs the job given to it:
//! `chain.run(program, shots)`, a [`Run`] of the program as the driver
//! drives it ([`DrivenProgram`]). A job is the chain's input, not one of its
//! components.
//!
//! - [`chain_prefix`]: a chain's prefix, a driver with the adapters
//!   composed below it so far ([`ChainPrefix`]), and how a driver becomes
//!   one ([`DriverExt`]).
//! - [`builtin_driver`](crate::facilities::builtin_driver): the crate's
//!   driver, which runs a [`StaticProgram`] and any [`AdaptiveProgram`]
//!   ([`BuiltinDriver`], from [`driver()`](fn@crate::driver)).
//! - [`driven_program`]: a program as its driver drives it, what a chain's
//!   run runs ([`DrivenProgram`]).

pub(crate) mod chain_prefix;
pub(crate) mod driven_program;

use std::future::Future;

#[cfg(doc)]
use crate::{
    AdaptiveProgram, BuiltinDriver, Chain, ChainPrefix, DrivenProgram, DriverExt, Run,
    StaticProgram,
};
use crate::{Executor, Instruction, InstructionBatch, Needs, Rejected, ShotSet};

/// A driver of programs of type `P`: what it needs of the chain to run a
/// program, and how it drives any executor of its instructions through the
/// program, as an [`AdaptiveProgram`] does on its own.
///
/// A runtime that executes a program's control logic, such as a QIR
/// runtime, is a driver: in [`drive`](Driver::drive) it makes its calls on
/// the one executor it is given, and the adapters and the executor below
/// look to it like that single executor. A caller makes it a chain's
/// prefix, `runtime.into_chain() >> adapters >> executor` (with
/// [`DriverExt`] in scope), then calls `chain.run(program, shots)`. A
/// driver with adapters below it is a [`ChainPrefix`]; that the prefix is
/// itself a `Driver` is planned (`docs/planned.md`).
///
/// The driver is apart from the program, so a crate may drive a program
/// type of another crate, and one driver may drive several program types,
/// one impl each. It takes itself by reference: a program does not use it
/// up, so that a chain that runs many jobs, planned ([`Chain`]), can keep
/// it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not drive a `{P}`",
    label = "not a driver of this program",
    note = "a chain runs the programs its driver drives: \
            `(driver() >> executor)?.run(program, shots)` (docs/composition.md, \"Composing a run\")",
    note = "the crate's driver, `driver()`, runs a `StaticProgram` and any `AdaptiveProgram`",
    note = "a `StaticProgram<I>` runs where `I` is an `Instruction` and the chain's executor accepts \
            batches of `I`"
)]
pub trait Driver<P> {
    /// The instructions it sends.
    type Instruction: Instruction;
    /// What it gives when it is done.
    type Output;

    /// What it needs of the chain to run `program`, checked when the
    /// program is given to the chain ([`Chain::run`]). The default needs
    /// nothing but the run's shots, held at once ([`Needs`]), and states no
    /// instruction set: enough for typed instructions, while a driver that
    /// sends calls by opcode ([`InstructionCall`](crate::InstructionCall))
    /// also states their set, or a chain refuses its job. A driver whose
    /// later calls continue a shot also states `resumable_shots`
    /// ([`Needs::resumable_shots`]), which no call checks.
    #[must_use]
    fn needs(&self, program: &P) -> Needs {
        let _ = program;
        Needs::default()
    }

    /// The faults of `program` that it can know before any call (a call
    /// outside the set it states), checked when the program is given to
    /// the chain ([`Chain::run`]) and reported with the job's other faults,
    /// so that none is found only once earlier calls have run. The default
    /// finds none.
    #[must_use]
    fn check(&self, program: &P) -> Vec<Rejected> {
        let _ = program;
        Vec::new()
    }

    /// Drives `executor` through `program` on `shots` to the end, as
    /// [`AdaptiveProgram::drive`] does: each call names the shots it runs
    /// on, a part of `shots`.
    fn drive<E>(
        &self,
        program: P,
        executor: &mut E,
        shots: &ShotSet,
    ) -> impl Future<Output = Self::Output>
    where
        E: Executor<InstructionBatch<Self::Instruction>>;
}
