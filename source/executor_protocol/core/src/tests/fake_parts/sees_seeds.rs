// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role that logs the seed of each call it gets:
//! [`SeesSeeds`].

use crate::prelude::*;
use crate::tests::fake_parts::Log;
use crate::{
    Batch, BatchReadouts, Binding, Capabilities, DriveError, ExecuteError, Limited, NotReady,
    RunRequirement, Seed, ShotId, ShotSet, TranslatingAdapter,
};

/// `part`, which logs `"{label} got seed {stream}"` for each `drive`,
/// `adapt` or `execute` it gets, where `stream` is the seed's stream for
/// shot 0, which tells seeds apart, since it is a bijection of the seed;
/// it does as `part` does otherwise, in each role `part` takes, stating
/// the limits `part` states.
pub(crate) struct SeesSeeds<P> {
    part: P,
    label: &'static str,
    log: Log,
}

impl<P> SeesSeeds<P> {
    pub(crate) fn new(part: P, label: &'static str, log: &Log) -> Limited<Self> {
        Limited {
            part: Self {
                part,
                label,
                log: log.clone(),
            },
        }
    }

    fn saw(&self, seed: Seed) {
        self.log.push(format!(
            "{} got seed {}",
            self.label,
            seed.shot(ShotId::new(0))
        ));
    }
}

impl<P: Driver> DriverLimits for SeesSeeds<P> {
    fn requires(&self, run: &RunRequirement) -> Capabilities<P::Sends> {
        self.part.required(run)
    }

    fn ready_for(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready_given(run)
    }
}

impl<P: Driver> Driver for SeesSeeds<P> {
    type Sends = P::Sends;

    fn sends(&self) -> &P::Sends {
        self.part.sends()
    }

    fn bind(&mut self, below: Binding<P::Sends>) {
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
        below: &mut impl Executor<Accepts = P::Sends>,
    ) -> impl Future<Output = Result<Vec<P::Output>, DriveError>> {
        self.saw(seed);
        self.part.drive(shots, seed, below)
    }
}

/// A translating adapter states its limits itself, so one that sees seeds
/// needs no [`AdapterLimits`]: it is the `Limited` its constructor returns
/// that takes the role, as for a driver and an executor.
impl<P: TranslatingAdapter> TranslatingAdapter for Limited<SeesSeeds<P>> {
    type Accepts = P::Accepts;
    type Sends = P::Sends;

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
        self.part.part.ready(above)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        below: &mut impl Executor<Accepts = P::Sends>,
        readouts: &mut [BatchReadouts<P::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.saw(seed);
        self.part.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.part.end_shots(shots);
    }
}

impl<P: Executor> ExecutorLimits for SeesSeeds<P> {
    fn provides(&self) -> &Capabilities<P::Accepts> {
        self.part.provided()
    }

    fn ready_for(
        &mut self,
        above: &Capabilities<P::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready_given(above)
    }
}

impl<P: Executor> Executor for SeesSeeds<P> {
    type Accepts = P::Accepts;

    fn accepts(&self) -> &P::Accepts {
        self.part.accepts()
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready()
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<P::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.saw(seed);
        self.part.execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
