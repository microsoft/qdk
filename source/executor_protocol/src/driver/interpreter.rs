// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A driver that interprets programs ([`Interpret`]): the programs are then
//! data (a list of batches, a circuit, a QIR program), and one interpreter
//! drives every program of its kind. As a chain's driver
//! ([`InterpreterDriver`]) it composes with adapters and an executor into a
//! [`Chain`], built and checked, which runs the job given to it:
//! `chain.run(program, shots)`, a [`Run`] of the program as the
//! interpreter drives it ([`Interpreted`]). A job is the chain's input, not
//! one of its components.
//!
//! The crate's interpreter ([`interpreter()`](fn@interpreter),
//! [`DefaultInterpreter`]) runs a [`StaticProgram`] and any program that
//! drives itself ([`Drive`]), so that every job reads
//! `chain.run(program, shots)`.

use std::future::Future;

#[cfg(doc)]
use crate::{Chain, Run, StaticProgram};
use crate::{Drive, Executor, Instruction, InstructionBatch, Needs, NoAdapter, Rejected, ShotSet};

/// An interpreter of programs of type `P`: what it needs of the chain to
/// run a program, and how it drives any executor of its instructions
/// through the program, as [`Drive`] does for a program that drives
/// itself.
///
/// The interpreter is apart from the program, so a crate may interpret a
/// program type of another crate, and one interpreter may interpret several
/// program types, one impl each. It takes itself by reference: a program
/// does not use it up, so that a chain that runs many jobs, planned
/// ([`Chain`]), can keep it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not interpret a `{P}`",
    label = "not an interpreter of this program",
    note = "a chain runs the programs its driver interprets: \
            `(interpreter() >> executor)?.run(program, shots)` (docs/composition.md, \"Composing a run\")",
    note = "the crate's interpreter, `interpreter()`, runs a `StaticProgram` and any program that \
            drives itself (`Drive`)",
    note = "a `StaticProgram<I>` runs where `I` is an `Instruction` and the chain's executor accepts \
            batches of `I`"
)]
pub trait Interpret<P> {
    /// The instructions it sends.
    type Instruction: Instruction;
    /// What it gives when it is done.
    type Output;

    /// What it needs of the chain to run `program`, checked when the
    /// program is given to the chain ([`Chain::run`]). The default needs
    /// nothing but a state per shot.
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
    /// [`Drive::drive`] does: each call names the shots it runs on, a part
    /// of `shots`.
    fn interpret<E>(
        &self,
        program: P,
        executor: &mut E,
        shots: &ShotSet,
    ) -> impl Future<Output = Self::Output>
    where
        E: Executor<InstructionBatch<Self::Instruction>>;
}

/// An interpreter as a chain's driver, with the adapters composed
/// downstream of it (none at first): composed with an executor, it gives a
/// [`Chain`], which runs the job given to it.
///
/// # Type parameters
///
/// - `T`: the interpreter ([`Interpret`]), given to
///   [`InterpreterDriver::new`]; the crate's ([`DefaultInterpreter`]) from
///   [`interpreter`].
/// - `L`: the adapters composed downstream of it by `>>`
///   ([`ComposedAdapters`](crate::ComposedAdapters)). At first no adapter
///   (`NoAdapter<I>`), whose `I`, the instructions the interpreter sends, is
///   fixed when the chain is composed, before any program is given: by the
///   first adapter downstream of it, or by the executor.
pub struct InterpreterDriver<T, L> {
    interpreter: T,
    adapters: L,
}

impl<T, I: Instruction> InterpreterDriver<T, NoAdapter<I>> {
    /// `interpreter` as a chain's driver, sending `I`s, with no adapter yet.
    #[must_use]
    pub fn new(interpreter: T) -> Self {
        Self {
            interpreter,
            adapters: NoAdapter::default(),
        }
    }
}

impl<T, L> InterpreterDriver<T, L> {
    /// This driver, with its adapters replaced by `f` of them.
    pub(crate) fn map_adapters<M>(self, f: impl FnOnce(L) -> M) -> InterpreterDriver<T, M> {
        InterpreterDriver {
            interpreter: self.interpreter,
            adapters: f(self.adapters),
        }
    }

    /// The interpreter and the adapters.
    pub(crate) fn into_parts(self) -> (T, L) {
        (self.interpreter, self.adapters)
    }
}

/// A program `P`, as its interpreter `T` drives it: the driver of the run a
/// [`Chain`] gives for a job ([`Chain::run`]), whose needs are the
/// interpreter's for the program ([`Interpret::needs`]).
pub struct Interpreted<T, P> {
    interpreter: T,
    program: P,
}

impl<T, P> Interpreted<T, P> {
    /// `program`, as `interpreter` drives it.
    pub(crate) fn new(interpreter: T, program: P) -> Self {
        Self {
            interpreter,
            program,
        }
    }
}

impl<T: Interpret<P>, P> Interpreted<T, P> {
    /// The program's faults, as its interpreter finds them
    /// ([`Interpret::check`]).
    pub(crate) fn check(&self) -> Vec<Rejected> {
        self.interpreter.check(&self.program)
    }
}

impl<T: Interpret<P>, P> Drive for Interpreted<T, P> {
    type Instruction = T::Instruction;
    type Output = T::Output;

    fn needs(&self) -> Needs {
        self.interpreter.needs(&self.program)
    }

    async fn drive<E>(self, executor: &mut E, shots: &ShotSet) -> T::Output
    where
        E: Executor<InstructionBatch<T::Instruction>>,
    {
        let Self {
            interpreter,
            program,
        } = self;
        interpreter.interpret(program, executor, shots).await
    }
}

/// The crate's interpreter, as a chain's driver: composed with an
/// executor, through adapters or not, it gives a [`Chain`] that runs a
/// [`StaticProgram`] of `I`s, or any program that drives itself
/// ([`Drive`]) and sends `I`s, `chain.run(program, shots)`. `I` is fixed
/// by what it is composed with.
#[must_use]
pub fn interpreter<I: Instruction>() -> InterpreterDriver<DefaultInterpreter, NoAdapter<I>> {
    InterpreterDriver::new(DefaultInterpreter)
}

/// The crate's interpreter, the driver of [`interpreter()`](fn@interpreter):
/// of a [`StaticProgram`] (each batch to every shot, in order), and of any
/// program that drives itself ([`Drive`]), which it lets drive. A chain of
/// another interpreter is `InterpreterDriver::new(interpreter)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultInterpreter;

/// A program that drives itself, given to a chain as any job is: its needs
/// are its own ([`Drive::needs`]), and it drives the chain's executor
/// itself, as it does composed with one ([`Driver`](crate::Driver)).
// A program it does not interpret is then reported as `Interpret`'s fault,
// whose notes name what it runs, not as a missing `Drive`.
#[diagnostic::do_not_recommend]
impl<P: Drive> Interpret<P> for DefaultInterpreter {
    type Instruction = P::Instruction;
    type Output = P::Output;

    fn needs(&self, program: &P) -> Needs {
        program.needs()
    }

    async fn interpret<E>(&self, program: P, executor: &mut E, shots: &ShotSet) -> P::Output
    where
        E: Executor<InstructionBatch<P::Instruction>>,
    {
        program.drive(executor, shots).await
    }
}
