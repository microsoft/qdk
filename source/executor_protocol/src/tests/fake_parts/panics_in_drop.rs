// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role whose futures may panic once dropped:
//! [`PanicsInDrop`].

use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{
    Adapt, Batch, BatchReadouts, Capabilities, Drive, DriveError, Execute, ExecuteError,
    InstructionSet, Needs, OpcodeMap, RunRefusal, Seed, ShotSet,
};

/// `part`, named as `part` is, whose futures of `drive`, `adapt` or
/// `execute` panic in their destructor when `armed`, once they have given
/// `part`'s answer; it does as `part` does otherwise, in each role `part`
/// takes. So that a chain of such parts has one type whichever is armed.
pub(crate) struct PanicsInDrop<P> {
    part: P,
    armed: bool,
}

impl<P> PanicsInDrop<P> {
    pub(crate) fn new(part: P, armed: bool) -> Self {
        Self { part, armed }
    }
}

/// A part's future, which panics in its destructor when `armed`.
struct Dropping<F> {
    future: Pin<Box<F>>,
    armed: bool,
}

impl<F: Future> Future for Dropping<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<F::Output> {
        self.future.as_mut().poll(context)
    }
}

impl<F> Drop for Dropping<F> {
    fn drop(&mut self) {
        assert!(!self.armed, "the part's future broke when dropped");
    }
}

impl<P: Drive> Drive for PanicsInDrop<P> {
    type Output = P::Output;

    fn name(&self) -> &str {
        self.part.name()
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
        let armed = self.armed;
        Dropping {
            future: Box::pin(self.part.drive(shots, seed, below)),
            armed,
        }
    }
}

impl<P: Adapt> Adapt for PanicsInDrop<P> {
    fn name(&self) -> &str {
        self.part.name()
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
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>> {
        let armed = self.armed;
        Dropping {
            future: Box::pin(self.part.adapt(batches, seed, below)),
            armed,
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}

impl<P: Execute> Execute for PanicsInDrop<P> {
    fn name(&self) -> &str {
        self.part.name()
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
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>> {
        let armed = self.armed;
        Dropping {
            future: Box::pin(self.part.execute(batches, seed)),
            armed,
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
