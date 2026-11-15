// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FailingAlgebra`]: [`FakeAlgebra`], except that the calls of one
//! method fail, for the tests of what the engine does when its backend
//! fails, since no input makes the test algebra itself fail.

use crate::algebra::fake_algebra::FakeAlgebra;
use crate::algebra::{GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

/// A method of [`SiteAlgebra`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Method {
    Upload,
    Read,
    Duplicate,
    GateSplit,
    QrMove,
}

/// The failure of a [`FailingAlgebra`]'s call: the method that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the test algebra's {0:?} failed")]
pub(super) struct AlgebraFailure(pub(super) Method);

/// [`FakeAlgebra`], except that the call of `method` numbered `failing`
/// (from 0) fails, or every call of it when `failing` is `None`; `asked`
/// counts the calls of `method` asked of it.
#[derive(Clone)]
pub(super) struct FailingAlgebra {
    method: Method,
    failing: Option<usize>,
    asked: Rc<Cell<usize>>,
}

impl FailingAlgebra {
    /// An algebra on which every call of `method` fails.
    pub(super) fn every(method: Method) -> Self {
        Self {
            method,
            failing: None,
            asked: Rc::default(),
        }
    }

    /// An algebra on which the call of `method` numbered `failing` fails,
    /// and the count of the calls of `method` asked of it.
    pub(super) fn the(method: Method, failing: usize) -> (Self, Rc<Cell<usize>>) {
        let asked = Rc::new(Cell::new(0));
        let algebra = Self {
            method,
            failing: Some(failing),
            asked: Rc::clone(&asked),
        };
        (algebra, asked)
    }

    /// Fails when the call of `method` is the failing one.
    fn call(&self, method: Method) -> Result<(), AlgebraFailure> {
        if method != self.method {
            return Ok(());
        }
        let number = self.asked.get();
        self.asked.set(number + 1);
        if self.failing.is_some_and(|failing| failing != number) {
            Ok(())
        } else {
            Err(AlgebraFailure(method))
        }
    }
}

/// The test algebra's answer, which never fails.
fn fake<T>(answer: Result<T, std::convert::Infallible>) -> T {
    match answer {
        Ok(answer) => answer,
    }
}

impl SiteAlgebra for FailingAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = AlgebraFailure;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        self.call(Method::Upload)?;
        Ok(fake(FakeAlgebra.upload(values)))
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        self.call(Method::Read)?;
        Ok(fake(FakeAlgebra.read(tensor, len)))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        self.call(Method::Duplicate)?;
        Ok(fake(FakeAlgebra.duplicate(tensor, len)))
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        self.call(Method::GateSplit)?;
        Ok(fake(FakeAlgebra.gate_split(call)))
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        self.call(Method::QrMove)?;
        Ok(fake(FakeAlgebra.qr_move(call)))
    }
}
