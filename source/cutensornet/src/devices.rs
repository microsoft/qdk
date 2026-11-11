// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceReport`]: each CUDA device's compute capability and memory, as a
//! user setting the GPU up needs to see them.
//!
//! NVIDIA supports cuTensorNet on devices of compute capability
//! [`MINIMUM_COMPUTE_CAPABILITY`] or later (the crate's README says where it
//! was verified), and a run's max bond dimension is bounded by the device's
//! free memory, so the setup check ([`diagnose`](crate::diagnose)) lists every
//! device with both, and names a device the engine cannot use.
//! `DeviceReport::read_all` reads them through the CUDA Runtime that a
//! discovery loaded (`cudaGetDeviceCount`, then per device `cudaSetDevice`,
//! `cudaDeviceGetAttribute` and `cudaMemGetInfo`), behind [`DeviceCalls`]
//! so that the rules run on a host with no GPU; `native.rs` implements it
//! with the loaded libraries (Linux x86-64).

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod native;

use crate::EngineError;

/// One CUDA device, as the setup check reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceReport {
    /// The device's CUDA ordinal; the engine runs on ordinal 0.
    pub ordinal: i32,
    /// Its compute capability, (major, minor).
    pub compute_capability: (i32, i32),
    /// Its free memory, in bytes, when it was read.
    pub free_bytes: usize,
    /// Its total memory, in bytes.
    pub total_bytes: usize,
}

impl DeviceReport {
    /// Every CUDA device, with its compute capability and memory, in
    /// ordinal order, read through the CUDA Runtime `availability` loaded:
    /// what the setup check reports. Each is read with its own device
    /// current, so the last one is left current; the engine selects its own
    /// device on every call.
    ///
    /// # Errors
    ///
    /// When the CUDA Runtime fails to count or read the devices (with no
    /// device or no driver, say).
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn read_all(availability: &crate::Availability) -> Result<Vec<Self>, EngineError> {
        device_reports(availability.libraries().as_ref())
    }

    /// Whether NVIDIA supports cuTensorNet on it: compute capability at
    /// least [`MINIMUM_COMPUTE_CAPABILITY`].
    #[must_use]
    pub fn is_supported(&self) -> bool {
        self.compute_capability >= MINIMUM_COMPUTE_CAPABILITY
    }
}

/// The oldest compute capability NVIDIA supports cuTensorNet on: 7.5. The
/// engine does not check it; the setup check applies it to device 0, where
/// the engine runs.
pub const MINIMUM_COMPUTE_CAPABILITY: (i32, i32) = (7, 5);

/// Every device `calls` sees, in ordinal order, each read with its own
/// device current.
pub(crate) fn device_reports(calls: &impl DeviceCalls) -> Result<Vec<DeviceReport>, EngineError> {
    (0..calls.device_count()?)
        .map(|ordinal| {
            calls.set_device(ordinal)?;
            let (free_bytes, total_bytes) = calls.memory_info()?;
            Ok(DeviceReport {
                ordinal,
                compute_capability: calls.compute_capability(ordinal)?,
                free_bytes,
                total_bytes,
            })
        })
        .collect()
}

/// The CUDA Runtime calls that read the devices.
pub(crate) trait DeviceCalls {
    fn device_count(&self) -> Result<i32, EngineError>;
    fn set_device(&self, ordinal: i32) -> Result<(), EngineError>;
    /// The (major, minor) compute capability of device `ordinal`.
    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), EngineError>;
    /// The current device's free and total memory, in bytes.
    fn memory_info(&self) -> Result<(usize, usize), EngineError>;
}
