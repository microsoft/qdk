// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SpyAlgebra`]: a spy on the CPU algebra, [`CpuAlgebra`], which
//! counts the engine's calls and records what it uploads, for the tests
//! that pin how many splits, moves of the centre, reads and duplicates an
//! operation takes, and what the engine hands the algebra.

use crate::CpuAlgebra;
use crate::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::SvdError;
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// How many calls of each counted operation a [`SpyAlgebra`] answered,
/// and the values of each upload, in order.
#[derive(Default)]
pub(crate) struct SpiedCalls {
    pub(crate) gate_splits: Cell<usize>,
    pub(crate) qr_moves: Cell<usize>,
    pub(crate) reads: Cell<usize>,
    pub(crate) duplicates: Cell<usize>,
    pub(crate) uploads: RefCell<Vec<Vec<Complex64>>>,
}

/// A spy: [`CpuAlgebra`], counting its splits, moves of the centre, reads
/// and duplicates, and recording its uploads, into the [`SpiedCalls`] it
/// shares with the test.
#[derive(Clone)]
pub(crate) struct SpyAlgebra(Rc<SpiedCalls>);

impl SpyAlgebra {
    /// The spy, and the record of the calls it answers.
    pub(crate) fn new() -> (Self, Rc<SpiedCalls>) {
        let calls = Rc::new(SpiedCalls::default());
        (Self(Rc::clone(&calls)), calls)
    }
}

fn count(counter: &Cell<usize>) {
    counter.set(counter.get() + 1);
}

impl SiteAlgebra for SpyAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = SvdError;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        self.0.uploads.borrow_mut().push(values.to_vec());
        CpuAlgebra.upload(values)
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        count(&self.0.reads);
        CpuAlgebra.read(tensor, len)
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        count(&self.0.duplicates);
        CpuAlgebra.duplicate(tensor, len)
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        count(&self.0.gate_splits);
        CpuAlgebra.gate_split(call)
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        count(&self.0.qr_moves);
        CpuAlgebra.qr_move(call)
    }
}
