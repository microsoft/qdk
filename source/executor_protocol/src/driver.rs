// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver ([`Driver`]): the component at the top of a chain, which runs
//! any program of its form given to the chain, so that programs are data (a
//! list of batches, a circuit, a QIR program) and one driver runs every
//! program of its form. In a chain being built it is the chain's head
//! ([`ChainHead`]), which composes with adapters and an executor into a
//! [`Chain`], built and checked, which runs the job given to it:
//! `chain.run(program, shots)`, a [`Run`] of the program as the driver
//! drives it ([`DrivenProgram`]). A job is the chain's input, not one of its
//! components.
//!
//! The crate's driver ([`driver()`](fn@driver), [`DefaultDriver`]) runs a
//! [`StaticProgram`] and any [`AdaptiveProgram`], so that every job reads
//! `chain.run(program, shots)`.

use std::future::Future;

use crate::{
    AdaptiveProgram, Executor, Instruction, InstructionBatch, Needs, NoAdapter, Rejected, ShotSet,
};
#[cfg(doc)]
use crate::{Chain, Run, StaticProgram};

/// A driver of programs of type `P`: what it needs of the chain to run a
/// program, and how it drives any executor of its instructions through the
/// program, as an [`AdaptiveProgram`] does on its own.
///
/// A runtime that executes a program's control logic, such as a QIR
/// runtime, is a driver: in [`drive`](Driver::drive) it makes its calls on
/// the one executor it is given, and the adapters and the executor below
/// look to it like that single executor. A caller puts it at the head of a
/// chain, `ChainHead::new(runtime) >> adapters >> executor`, then calls
/// `chain.run(program, shots)`. A driver with adapters below it is a
/// [`ChainHead`], not itself a `Driver` (planned).
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
    /// nothing but the run's shots, held at once ([`Needs`]).
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

/// The head of a chain being built: a driver, with the adapters composed
/// downstream of it so far (none at first). Composed with an adapter, it
/// gives a `ChainHead` with that adapter below the others; composed with an
/// executor, it gives a [`Chain`], which runs the job given to it. The
/// grouping does not matter: `(driver() >> a) >> executor` and
/// `driver() >> (a >> executor)?` build chains with the same run.
///
/// It holds the driver so that the driver composes with `>>`: Rust lets
/// this crate implement `>>` on its own types only, not on any type that
/// implements [`Driver`]. A chain's head is not itself a [`Driver`], even
/// with no adapter below its driver; that it is one, so that a driver with
/// adapters below it can be used wherever a driver is expected, is planned
/// (`docs/planned.md`).
///
/// # Type parameters
///
/// - `T`: the driver ([`Driver`]), given to [`ChainHead::new`]; the crate's
///   ([`DefaultDriver`]) from [`driver()`](fn@driver).
/// - `L`: the adapters composed downstream of it by `>>`
///   ([`ComposedAdapters`](crate::ComposedAdapters)). At first no adapter
///   (`NoAdapter<I>`), whose `I`, the instructions the driver sends, is
///   fixed when the chain is composed, before any program is given: by the
///   first adapter downstream of it, or by the executor.
pub struct ChainHead<T, L> {
    driver: T,
    adapters: L,
}

impl<T, I: Instruction> ChainHead<T, NoAdapter<I>> {
    /// `driver` at the head of a chain, sending `I`s, with no adapter yet.
    #[must_use]
    pub fn new(driver: T) -> Self {
        Self {
            driver,
            adapters: NoAdapter::default(),
        }
    }
}

impl<T, L> ChainHead<T, L> {
    /// This head, with its adapters replaced by `f` of them.
    pub(crate) fn map_adapters<M>(self, f: impl FnOnce(L) -> M) -> ChainHead<T, M> {
        ChainHead {
            driver: self.driver,
            adapters: f(self.adapters),
        }
    }

    /// The driver and the adapters.
    pub(crate) fn into_parts(self) -> (T, L) {
        (self.driver, self.adapters)
    }
}

/// A program `P`, as its driver `T` drives it: what the run a [`Chain`]
/// gives for a job runs ([`Chain::run`]), an [`AdaptiveProgram`] whose
/// needs are the driver's for the program ([`Driver::needs`]).
pub struct DrivenProgram<T, P> {
    driver: T,
    program: P,
}

impl<T, P> DrivenProgram<T, P> {
    /// `program`, as `driver` drives it.
    pub(crate) fn new(driver: T, program: P) -> Self {
        Self { driver, program }
    }
}

