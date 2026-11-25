// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceCalls`]: the CUDA Runtime calls that read the devices. It is a
//! seam, so that the setup check's rules run on a host with no GPU.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod loaded_libraries;

use crate::CudaAlgebraError;
use crate::libraries::setup_guide::ComputeCapability;

/// The CUDA Runtime calls that read the devices.
///
/// Each fails when the CUDA Runtime returns an error status, with an error
/// naming the call and that status. Such a failure is on the host's side
/// (no driver, a device held by another process), not a fault of the
/// check: the setup check reports it as the devices step's outcome, and
/// advises on the driver.
pub(crate) trait DeviceCalls {
    /// How many CUDA devices the process sees (`cudaGetDeviceCount`).
    fn device_count(&self) -> Result<i32, CudaAlgebraError>;
    /// Makes device `ordinal` current (`cudaSetDevice`).
    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError>;
    /// The compute capability of device `ordinal`
    /// (`cudaDeviceGetAttribute`, once for each part).
    fn compute_capability(&self, ordinal: i32) -> Result<ComputeCapability, CudaAlgebraError>;
    /// The current device's free and total memory, in bytes
    /// (`cudaMemGetInfo`).
    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError>;
}
