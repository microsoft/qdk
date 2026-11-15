// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CountingAlgebra`]: [`FakeAlgebra`], counting the engine's calls and
//! recording what it uploads, for the tests that pin how many moves of the
//! centre, reads and duplicates an operation takes, and what the engine
//! hands the algebra.

use crate::algebra::fake_algebra::FakeAlgebra;
use crate::algebra::{GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::rc::Rc;

/// How many calls of each counted method a [`CountingAlgebra`] answered,
/// and the values of each upload, in order.
#[derive(Default)]
pub(super) struct Counts {
    pub(super) qr_moves: Cell<usize>,
    pub(super) reads: Cell<usize>,
    pub(super) duplicates: Cell<usize>,
    pub(super) uploads: RefCell<Vec<Vec<Complex64>>>,
}

/// [`FakeAlgebra`], counting its moves of the centre, reads and duplicates,
/// and recording its uploads, into the [`Counts`] it shares with the test.
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
        self.0.uploads.borrow_mut().push(values.to_vec());
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
        count(&self.0.duplicates);
        FakeAlgebra.duplicate(tensor, len)
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
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