impl<T: Driver<P>, P> DrivenProgram<T, P> {
    /// The program's faults, as its driver finds them
    /// ([`Driver::check`]).
    pub(crate) fn check(&self) -> Vec<Rejected> {
        self.driver.check(&self.program)
    }
}

impl<T: Driver<P>, P> AdaptiveProgram for DrivenProgram<T, P> {
    type Instruction = T::Instruction;
    type Output = T::Output;

    fn needs(&self) -> Needs {
        self.driver.needs(&self.program)
    }

    async fn drive<E>(self, executor: &mut E, shots: &ShotSet) -> T::Output
    where
        E: Executor<InstructionBatch<T::Instruction>>,
    {
        let Self { driver, program } = self;
        driver.drive(program, executor, shots).await
    }
}

/// The crate's driver, at the head of a chain with no adapter yet: composed
/// with an executor, through adapters or not, it gives a [`Chain`] that
/// runs a [`StaticProgram`] of `I`s, or any [`AdaptiveProgram`] that sends
/// `I`s, `chain.run(program, shots)`. `I` is fixed by what it is composed
/// with.
#[must_use]
pub fn driver<I: Instruction>() -> ChainHead<DefaultDriver, NoAdapter<I>> {
    ChainHead::new(DefaultDriver)
}

/// The crate's driver, the one [`driver()`](fn@driver) puts at the head of
/// a chain: of a [`StaticProgram`] (each batch to every shot, in order), and
/// of any [`AdaptiveProgram`], which it lets drive. A chain of another
/// driver starts at `ChainHead::new(driver)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultDriver;

/// An adaptive program, given to a chain as any job is: its needs are its
/// own ([`AdaptiveProgram::needs`]), and it drives the chain's executor
/// itself.
// A program it does not drive is then reported as `Driver`'s fault, whose
// notes name what it runs, not as a missing `AdaptiveProgram`.
#[diagnostic::do_not_recommend]
impl<P: AdaptiveProgram> Driver<P> for DefaultDriver {
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

/// A job composed as a chain's head is, for the crate's tests: a program,
/// the shots it runs, and the adapters composed downstream of it (none at
/// first). Composed with an executor, it gives the [`Run`] of the chain of
/// the crate's driver on its adapters and that executor, its job's faults
/// reported as [`BuildFaults`](crate::BuildFaults).
///
/// It takes no seed: the run's seed is given to the last executor when it
/// is built ([`ExecutorFn::seed`](crate::ExecutorFn::seed)), and reaches
/// each adapter that draws (a noise adapter) when the adapter is built
/// against what is downstream of it
/// ([`AdapterBoundary::drawing_seeds`](crate::AdapterBoundary::drawing_seeds)),
/// so that each draws from seeds of its own and the program states only
/// what it runs.
///
/// # Type parameters
///
/// - `P`: the program ([`AdaptiveProgram`]), given to [`ComposedJob::new`].
/// - `L`: the adapters composed downstream of it by `>>`
///   ([`ComposedAdapters`](crate::ComposedAdapters)); none at first.
#[cfg(test)]
pub(crate) struct ComposedJob<P, L> {
    program: P,
    shots: ShotSet,
    adapters: L,
}

#[cfg(test)]
impl<P: AdaptiveProgram> ComposedJob<P, NoAdapter<P::Instruction>> {
    /// `program`, to run `shots`. The same job also runs as any other
    /// does, given to a chain of the crate's driver:
    /// `(driver() >> executor)?.run(program, shots)`
    /// ([`Chain::run`](crate::Chain::run)), its faults then reported
    /// together with the shots' ([`JobFaults`](crate::JobFaults)).
    #[must_use]
    pub(crate) fn new(program: P, shots: ShotSet) -> Self {
        Self {
            program,
            shots,
            adapters: NoAdapter::default(),
        }
    }
}

#[cfg(test)]
impl<P, L> ComposedJob<P, L> {
    /// This job, with its adapters replaced by `f` of them.
    pub(crate) fn map_adapters<M>(self, f: impl FnOnce(L) -> M) -> ComposedJob<P, M> {
        ComposedJob {
            program: self.program,
            shots: self.shots,
            adapters: f(self.adapters),
        }
    }

    /// The program, the shots and the adapters.
    pub(crate) fn into_parts(self) -> (P, ShotSet, L) {
        (self.program, self.shots, self.adapters)
    }
}
