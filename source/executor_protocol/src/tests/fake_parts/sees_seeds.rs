// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role that logs the seed of each call it gets:
//! [`SeesSeeds`].

use crate::tests::fake_parts::Log;
use crate::{
    Adapt, AdapterSetup, Batch, BatchReadouts, Drive, DriveError, DriverSetup, Execute,
    ExecuteError, ExecutorSetup, InstructionSet, Needs, NotReady, Offer, OpcodeMap, Seed, ShotId,
    ShotSet,
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

impl<P: DriverSetup> DriverSetup for SeesSeeds<P> {
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

impl<P: Drive> Drive for SeesSeeds<P> {
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
        self.saw(seed);
        self.part.drive(shots, seed, below)
    }
}

impl<P: AdapterSetup> AdapterSetup for SeesSeeds<P> {
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

impl<P: Adapt> Adapt for SeesSeeds<P> {
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
        self.saw(seed);
        self.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}

impl<P: ExecutorSetup> ExecutorSetup for SeesSeeds<P> {
    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn offers(&self) -> Offer {
        self.part.offers()
    }
}

impl<P: Execute> Execute for SeesSeeds<P> {
    fn ready(&mut self, needs: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready(needs)
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
