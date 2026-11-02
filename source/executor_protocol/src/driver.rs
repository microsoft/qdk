// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver: a program that calls the executor of the chain it is given
//! to ([`Drive`]).
//!
//! - [`interpreter`]: a driver that is an interpreter of programs, which
//!   are then data: composed with an executor, it gives a chain that runs
//!   a job, `chain.run(program, shots)`.
//! - [`static_program`]: a program of fixed batches, and the crate's
//!   interpreter of it, [`interpreter()`](fn@crate::interpreter).

pub(crate) mod interpreter;
pub(crate) mod static_program;

use std::future::Future;

#[cfg(test)]
use crate::NoAdapter;
#[cfg(doc)]
use crate::{Chain, Run};
use crate::{Executor, Instruction, InstructionBatch, Needs, ShotSet};

/// A program that drives an executor: what it needs of the chain, and how it
/// drives any executor of its instructions, through one generic method.
///
/// A synchronous program writes `drive` as an `async fn` that takes each
/// reply with [`wait`](crate::wait); an asynchronous one awaits them
/// ([`wait_async`](crate::wait_async)). Either runs on a [`Run`], waited for
/// or awaited.
pub trait Drive {
    /// The instructions it sends.
    type Instruction: Instruction;
    /// What it gives when it is done.
    type Output;

    /// What it needs of the chain, checked when it is given to a chain
    /// ([`Chain::run`]). The default needs nothing but the run's shots, held
    /// at once ([`Needs`]).
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

/// A driver, for the crate's tests: a program, the shots it runs, and the
/// adapters composed downstream of it (none at first). Composed with an
/// executor, it gives the [`Run`] of the chain of the crate's interpreter
/// on its adapters and that executor, its job's faults reported as
/// [`BuildFaults`](crate::BuildFaults).
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
/// - `P`: the program ([`Drive`]), given to [`Driver::new`].
/// - `L`: the adapters composed downstream of it by `>>`
///   ([`ComposedAdapters`](crate::ComposedAdapters)); none at first.
#[cfg(test)]
pub(crate) struct Driver<P, L> {
    program: P,
    shots: ShotSet,
    adapters: L,
}

#[cfg(test)]
impl<P: Drive> Driver<P, NoAdapter<P::Instruction>> {
    /// `program`, to run `shots`. The same job also runs as any other
    /// does, given to a chain of the crate's interpreter:
    /// `(interpreter() >> executor)?.run(program, shots)`
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
impl<P, L> Driver<P, L> {
    /// This driver, with its adapters replaced by `f` of them.
    pub(crate) fn map_adapters<M>(self, f: impl FnOnce(L) -> M) -> Driver<P, M> {
        Driver {
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
