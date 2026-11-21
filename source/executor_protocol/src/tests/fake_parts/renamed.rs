// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role, renamed: [`Renamed`].

use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, InstructionSet, NotReady,
    OpcodeMap, Seed, ShotSet,
};

/// `part`, named `name`, as a part built at run time is named by what it
/// was built from rather than by its type; it does as `part` does
/// otherwise, in each role `part` takes.
pub(crate) struct Renamed<P> {
    part: P,
    name: &'static str,
    /// The name it answers once its role's `ready` is called.
    name_once_ready: &'static str,
}

impl<P> Renamed<P> {
    pub(crate) fn new(part: P, name: &'static str) -> Self {
        Self {
            part,
            name,
            name_once_ready: name,
        }
    }

    /// This part, named `name` once its role's `ready` is called: a part
    /// that breaks the rule that its name does not change once composed.
    pub(crate) fn renamed_once_ready(mut self, name: &'static str) -> Self {
        self.name_once_ready = name;
        self
    }
}

impl<P: Driver> Driver for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn sends(&self) -> &InstructionSet {
        self.part.sends()
    }

    fn requires(&self, run: &Capabilities) -> Capabilities {
        self.part.requires(run)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.part.bind(below);
    }

    type Output = P::Output;

    fn ready(&mut self, run: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        self.name = self.name_once_ready;
        self.part.ready(run)
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> impl Future<Output = Result<Vec<P::Output>, DriveError>> {
        self.part.drive(shots, seed, below)
    }
}

impl<P: Adapter> Adapter for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.part.sends()
    }

    fn provides(&self) -> &Capabilities {
        self.part.provides()
    }

    fn requires(&self, above: &Capabilities) -> Capabilities {
        self.part.requires(above)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.part.bind(below);
    }

    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        self.name = self.name_once_ready;
        self.part.ready(required)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}

impl<P: Executor> Executor for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn provides(&self) -> &Capabilities {
        self.part.provides()
    }

    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        self.name = self.name_once_ready;
        self.part.ready(required)
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
