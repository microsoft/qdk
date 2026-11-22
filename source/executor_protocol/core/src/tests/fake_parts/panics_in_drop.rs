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
/// [`in_ready`](Self::in_ready), whose future of `ready` does so instead.
/// It does as `part` does otherwise, in each role `part` takes. So that a
/// chain of such parts has one type whichever is armed.
pub(crate) struct PanicsInDrop<P> {
    part: P,
    armed: bool,
    polled: bool,
    in_ready: bool,
}

impl<P> PanicsInDrop<P> {
    pub(crate) fn new(part: P, armed: bool) -> Self {
        Self {
            part,
            armed,
            polled: false,
            in_ready: false,
        }
    }

    /// `part`, whose futures, when `armed`, panic when polled, and again
    /// when dropped.
    pub(crate) fn polled_too(part: P, armed: bool) -> Self {
        Self {
            part,
            armed,
            polled: armed,
            in_ready: false,
        }
    }

    /// `part`, whose future of `ready`, when `armed`, panics when polled,
    /// and again when dropped; its other futures do not panic.
    pub(crate) fn in_ready(part: P, armed: bool) -> Self {
        Self {
            part,
            armed: false,
            polled: false,
            in_ready: armed,
        }
    }

    /// `future`, a future of `part`'s `ready`, which panics as `in_ready`
    /// says.
    fn readying<F>(in_ready: bool, future: F) -> Dropping<F> {
        Dropping {
            future: Box::pin(future),
            armed: in_ready,
            polled: in_ready,
        }
    }
}

/// A part's future, which panics in its destructor when `armed`, and
/// when polled if `polled`.
struct Dropping<F> {
    future: Pin<Box<F>>,
    armed: bool,
    polled: bool,
}

impl<F: Future> Future for Dropping<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<F::Output> {
        assert!(!self.polled, "the part's future broke when polled");
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
        Self::readying(self.in_ready, self.part.ready(run))
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = P::Sends>,
    ) -> impl Future<Output = Result<Vec<P::Output>, DriveError>> {
        let future = self.part.drive(shots, seed, below);
        Dropping {
            future: Box::pin(future),
            armed: self.armed,
            polled: self.polled,
        }
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
        Self::readying(self.in_ready, self.part.ready(above))
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        below: &mut impl Executor<Accepts = P::Sends>,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        let future = self.part.adapt(batches, seed, below, readouts);
        Dropping {
            future: Box::pin(future),
            armed: self.armed,
            polled: self.polled,
        }
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
        Self::readying(self.in_ready, self.part.ready(above))
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_, P::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        let future = self.part.execute(batches, seed, readouts);
        Dropping {
            future: Box::pin(future),
            armed: self.armed,
            polled: self.polled,
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.part.end_shots(shots);
    }
}
