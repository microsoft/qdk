// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role, renamed: [`Renamed`].

use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, NotReady, OpcodeMap, Seed,
    ShotSet,
};

/// `part`, named `name`, as a part built at run time is named by what it
/// was built from rather than by its type; it does as `part` does
/// otherwise, in each role `part` takes.
pub(crate) struct Renamed<P> {
    part: P,
    name: &'static str,
}

impl<P> Renamed<P> {
    pub(crate) fn new(part: P, name: &'static str) -> Self {
        Self { part, name }
    }
}

impl<P: Driver> Driver for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn requires(&self) -> &Capabilities {
        self.part.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.part.bind(below);
    }

    type Output = P::Output;

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready()
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

    fn provides(&self) -> &Capabilities {
        self.part.provides()
    }

    fn requires(&self) -> &Capabilities {
        self.part.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.part.bind(below);
    }

    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
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

    fn provides(&self) -> &Capabilities {
        self.part.provides()
    }

    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
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
