// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A fake part of any role whose futures may panic once dropped, and
//! also when polled: [`PanicsInDrop`].

use std::pin::Pin;
use std::task::{Context, Poll};

use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, Binding, Capabilities, DriveError, ExecuteError, NotReady,
    RunRequirement, Seed, ShotSet, TranslatingAdapter,
};

/// `part`, named as `part` is, whose futures of `drive`, `adapt` or
/// `execute` panic in their destructor when `armed`, once they have given
/// `part`'s answer, or, when made with [`polled_too`](Self::polled_too),
/// first when polled, before `part`'s future is; or, when made with
/// [`in_ready`](Self::in_ready), whose future of `ready` does so instead;
/// or, when made with [`pending_in_ready`](Self::pending_in_ready), whose
/// future of `ready` is pending at its first poll, and panics if dropped
/// then. It does as `part` does otherwise, in each role `part` takes. So
/// that a chain of such parts has one type whichever is armed.
pub(crate) struct PanicsInDrop<P> {
    part: P,
    breaks: Breaks,
}

/// Which of a [`PanicsInDrop`]'s futures panic, and when.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Breaks {
    Never,
    /// Those of `drive`, `adapt` and `execute`, in their destructor.
    WhenDropped,
    /// Those of `drive`, `adapt` and `execute`, when polled and again in
    /// their destructor.
    WhenPolled,
    /// That of `ready`, when polled and again in its destructor.
    InReady,
    /// That of `ready`, pending at its first poll, in its destructor.
    PendingInReady,
}

impl<P> PanicsInDrop<P> {
    pub(crate) fn new(part: P, armed: bool) -> Self {
        Self::breaking(part, armed, Breaks::WhenDropped)
    }

    /// `part`, whose futures, when `armed`, panic when polled, and again
    /// when dropped.
    pub(crate) fn polled_too(part: P, armed: bool) -> Self {
        Self::breaking(part, armed, Breaks::WhenPolled)
    }

    /// `part`, whose future of `ready`, when `armed`, panics when polled,
    /// and again when dropped; its other futures do not panic.
    pub(crate) fn in_ready(part: P, armed: bool) -> Self {
        Self::breaking(part, armed, Breaks::InReady)
    }

    /// `part`, whose future of `ready`, when `armed`, is pending at its
    /// first poll, as a device's still calibrating, and panics if dropped
    /// then; its other futures do not panic.
    pub(crate) fn pending_in_ready(part: P, armed: bool) -> Self {
        Self::breaking(part, armed, Breaks::PendingInReady)
    }

    fn breaking(part: P, armed: bool, breaks: Breaks) -> Self {
        Self {
            part,
            breaks: if armed { breaks } else { Breaks::Never },
        }
    }

    /// `future`, a future of `part`'s `ready`, which panics as `breaks`
    /// says.
    fn readying<F>(breaks: Breaks, future: F) -> Dropping<F> {
        Dropping {
            future: Box::pin(future),
            armed: matches!(breaks, Breaks::InReady | Breaks::PendingInReady),
            polled: breaks == Breaks::InReady,
            pending: breaks == Breaks::PendingInReady,
        }
    }

    /// `future`, a future of `part`'s `drive`, `adapt` or `execute`, which
    /// panics as `breaks` says.
    fn working<F>(breaks: Breaks, future: F) -> Dropping<F> {
        Dropping {
            future: Box::pin(future),
            armed: matches!(breaks, Breaks::WhenDropped | Breaks::WhenPolled),
            polled: breaks == Breaks::WhenPolled,
            pending: false,
        }
    }
}

/// A part's future, which panics in its destructor when `armed`, when
/// polled if `polled`, and is pending at its first poll if `pending`.
struct Dropping<F> {
    future: Pin<Box<F>>,
    armed: bool,
    polled: bool,
    pending: bool,
}

impl<F: Future> Future for Dropping<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<F::Output> {
        assert!(!self.polled, "the part's future broke when polled");
        if self.pending {
            self.pending = false;
            context.waker().wake_by_ref();
            return Poll::Pending;
        }
        self.future.as_mut().poll(context)
    }
}

impl<F> Drop for Dropping<F> {
    fn drop(&mut self) {
        assert!(!self.armed, "the part's future broke when dropped");
    }
}

impl<P: Driver> Driver for PanicsInDrop<P> {
    type Sends = P::Sends;

    fn name(&self) -> &str {
        self.part.name()
    }

    fn sends(&self) -> &P::Sends {
        self.part.sends()
    }

    fn requires(&self, run: &RunRequirement) -> Capabilities<P::Sends> {
        self.part.requires(run)
    }

    fn bind(&mut self, below: Binding<P::Sends>) {
        self.part.bind(below);
    }

    type Output = P::Output;

    fn ready(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        Self::readying(self.breaks, self.part.ready(run))
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = P::Sends>,
    ) -> impl Future<Output = Result<Vec<P::Output>, DriveError>> {
        let future = self.part.drive(shots, seed, below);
        Self::working(self.breaks, future)
    }
}

impl<P: TranslatingAdapter> TranslatingAdapter for PanicsInDrop<P> {
    type Accepts = P::Accepts;
    type Sends = P::Sends;

    fn name(&self) -> &str {
        self.part.name()
    }

    fn accepts(&self) -> &P::Accepts {
        self.part.accepts()
    }

    fn sends(&self) -> &P::Sends {
        self.part.sends()
    }

    fn provides(&self) -> &Capabilities<P::Accepts> {
        self.part.provides()
    }

    fn requires(&self, above: &Capabilities<P::Accepts>) -> Capabilities<P::Sends> {
        self.part.requires(above)
    }

    fn bind(&mut self, below: Binding<P::Sends>) {
        self.part.bind(below);
    }

    fn ready(
        &mut self,
        above: &Capabilities<P::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        Self::readying(self.breaks, self.part.ready(above))
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        below: &mut impl Executor<Accepts = P::Sends>,
        readouts: &mut [BatchReadouts<P::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        let future = self.part.adapt(batches, seed, below, readouts);
        Self::working(self.breaks, future)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}

impl<P: Executor> Executor for PanicsInDrop<P> {
    type Accepts = P::Accepts;

    fn name(&self) -> &str {
        self.part.name()
    }

    fn accepts(&self) -> &P::Accepts {
        self.part.accepts()
    }

    fn provides(&self) -> &Capabilities<P::Accepts> {
        self.part.provides()
    }

    fn ready(
        &mut self,
        above: &Capabilities<P::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        Self::readying(self.breaks, self.part.ready(above))
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<P::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        let future = self.part.execute(batches, seed, readouts);
        Self::working(self.breaks, future)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
