// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceReport`]: the CUDA devices as a user setting the GPU up needs to
//! see them: how many the process sees, and the compute capability and
//! memory of the one the GPU engine runs on ([`DEVICE_ORDINAL`]).
//!
//! A library's setup guide sets the oldest compute capability it runs on,
//! if any. A run's max bond dimension is bounded by the device's free
//! memory. So the setup check reports the engine's device's compute
//! capability and free memory. It reads no other device. Reading a
//! device's memory needs a CUDA context on it: making the device current
//! creates one, kept as long as the process. A device the engine does not
//! use must not make the setup look broken. Counting the devices creates
//! no context.
//!
//! [`read_devices`] reads them through [`DeviceCalls`].

mod device_calls;

pub(super) use self::device_calls::DeviceCalls;
use crate::CudaAlgebraError;
use crate::engine::cuda_algebra::settings::DEVICE_ORDINAL;
use crate::libraries::setup_guide::ComputeCapability;

/// The CUDA devices, as the setup check reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DeviceReport {
    /// How many CUDA devices the process sees.
    pub(super) count: i32,
    /// The device the GPU engine runs on, [`DEVICE_ORDINAL`], or `None`
    /// when the process sees no such device.
    pub(super) engine_device: Option<EngineDevice>,
}

/// The device the GPU engine runs on: what it offers a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EngineDevice {
    /// Its compute capability.
    pub(super) compute_capability: ComputeCapability,
    /// Its free memory, in bytes, when it was read.
    pub(super) free_bytes: usize,
    /// Its total memory, in bytes.
    pub(super) total_bytes: usize,
}

/// How many devices `calls` sees, and the GPU engine's device,
/// [`DEVICE_ORDINAL`], read with it current. No other device is selected
/// or read. The engine's device is left current; the GPU engine selects
/// its device on every call, so it does not rely on that.
///
/// # Errors
///
/// When a call fails: the CUDA Runtime cannot count the devices (with no
/// driver, say), or cannot select or read the engine's device. The error
/// is the CUDA Runtime's, usually from the host's setup (its driver or
/// device). It replaces the whole reading, since a partial one would hide
/// the fault.
pub(super) fn read_devices(calls: &impl DeviceCalls) -> Result<DeviceReport, CudaAlgebraError> {
    let count = calls.device_count()?;
    let engine_device = if (0..count).contains(&DEVICE_ORDINAL) {
        calls.set_device(DEVICE_ORDINAL)?;
        let (free_bytes, total_bytes) = calls.memory_info()?;
        Some(EngineDevice {
            compute_capability: calls.compute_capability(DEVICE_ORDINAL)?,
            free_bytes,
            total_bytes,
        })
    } else {
        None
    };
    Ok(DeviceReport {
        count,
        engine_device,
    })
}

#[cfg(test)]
mod tests;
