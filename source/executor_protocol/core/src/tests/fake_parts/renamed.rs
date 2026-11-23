// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role, renamed, and logging the seeds it gets if
//! asked to: [`Renamed`].

use crate::prelude::*;
use crate::tests::fake_parts::Log;
use crate::{
    Batch, BatchReadouts, Binding, Capabilities, DriveError, ExecuteError, NotReady,
    RunRequirement, Seed, ShotId, ShotSet, StatesLimits, TranslatingAdapter,
};

/// `part`, named `name`, as a part built at run time is named by what it
/// was built from rather than by its type; it does as `part` does
/// otherwise, in each role `part` takes, stating the limits `part` states
/// when [`new`](Self::new) makes it, and none when
/// [`stating_nothing`](Self::stating_nothing) does. Made by
/// [`seeing_seeds`](Self::seeing_seeds), it also logs the seed of each
/// call it gets.
pub(crate) struct Renamed<P> {
    part: P,
    name: &'static str,
    /// The name it answers once its role's `ready` is called.
    name_once_ready: &'static str,
    /// Where it logs the seeds it gets, if it does.
    seeds: Option<Log>,
}

impl<P> Renamed<P> {
    pub(crate) fn new(part: P, name: &'static str) -> StatesLimits<Self> {
        StatesLimits {
            part: Self::stating_nothing(part, name),
        }
    }

    /// `part`, named `name`, which logs `"{name} got seed {stream}"` for
    /// each `drive`, `adapt` or `execute` it gets, where `stream` is the
    /// seed's stream for shot 0, which tells seeds apart, since it is a
    /// bijection of the seed.
    pub(crate) fn seeing_seeds(part: P, name: &'static str, log: &Log) -> StatesLimits<Self> {
        let mut renamed = Self::new(part, name);
        renamed.part.seeds = Some(log.clone());
        renamed
    }

    /// `part`, named `name`, held as a user's part that forwards only its
    /// role's public methods holds it: it states no limits, and gets
    /// `part` ready through that role's public `ready`.
    pub(crate) fn stating_nothing(part: P, name: &'static str) -> Self {
        Self {
            part,
            name,
            name_once_ready: name,
            seeds: None,
        }
    }

    fn saw(&self, seed: Seed) {
        if let Some(log) = &self.seeds {
            log.push(format!(
                "{} got seed {}",
                self.name,
                seed.shot(ShotId::new(0))
            ));
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
        self.saw(seed);
        self.part.drive(shots, seed, below)
    }
}

/// A translating adapter states its limits itself, so a renamed one needs
/// no [`AdapterLimits`]: it is the `StatesLimits` its constructor returns that
/// takes the role, as for a driver and an executor. Only the core can
/// write this impl, as `StatesLimits` and `TranslatingAdapter` are both its
/// own: code outside it implements `TranslatingAdapter` on its own type.
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

    fn provides(&self) -> Option<&Capabilities<P::Accepts>> {
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
        self.part.saw(seed);
        self.part.part.adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.part.end_shots(shots);
    }
}

impl<P: Executor> ExecutorLimits for Renamed<P> {
    fn provides(&self) -> Option<&Capabilities<P::Accepts>> {
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
        self.saw(seed);
        self.part.execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
