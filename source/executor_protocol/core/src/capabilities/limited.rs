// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A part whose limits the protocol reads, [`Limited`].

use crate::composition::DeclaredParts;
use crate::part_id::ChainPosition;
use crate::roles::Sealed;
use crate::{
    Adapter, AdapterLimits, Batch, BatchReadouts, Binding, Capabilities, DriveError, Driver,
    DriverLimits, ExecuteError, Executor, ExecutorLimits, NotReady, RunRequirement, Seed, ShotSet,
};

/// A part that states its limits, in the same role: what it provides,
/// what it requires, and how it gets ready for what arrives, as its
/// [`ExecutorLimits`], [`AdapterLimits`] or [`DriverLimits`] say.
///
/// A part with limits is joined as this, made by its `limited()`, so that
/// a part with none never sees them: the protocol reads a part's limits
/// only through this wrapper. Every other method is the part's own.
#[derive(Clone, Debug)]
pub struct Limited<P> {
    /// The part, reachable to read what it holds after a run, and to wrap
    /// a part stating limits in several roles, whose `limited()` would be
    /// ambiguous.
    pub part: P,
}

// Every method forwards, but the hooks, which answer with the part's
// limits; the lint fails the build if a new method falls back to its
// default instead.
#[deny(clippy::missing_trait_methods)]
impl<E: ExecutorLimits> Executor for Limited<E> {
    type Accepts = E::Accepts;

    fn name(&self) -> &str {
        Executor::name(&self.part)
    }

    fn accepts(&self) -> &E::Accepts {
        Executor::accepts(&self.part)
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        Executor::ready(&mut self.part)
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_, E::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<E::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        self.part.execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }

    fn provision(&self, sealed: Sealed) -> &Capabilities<E::Accepts> {
        let _ = sealed;
        ExecutorLimits::provides(&self.part)
    }

    fn declare<'a>(&'a self, above: &Capabilities<E::Accepts>, parts: &mut DeclaredParts<'a>) {
        parts.push(Executor::name(self));
        parts.receive(self.accepts(), self.provision(Sealed(())), above);
    }

    fn ready_at(
        &mut self,
        above: &Capabilities<E::Accepts>,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = at;
        self.part.ready_for(above)
    }
}

#[deny(clippy::missing_trait_methods)]
impl<A: AdapterLimits> Adapter for Limited<A> {
    type Set = A::Set;

    fn name(&self) -> &str {
        Adapter::name(&self.part)
    }

    fn accepts(&self) -> &A::Set {
        Adapter::accepts(&self.part)
    }

    fn sends(&self) -> &A::Set {
        Adapter::sends(&self.part)
    }

    fn bind(&mut self, below: Binding<A::Set>) {
        Adapter::bind(&mut self.part, below);
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        Adapter::ready(&mut self.part)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_, A::Set>],
        seed: Seed,
        below: &mut impl Executor<Accepts = A::Set>,
        readouts: &mut [BatchReadouts<A::Set>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        Adapter::adapt(&mut self.part, batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        Adapter::end_shots(&mut self.part, shots);
    }

    fn provision(&self, sealed: Sealed) -> &Capabilities<A::Set> {
        let _ = sealed;
        AdapterLimits::provides(&self.part)
    }

    fn requirement(&self, above: &Capabilities<A::Set>, sealed: Sealed) -> Capabilities<A::Set> {
        let _ = sealed;
        AdapterLimits::requires(&self.part, above)
    }

    fn ready_at(
        &mut self,
        above: &Capabilities<A::Set>,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = at;
        AdapterLimits::ready_for(&mut self.part, above)
    }
}

#[deny(clippy::missing_trait_methods)]
impl<D: DriverLimits> Driver for Limited<D> {
    type Output = D::Output;
    type Sends = D::Sends;

    fn name(&self) -> &str {
        Driver::name(&self.part)
    }

    fn sends(&self) -> &D::Sends {
        self.part.sends()
    }

    fn bind(&mut self, below: Binding<D::Sends>) {
        self.part.bind(below);
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        Driver::ready(&mut self.part)
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = D::Sends>,
    ) -> impl Future<Output = Result<Vec<D::Output>, DriveError>> {
        self.part.drive(shots, seed, below)
    }

    fn requirement(&self, run: &RunRequirement, sealed: Sealed) -> Capabilities<D::Sends> {
        let _ = sealed;
        DriverLimits::requires(&self.part, run)
    }

    fn declare<'a>(
        &'a self,
        run: &RunRequirement,
        parts: &mut DeclaredParts<'a>,
    ) -> Capabilities<D::Sends> {
        parts.push(Driver::name(self));
        let required = self.requirement(run, Sealed(()));
        parts.send(self.sends(), &required);
        required
    }

    fn count_parts<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        parts.push(Driver::name(self));
    }

    fn ready_at(
        &mut self,
        run: &RunRequirement,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = at;
        DriverLimits::ready_for(&mut self.part, run)
    }
}
