// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NanReadingFakeAlgebra`]: a fake backend algebra whose reads come back
//! NaN, as a device's may after an overflow, for the tests of what the
//! engine refuses on a probability of NaN.

use crate::CpuAlgebra;
use crate::dense_linalg::SvdError;
use crate::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use num_complex::Complex64;
use std::borrow::Cow;

/// A fake: [`CpuAlgebra`], except that `read` returns NaN elements.
#[derive(Clone)]
pub(crate) struct NanReadingFakeAlgebra;

impl SiteAlgebra for NanReadingFakeAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = SvdError;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        CpuAlgebra.upload(values)
    }

    fn read<'t>(
        &self,
        _: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        Ok(Cow::Owned(vec![Complex64::new(f64::NAN, f64::NAN); len]))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        CpuAlgebra.duplicate(tensor, len)
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        CpuAlgebra.gate_split(call)
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        CpuAlgebra.qr_move(call)
    }
}
