// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role that logs the seed of each call it gets:
//! [`SeesSeeds`].

use crate::prelude::*;
use crate::tests::fake_parts::Log;
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, NotReady, OpcodeMap,
    RunProvision, Seed, ShotId, ShotSet,
};

/// `part`, which logs `"{label} got seed {stream}"` for each `drive`,
/// `adapt` or `execute` it gets, where `stream` is the seed's stream for
/// shot 0, which tells seeds apart, since it is a bijection of the seed;
/// it does as `part` does otherwise, in each role `part` takes.
pub(crate) struct SeesSeeds<P> {
    part: P,
    label: &'static str,
    log: Log,
}

impl<P> SeesSeeds<P> {
    pub(crate) fn new(part: P, label: &'static str, log: &Log) -> Self {
        Self {
            part,
            label,
            log: log.clone(),
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

impl<P: Driver> Driver for SeesSeeds<P> {
    fn requires(&self) -> &Capabilities {
        self.part.requires()
    }

    fn provides(&self) -> &RunProvision {
        self.part.provides()
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
        self.saw(seed);
        self.part.drive(shots, seed, below)
    }
}

impl<P: Adapter> Adapter for SeesSeeds<P> {
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
        self.saw(seed);
        self.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}

impl<P: Executor> Executor for SeesSeeds<P> {
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
        self.saw(seed);
        self.part.execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
