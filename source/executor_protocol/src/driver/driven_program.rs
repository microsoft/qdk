// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A program as its driver drives it ([`DrivenProgram`]): what the run a
//! chain gives for a job runs.

#[cfg(doc)]
use crate::Chain;
use crate::{AdaptiveProgram, Driver, Executor, InstructionBatch, Needs, Rejected, ShotSet};

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
