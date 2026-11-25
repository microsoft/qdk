// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ZeroUploadingFakeAlgebra`]: a fake backend algebra that stores zeros
//! whatever it is given, as a device's lost memory may, for the tests of
//! what the engine refuses on a state whose norm is zero.

use crate::CpuAlgebra;
use crate::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::SvdError;
use num_complex::Complex64;
use std::borrow::Cow;

/// A fake: [`CpuAlgebra`], except that `upload` stores zeros. So a state
/// it prepares has a norm of zero, and its reads return those zeros.
#[derive(Clone)]
pub(crate) struct ZeroUploadingFakeAlgebra;

impl SiteAlgebra for ZeroUploadingFakeAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = SvdError;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        CpuAlgebra.upload(&vec![Complex64::ZERO; values.len()])
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        CpuAlgebra.read(tensor, len)
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
