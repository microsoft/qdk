// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FixedFractionFakeAlgebra`]: a fake backend algebra whose every split
//! reports the same retained fraction, for the tests of a retained weight
//! near the smallest normal `f64`: the reads keep their value, and a split
//! that would underflow the weight fails the engine's operation. One split
//! sets the weight a thousand halvings of a real truncation would.

use crate::CpuAlgebra;
use crate::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::SvdError;
use num_complex::Complex64;
use std::borrow::Cow;

/// A fake: [`CpuAlgebra`], except that every `gate_split` reports the
/// retained fraction it holds, in `(0, 1]`, whatever it kept.
#[derive(Clone)]
pub(crate) struct FixedFractionFakeAlgebra(pub(crate) f64);

impl SiteAlgebra for FixedFractionFakeAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = SvdError;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        CpuAlgebra.upload(values)
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
        Ok(GateSplitOutcome {
            retained_fraction: self.0,
            ..CpuAlgebra.gate_split(call)?
        })
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        CpuAlgebra.qr_move(call)
    }
}
