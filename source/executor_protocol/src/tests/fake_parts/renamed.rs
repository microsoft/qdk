// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role, renamed: [`Renamed`].

use crate::{
    Adapt, AdapterSetup, Batch, BatchReadouts, Drive, DriveError, DriverSetup, Execute,
    ExecuteError, ExecutorSetup, InstructionSet, Needs, NotReady, Offer, OpcodeMap, Seed, ShotSet,
};

/// `part`, named `name`, as a part built at run time is named by what it
/// was built from rather than by its type; it does as `part` does
/// otherwise, in each role `part` takes. It forwards neither the `_each`
/// nor the `_stream` forms: through it, a `part` that gives its rows as
/// they come gives them at the end, and a stream is cut into batches, by
/// the defaults.
pub(crate) struct Renamed<P> {
    part: P,
    name: &'static str,
}

impl<P> Renamed<P> {
    pub(crate) fn new(part: P, name: &'static str) -> Self {
        Self { part, name }
    }
}

impl<P: DriverSetup> DriverSetup for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn sends(&self) -> &InstructionSet {
        self.part.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.part.bind(below);
    }

    fn needs(&self) -> Needs {
        self.part.needs()
    }
}

impl<P: Drive> Drive for Renamed<P> {
    type Output = P::Output;

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready()
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> impl Future<Output = Result<Vec<P::Output>, DriveError>> {
        self.part.drive(shots, seed, below)
    }
}

impl<P: AdapterSetup> AdapterSetup for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.part.sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.part.bind(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        self.part.needs_below(above)
    }

    fn offers_above(&self, below: Offer) -> Offer {
        self.part.offers_above(below)
    }
}

impl<P: Adapt> Adapt for Renamed<P> {
    fn ready(&mut self, above: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready(above)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}

impl<P: ExecutorSetup> ExecutorSetup for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn offers(&self) -> Offer {
        self.part.offers()
    }
}

impl<P: Execute> Execute for Renamed<P> {
    fn ready(&mut self, needs: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready(needs)
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
