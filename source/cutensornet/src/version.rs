// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The library versions discovery accepts: exactly the audited ones.
//!
//! The bindings were checked against cuTensorNet 2.13.0 and the CUDA
//! Runtime 12.9, and a newer library is not assumed compatible, so any other
//! version fails discovery before a function is called beyond the version
//! probes. cuTensorNet must also report that it was built against the CUDA
//! Runtime 12.9. The CUDA driver's version is not checked.

use crate::{AvailabilityError, AvailabilityReport};

/// cuTensorNet 2.13.0, as `cutensornetGetVersion` reports it.
const AUDITED_CUTENSORNET: usize = 21_300;
/// The CUDA Runtime 12.9, as CUDA reports it.
const AUDITED_CUDA_RUNTIME: i32 = 12_090;

/// Checks the versions of `report` against the audited ones, in this order:
/// cuTensorNet's, the CUDA Runtime's, and the CUDA Runtime's that
/// cuTensorNet was built against.
pub(crate) fn check_versions(report: &AvailabilityReport) -> Result<(), AvailabilityError> {
    check(
        "cuTensorNet",
        report.cutensornet_version as i128,
        AUDITED_CUTENSORNET as i128,
    )?;
    check(
        "CUDA Runtime",
        report.cuda_runtime_version.into(),
        AUDITED_CUDA_RUNTIME.into(),
    )?;
    check(
        "cuTensorNet's CUDA Runtime",
        report.cutensornet_cuda_runtime_version as i128,
        AUDITED_CUDA_RUNTIME.into(),
    )
}

fn check(component: &'static str, found: i128, supported: i128) -> Result<(), AvailabilityError> {
    if found == supported {
        Ok(())
    } else {
        Err(AvailabilityError::UnsupportedVersion {
            component,
            found,
            supported,
        })
    }
}
