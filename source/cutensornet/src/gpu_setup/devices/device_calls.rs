// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceCalls`]: the CUDA Runtime calls that read the devices, a seam so
//! that the setup check's rules run on a host with no GPU. The loaded
//! libraries implement it (`device_calls/loaded_libraries.rs`), and the
//! tests a fake.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod loaded_libraries;

use crate::CudaAlgebraError;

/// The CUDA Runtime calls that read the devices.
pub(crate) trait DeviceCalls {
    /// How many CUDA devices the process sees.
    fn device_count(&self) -> Result<i32, CudaAlgebraError>;
    /// Makes device `ordinal` current.
    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError>;
    /// The (major, minor) compute capability of device `ordinal`.
    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), CudaAlgebraError>;
    /// The current device's free and total memory, in bytes.
    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError>;
}
