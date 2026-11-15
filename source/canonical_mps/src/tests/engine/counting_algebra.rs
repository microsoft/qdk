// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CountingAlgebra`]: [`FakeAlgebra`], counting the engine's calls, for
//! the tests that pin how many splits, moves of the centre and reads an
//! operation takes.

use crate::algebra::fake_algebra::FakeAlgebra;
use crate::algebra::{GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::Cell;
use std::convert::Infallible;
use std::rc::Rc;

/// How many calls of each counted method a [`CountingAlgebra`] answered.
#[derive(Default)]
pub(super) struct Counts {
    pub(super) gate_splits: Cell<usize>,
    pub(super) qr_moves: Cell<usize>,
    pub(super) reads: Cell<usize>,
}

/// [`FakeAlgebra`], counting its gate splits, moves of the centre and
/// reads into the [`Counts`] it shares with the test.
#[derive(Clone)]
pub(super) struct CountingAlgebra(Rc<Counts>);

impl CountingAlgebra {
    /// The algebra, and the counts it adds to.
    pub(super) fn new() -> (Self, Rc<Counts>) {
        let counts = Rc::new(Counts::default());
        (Self(Rc::clone(&counts)), counts)
    }
}

fn count(counter: &Cell<usize>) {
    counter.set(counter.get() + 1);
}

impl SiteAlgebra for CountingAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = Infallible;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        FakeAlgebra.upload(values)
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        count(&self.0.reads);
        FakeAlgebra.read(tensor, len)
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        FakeAlgebra.duplicate(tensor, len)
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        count(&self.0.gate_splits);
        FakeAlgebra.gate_split(call)
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        count(&self.0.qr_moves);
        FakeAlgebra.qr_move(call)
    }
}
