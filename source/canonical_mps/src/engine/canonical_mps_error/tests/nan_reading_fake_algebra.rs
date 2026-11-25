// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NanReadingFakeAlgebra`]: a fake backend algebra whose reads come back
//! NaN, as a device's may after an overflow, for the tests of what the
//! engine refuses on a probability of NaN.

use crate::CpuAlgebra;
use crate::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::SvdError;
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::Cell;

/// A fake: [`CpuAlgebra`], except that `read` returns NaN elements once it
/// has answered a chosen number of uploads.
#[derive(Clone)]
pub(crate) struct NanReadingFakeAlgebra {
    /// The uploads still to answer before every read returns NaN.
    uploads_left: Cell<usize>,
}

impl NanReadingFakeAlgebra {
    /// Reads NaN from the first read on.
    pub(crate) fn new() -> Self {
        Self::after_uploads(0)
    }

    /// Returns its reads stored until it has answered `uploads` uploads,
    /// then reads NaN, whatever the number and order of the reads.
    pub(crate) fn after_uploads(uploads: usize) -> Self {
        Self {
            uploads_left: Cell::new(uploads),
        }
    }
}

impl SiteAlgebra for NanReadingFakeAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = SvdError;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        self.uploads_left
            .set(self.uploads_left.get().saturating_sub(1));
        CpuAlgebra.upload(values)
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        if self.uploads_left.get() == 0 {
            Ok(Cow::Owned(vec![Complex64::new(f64::NAN, f64::NAN); len]))
        } else {
            CpuAlgebra.read(tensor, len)
        }
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
