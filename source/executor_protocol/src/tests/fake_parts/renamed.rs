// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role, renamed: [`Renamed`].

use crate::{
    Adapt, AppendOnlyReadouts, Batch, Capabilities, Drive, DriveError, Execute, ExecuteError,
    InstructionSet, Needs, OpcodeMap, RunRefusal, Seed, ShotSet,
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

impl<P: Drive> Drive for Renamed<P> {
    type Output = P::Output;

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

    fn provision(&mut self) {
        self.part.provision();
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
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

impl<P: Adapt> Adapt for Renamed<P> {
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

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        self.part.offers_above(below)
    }

    fn provision(&mut self, above: Needs) {
        self.part.provision(above);
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        self.part.ready()
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}

impl<P: Execute> Execute for Renamed<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.part.offers()
    }

    fn provision(&mut self, needs: Needs) {
        self.part.provision(needs);
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        self.part.ready()
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
