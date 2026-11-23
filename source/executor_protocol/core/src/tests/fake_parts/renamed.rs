// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role, renamed: [`Renamed`].

use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, Binding, Capabilities, DriveError, ExecuteError, NotReady,
    RunRequirement, Seed, ShotSet, StatesLimits, TranslatingAdapter,
};

/// `part`, named `name`, as a part built at run time is named by what it
/// was built from rather than by its type; it does as `part` does
/// otherwise, in each role `part` takes, stating the limits `part`
/// states.
pub(crate) struct Renamed<P> {
    part: P,
    name: &'static str,
    /// The name it answers once its role's `ready` is called.
    name_once_ready: &'static str,
}

impl<P> Renamed<P> {
    pub(crate) fn new(part: P, name: &'static str) -> StatesLimits<Self> {
        StatesLimits {
            part: Self {
                part,
                name,
                name_once_ready: name,
            },
        }
    }
}

impl<P> StatesLimits<Renamed<P>> {
    /// This part, named `name` once its role's `ready` is called: a part
    /// that breaks the rule that its name does not change once composed.
    pub(crate) fn renamed_once_ready(mut self, name: &'static str) -> Self {
        self.part.name_once_ready = name;
        self
    }
}

impl<P: Driver> DriverRequirements for Renamed<P> {
    fn requires(&self, run: &RunRequirement) -> Capabilities<P::Sends> {
        self.part.required(run)
    }

    fn ready_for(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        self.name = self.name_once_ready;
        self.part.ready_given(run)
    }
}

impl<P: Driver> Driver for Renamed<P> {
    type Sends = P::Sends;

    fn name(&self) -> &str {
        self.name
    }

    fn sends(&self) -> &P::Sends {
        self.part.sends()
    }

    fn bind(&mut self, below: Binding<P::Sends>) {
        self.part.bind(below);
    }

    type Output = P::Output;

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.name = self.name_once_ready;
        self.part.ready()
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = P::Sends>,
    ) -> impl Future<Output = Result<Vec<P::Output>, DriveError>> {
        self.part.drive(shots, seed, below)
    }
}

/// A translating adapter states its limits itself, so a renamed one needs
/// no [`AdapterLimits`]: it is the `StatesLimits` its constructor returns that
/// takes the role, as for a driver and an executor.
impl<P: TranslatingAdapter> TranslatingAdapter for StatesLimits<Renamed<P>> {
    type Accepts = P::Accepts;
    type Sends = P::Sends;

    fn name(&self) -> &str {
        self.part.name
    }

    fn accepts(&self) -> &P::Accepts {
        self.part.part.accepts()
    }

    fn sends(&self) -> &P::Sends {
        self.part.part.sends()
    }

    fn provides(&self) -> &Capabilities<P::Accepts> {
        self.part.part.provides()
    }

    fn requires(&self, above: &Capabilities<P::Accepts>) -> Capabilities<P::Sends> {
        self.part.part.requires(above)
    }

    fn bind(&mut self, below: Binding<P::Sends>) {
        self.part.part.bind(below);
    }

    fn ready(
        &mut self,
        above: &Capabilities<P::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        self.part.name = self.part.name_once_ready;
        self.part.part.ready(above)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        below: &mut impl Executor<Accepts = P::Sends>,
        readouts: &mut [BatchReadouts<P::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.part.end_shots(shots);
    }
}

impl<P: Executor> ExecutorLimits for Renamed<P> {
    fn provides(&self) -> &Capabilities<P::Accepts> {
        self.part.provided()
    }

    fn ready_for(
        &mut self,
        above: &Capabilities<P::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        self.name = self.name_once_ready;
        self.part.ready_given(above)
    }
}

impl<P: Executor> Executor for Renamed<P> {
    type Accepts = P::Accepts;

    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self) -> &P::Accepts {
        self.part.accepts()
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.name = self.name_once_ready;
        self.part.ready()
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<P::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
