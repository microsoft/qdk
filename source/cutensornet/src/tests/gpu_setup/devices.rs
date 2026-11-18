// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`read_devices`]: every device, in ordinal order, each read with its
//! own device current, and a failed read fails the report; and which
//! compute capabilities NVIDIA supports cuTensorNet on.

use std::cell::Cell;

use crate::CudaAlgebraError;
use crate::gpu_setup::devices::{DeviceCalls, DeviceReport, read_devices};

/// A fake of the CUDA Runtime's device calls: canned devices, as the CUDA
/// Runtime would report them, and a failure of each call on request. The
/// memory read is the current device's.
struct FakeDevices {
    /// Each device's compute capability, free and total bytes.
    devices: Vec<((i32, i32), usize, usize)>,
    current: Cell<Option<i32>>,
    fail_count: bool,
    fail_compute_capability: bool,
    /// The ordinal that cannot be made current, if any.
    fail_select: Option<i32>,
    fail_memory: bool,
}

impl FakeDevices {
    fn new(devices: Vec<((i32, i32), usize, usize)>) -> Self {
        Self {
            devices,
            current: Cell::new(None),
            fail_count: false,
            fail_compute_capability: false,
            fail_select: None,
            fail_memory: false,
        }
    }

    fn device(&self, ordinal: i32) -> ((i32, i32), usize, usize) {
        self.devices[usize::try_from(ordinal).expect("a valid ordinal")]
    }
}

impl DeviceCalls for FakeDevices {
    fn device_count(&self) -> Result<i32, CudaAlgebraError> {
        if self.fail_count {
            return Err(CudaAlgebraError::NoDevice);
        }
        Ok(i32::try_from(self.devices.len()).expect("a few devices"))
    }

    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError> {
        if self.fail_select == Some(ordinal) {
            return Err(CudaAlgebraError::NoDevice);
        }
        self.current.set(Some(ordinal));
        Ok(())
    }

    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), CudaAlgebraError> {
        if self.fail_compute_capability {
            return Err(CudaAlgebraError::NoDevice);
        }
        Ok(self.device(ordinal).0)
    }

    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError> {
        if self.fail_memory {
            return Err(CudaAlgebraError::NoDevice);
        }
        let current = self.current.get().expect("a device is current");
        let (_, free, total) = self.device(current);
        Ok((free, total))
    }
}

/// Two devices of different capabilities and memory are reported in
/// ordinal order, each with its own memory.
#[test]
fn each_device_is_reported_in_order_with_its_own_memory() {
    let libraries = FakeDevices::new(vec![((8, 0), 79, 80), ((7, 0), 15, 16)]);
    assert_eq!(
        read_devices(&libraries).expect("the devices are read"),
        vec![
            DeviceReport {
                ordinal: 0,
                compute_capability: (8, 0),
                free_bytes: 79,
                total_bytes: 80,
            },
            DeviceReport {
                ordinal: 1,
                compute_capability: (7, 0),
                free_bytes: 15,
                total_bytes: 16,
            },
        ]
    );
}

/// A host with no GPU reports no device, which is no failure: the setup
/// check, not the read, names it as a problem.
#[test]
fn a_host_with_no_device_reports_none() {
    assert_eq!(
        read_devices(&FakeDevices::new(Vec::new())).expect("no device is no failure"),
        Vec::new()
    );
}

/// A driver that cannot count the devices fails the report, rather than
/// reporting none.
#[test]
fn a_failed_device_count_fails_the_report() {
    let failing = FakeDevices {
        fail_count: true,
        ..FakeDevices::new(vec![((8, 0), 79, 80)])
    };
    assert!(read_devices(&failing).is_err());
}

/// A device whose memory cannot be read fails the report, rather than
/// being reported with no memory.
#[test]
fn a_device_that_cannot_be_read_fails_the_report() {
    let failing = FakeDevices {
        fail_memory: true,
        ..FakeDevices::new(vec![((8, 0), 79, 80)])
    };
    assert!(read_devices(&failing).is_err());
}

/// A device whose compute capability cannot be read fails the report,
/// rather than being reported as 0.0, which the setup check would call too
/// old, hiding the fault.
#[test]
fn a_device_whose_compute_capability_cannot_be_read_fails_the_report() {
    let failing = FakeDevices {
        fail_compute_capability: true,
        ..FakeDevices::new(vec![((8, 0), 79, 80)])
    };
    assert!(read_devices(&failing).is_err());
}

/// A device that cannot be made current fails the report, rather than
/// being reported with the memory of the device current before it: here
/// device 1 of two.
#[test]
fn a_device_that_cannot_be_selected_fails_the_report() {
    let failing = FakeDevices {
        fail_select: Some(1),
        ..FakeDevices::new(vec![((8, 0), 79, 80), ((7, 0), 15, 16)])
    };
    assert!(read_devices(&failing).is_err());
}

/// NVIDIA supports cuTensorNet on compute capability 7.5 and later: 6.1,
/// 7.0 and 7.4 are not supported, 7.5, 8.0 and 9.0 are.
#[test]
fn the_minimum_compute_capability_and_later_are_supported() {
    let device = |compute_capability| DeviceReport {
        ordinal: 0,
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
