// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SpyAlgebra`]: a spy on [`FakeAlgebra`], which counts the engine's
//! calls and records what it uploads, for the tests that pin how many moves
//! of the centre, reads and duplicates an operation takes, and what the
//! engine hands the algebra.

use crate::algebra::fake_algebra::FakeAlgebra;
use crate::algebra::{GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::rc::Rc;

/// How many calls of each counted method a [`SpyAlgebra`] answered,
/// and the values of each upload, in order.
#[derive(Default)]
pub(super) struct SpiedCalls {
    pub(super) qr_moves: Cell<usize>,
    pub(super) reads: Cell<usize>,
    pub(super) duplicates: Cell<usize>,
    pub(super) uploads: RefCell<Vec<Vec<Complex64>>>,
}

/// A spy: [`FakeAlgebra`], counting its moves of the centre, reads and
/// duplicates, and recording its uploads, into the [`SpiedCalls`] it shares
/// with the test.
#[derive(Clone)]
pub(super) struct SpyAlgebra(Rc<SpiedCalls>);

impl SpyAlgebra {
    /// The spy, and the record of the calls it answers.
    pub(super) fn new() -> (Self, Rc<SpiedCalls>) {
        let calls = Rc::new(SpiedCalls::default());
        (Self(Rc::clone(&calls)), calls)
    }
}

fn count(counter: &Cell<usize>) {
    counter.set(counter.get() + 1);
}

impl SiteAlgebra for SpyAlgebra {
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
