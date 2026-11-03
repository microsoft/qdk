// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver ([`Driver`]): the component at the top of a chain, which
//! drives the programs of its form given to the chain: a program is data
//! ([`Program`]: its instructions, its output and its needs), and the
//! driver sends its calls, reads their outcomes and, for a program with a
//! branch on a readout, chooses the next calls from them. In a chain being
//! built it is in the chain's prefix ([`ChainPrefix`]), with the adapters
//! composed below it so far, which composes with adapters and an executor
//! into a [`Chain`], built and checked, which runs the job given to it:
//! `chain.run(program, shots)`, a [`Run`] of the program as the driver
//! drives it. A job is the chain's input, not one of its components.
//!
//! - [`chain_prefix`]: a chain's prefix, a driver with the adapters
//!   composed below it so far ([`ChainPrefix`]), and how a driver becomes
//!   one ([`DriverExt`]).
//! - [`builtin_driver`](crate::facilities::builtin_driver): the crate's
//!   driver, which runs a [`StaticProgram`] ([`BuiltinDriver`], from
//!   [`driver()`](fn@crate::driver)).

pub(crate) mod chain_prefix;

use std::future::Future;

#[cfg(doc)]
use crate::{BuiltinDriver, Chain, ChainPrefix, DriverExt, Run, StaticProgram};
use crate::{Executor, InstructionBatch, Needs, Program, Rejected, ShotSet};

/// A driver of programs of type `P` ([`Program`]): what its way of sending
/// adds to the program's needs, the program's faults it can find before
/// any call, and how it drives any executor of the program's instructions
/// through the program, to the program's output. It sends the calls,
/// reads their outcomes and, for a program with a branch on a readout,
/// chooses the next calls from them; the program is data.
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
    note = "the crate's driver, `driver()`, runs a `StaticProgram`; a program of another form \
            needs a driver of its own, made a chain's prefix by `DriverExt::into_chain`",
    note = "a `StaticProgram<I>` runs where `I` is an `Instruction` and the chain's executor accepts \
            batches of `I`"
)]
pub trait Driver<P: Program> {
    /// What its way of sending adds to the needs of `program`
    /// ([`Program::needs`]), such as `resumable_shots`
    /// ([`Needs::resumable_shots`]) for a driver whose later calls
    /// continue a shot, which no call checks. The chain checks the two
    /// combined when the program is given to it ([`Chain::run`]), so a
    /// driver cannot drop a need of the program: the larger number of
    /// qubits, each flag either states, and the instruction set either
    /// states, refused as a fault of the job when both state one and the
    /// two differ ([`Needs`]). The default adds nothing.
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

    /// Drives `executor` through `program` on `shots` to the end, and gives
    /// the program's output: each call names the shots it runs on, a part
    /// of `shots`; a call naming another shot is refused by the run
    /// ([`Rejected::ShotNotHeld`]). It only calls `executor`: what the
    /// chain offers was checked when the program was given to it, and the
    /// run closes it once this future has ended.
    ///
    /// A synchronous driver writes it as an `async fn` that takes each
    /// reply with [`wait`](crate::wait); an asynchronous one awaits them
    /// ([`wait_async`](crate::wait_async)). Either runs on a [`Run`],
    /// waited for or awaited.
    fn drive<E>(
        &self,
        program: P,
        executor: &mut E,
        shots: &ShotSet,
    ) -> impl Future<Output = P::Output>
    where
        E: Executor<InstructionBatch<P::Instruction>>;
}
