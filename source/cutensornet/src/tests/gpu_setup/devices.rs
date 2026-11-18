// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`read_devices`]: how many devices there are, and the GPU engine's
//! device, read with it current, and no other device; a failed call fails
//! the report; and which compute capabilities NVIDIA supports cuTensorNet
//! on.

use std::cell::Cell;

use crate::CudaAlgebraError;
use crate::gpu_setup::devices::{DeviceCalls, DeviceReport, EngineDevice, read_devices};

/// A call of [`DeviceCalls`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Count,
    Select,
    ComputeCapability,
    Memory,
}

/// A fake of the CUDA Runtime's device calls: canned devices, as the CUDA
/// Runtime would report them, a call that fails on request, and a device
/// on which every call fails. The memory read is the current device's.
struct FakeDevices {
    /// Each device's compute capability, free and total bytes.
    devices: Vec<((i32, i32), usize, usize)>,
    current: Cell<Option<i32>>,
    /// The call that fails, on every device, if any.
    failing: Option<Call>,
    /// The device on which every call fails, if any: one held by another
    /// process, say.
    broken_device: Option<i32>,
}

impl FakeDevices {
    fn new(devices: Vec<((i32, i32), usize, usize)>) -> Self {
        Self {
            devices,
            current: Cell::new(None),
            failing: None,
            broken_device: None,
        }
    }

    fn device(&self, ordinal: i32) -> ((i32, i32), usize, usize) {
        self.devices[usize::try_from(ordinal).expect("a valid ordinal")]
    }

    /// Fails when `call` is the failing call, or is made on the broken
    /// device.
    fn check(&self, call: Call, ordinal: Option<i32>) -> Result<(), CudaAlgebraError> {
        if self.failing == Some(call) || (ordinal.is_some() && ordinal == self.broken_device) {
            return Err(CudaAlgebraError::NoDevice);
        }
        Ok(())
    }
}

impl DeviceCalls for FakeDevices {
    fn device_count(&self) -> Result<i32, CudaAlgebraError> {
        self.check(Call::Count, None)?;
        Ok(i32::try_from(self.devices.len()).expect("a few devices"))
    }

    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError> {
        self.check(Call::Select, Some(ordinal))?;
        self.current.set(Some(ordinal));
        Ok(())
    }

    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), CudaAlgebraError> {
        self.check(Call::ComputeCapability, Some(ordinal))?;
        Ok(self.device(ordinal).0)
    }

    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError> {
        let current = self.current.get().expect("a device is current");
        self.check(Call::Memory, Some(current))?;
        let (_, free, total) = self.device(current);
        Ok((free, total))
    }
}

/// A host with two devices, the second current before the check, reports
/// both counted and CUDA device 0, the GPU engine's, with its own compute
/// capability and memory.
#[test]
fn the_engine_device_is_reported_with_its_own_memory() {
    let devices = FakeDevices::new(vec![((8, 0), 79, 80), ((7, 0), 15, 16)]);
    devices.current.set(Some(1));
    assert_eq!(
        read_devices(&devices).expect("the devices are read"),
        DeviceReport {
            count: 2,
            engine_device: Some(EngineDevice {
                compute_capability: (8, 0),
                free_bytes: 79,
                total_bytes: 80,
            }),
        }
    );
}

/// A second device on which every call fails, held by another process,
/// say, leaves the report whole: only the GPU engine's device is read.
#[test]
fn a_device_the_engine_does_not_use_is_not_read() {
    let devices = FakeDevices {
        broken_device: Some(1),
        ..FakeDevices::new(vec![((8, 0), 79, 80), ((7, 0), 15, 16)])
    };
    let report = read_devices(&devices).expect("device 1 is not read");
    assert_eq!(report.count, 2);
}

/// A host with no GPU reports no device, which is no failure: the setup
/// check, not the read, names it as a problem.
#[test]
fn a_host_with_no_device_reports_none() {
    assert_eq!(
        read_devices(&FakeDevices::new(Vec::new())).expect("no device is no failure"),
        DeviceReport {
            count: 0,
            engine_device: None,
        }
    );
}

/// Each call that fails fails the report, rather than leaving out what it
/// reads: a count read as none, a device reported with the memory of the
/// device current before it, or with compute capability 0.0, which the
/// setup check would call too old, hiding the fault.
#[test]
fn a_failed_call_fails_the_report() {
    for call in [
        Call::Count,
        Call::Select,
        Call::ComputeCapability,
        Call::Memory,
    ] {
        let devices = FakeDevices {
            failing: Some(call),
            ..FakeDevices::new(vec![((8, 0), 79, 80)])
        };
        devices.current.set(Some(0));
        assert!(read_devices(&devices).is_err(), "{call:?}");
    }
}

/// NVIDIA supports cuTensorNet on compute capability 7.5 and later: 6.1,
/// 7.0 and 7.4 are not supported, 7.5, 8.0 and 9.0 are.
#[test]
fn the_minimum_compute_capability_and_later_are_supported() {
    let device = |compute_capability| EngineDevice {
        compute_capability,
        free_bytes: 0,
        total_bytes: 0,
    };
    for unsupported in [(6, 1), (7, 0), (7, 4)] {
        assert!(!device(unsupported).is_supported(), "{unsupported:?}");
    }
    for supported in [(7, 5), (8, 0), (9, 0)] {
        assert!(device(supported).is_supported(), "{supported:?}");
    }
}
