// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FailingFakeAlgebra`]: a fake, the CPU algebra ([`CpuAlgebra`])
//! except that the calls of one operation fail, for the tests of what the
//! engine does when its backend fails: [`CpuAlgebra`] fails only when its
//! SVD does, and the engine's tests give the SVD finite matrices, which it
//! answers.

use crate::CpuAlgebra;
use crate::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::SvdError;
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

/// An operation of [`SiteAlgebra`], one of its methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Upload,
    Read,
    Duplicate,
    GateSplit,
    QrMove,
}

/// The failure of a [`FailingFakeAlgebra`]'s call: the operation that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the failing fake algebra's {0:?} failed")]
pub(crate) struct AlgebraFailure(pub(crate) Operation);

/// [`CpuAlgebra`], except that the call of `operation` numbered `failing`
/// (from 0) fails, or every call of it when `failing` is `None`; `asked`
/// counts the calls of `operation` asked of it.
#[derive(Clone)]
pub(crate) struct FailingFakeAlgebra {
    operation: Operation,
    failing: Option<usize>,
    asked: Rc<Cell<usize>>,
}

impl FailingFakeAlgebra {
    /// A fake on which every call of `operation` fails.
    pub(crate) fn every(operation: Operation) -> Self {
        Self {
            operation,
            failing: None,
            asked: Rc::default(),
        }
    }

    /// A fake on which the call of `operation` numbered `failing` (from 0)
    /// fails, and the count of the calls of `operation` asked of it.
    pub(crate) fn on_call(operation: Operation, failing: usize) -> (Self, Rc<Cell<usize>>) {
        let asked = Rc::new(Cell::new(0));
        let algebra = Self {
            operation,
            failing: Some(failing),
            asked: Rc::clone(&asked),
        };
        (algebra, asked)
    }

    /// Fails when the call of `operation` is the failing one.
    fn call(&self, operation: Operation) -> Result<(), AlgebraFailure> {
        if operation != self.operation {
            return Ok(());
        }
        let number = self.asked.get();
        self.asked.set(number + 1);
        if self.failing.is_some_and(|failing| failing != number) {
            Ok(())
        } else {
            Err(AlgebraFailure(operation))
        }
    }
}

/// [`CpuAlgebra`]'s answer, which fails only when its SVD does.
fn cpu_answer<T>(answer: Result<T, SvdError>) -> T {
    answer.expect("the engine's tests give the SVD small, finite matrices, on which it converges")
}

impl SiteAlgebra for FailingFakeAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = AlgebraFailure;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        self.call(Operation::Upload)?;
        Ok(cpu_answer(CpuAlgebra.upload(values)))
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        self.call(Operation::Read)?;
        Ok(cpu_answer(CpuAlgebra.read(tensor, len)))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        self.call(Operation::Duplicate)?;
        Ok(cpu_answer(CpuAlgebra.duplicate(tensor, len)))
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        self.call(Operation::GateSplit)?;
        Ok(cpu_answer(CpuAlgebra.gate_split(call)))
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        self.call(Operation::QrMove)?;
        Ok(cpu_answer(CpuAlgebra.qr_move(call)))
    }
}
