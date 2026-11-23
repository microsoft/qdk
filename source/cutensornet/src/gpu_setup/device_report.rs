// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DeviceReport`]: the CUDA devices as a user setting the GPU up needs to
//! see them: how many the process sees, and the compute capability and
//! memory of the one the GPU engine runs on ([`DEVICE_ORDINAL`]).
//!
//! A library may run only on devices of some compute capability or later
//! (its setup guide says), and a run's max bond dimension is bounded by the
//! device's free memory, so the setup check reports both of the engine's
//! device. It reads no other device: reading a device's memory needs a
//! CUDA context on it, created when the device is made current and kept as
//! long as the process, and a device the engine does not use must not make
//! the setup look broken. Counting the devices creates no context.
//!
//! [`read_devices`] reads them through [`DeviceCalls`].

mod device_calls;

pub(crate) use self::device_calls::DeviceCalls;
use crate::CudaAlgebraError;
use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;

/// The CUDA devices, as the setup check reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceReport {
    /// How many CUDA devices the process sees.
    pub(crate) count: i32,
    /// The device the GPU engine runs on, [`DEVICE_ORDINAL`]; `None` when
    /// the process sees no such device.
    pub(crate) engine_device: Option<EngineDevice>,
}

/// The device the GPU engine runs on: what it offers a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EngineDevice {
    /// Its compute capability, (major, minor).
    pub(crate) compute_capability: (i32, i32),
    /// Its free memory, in bytes, when it was read.
    pub(crate) free_bytes: usize,
    /// Its total memory, in bytes.
    pub(crate) total_bytes: usize,
}

/// How many devices `calls` sees, and the GPU engine's device,
/// [`DEVICE_ORDINAL`], read with it current; no other device is selected
/// or read. The engine's device is left current, which the GPU engine,
/// selecting its device on every call, does not rely on.
///
/// # Errors
///
/// When a call fails: the CUDA Runtime cannot count the devices (with no
/// driver, say), or cannot select or read the engine's device.
pub(crate) fn read_devices(calls: &impl DeviceCalls) -> Result<DeviceReport, CudaAlgebraError> {
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
