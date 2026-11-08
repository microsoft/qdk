// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`device_reports`]: every device, in ordinal order, each read with its
//! own device current; and which compute capabilities cuTensorNet runs on.

use std::cell::Cell;

use crate::devices::{DeviceCalls, device_reports};
use crate::{DeviceReport, EngineError};

/// Devices of a host, as the CUDA Runtime would report them: the memory
/// read is the current device's.
struct Host {
    /// Each device's compute capability, free and total bytes.
    devices: Vec<((i32, i32), usize, usize)>,
    current: Cell<Option<i32>>,
    fail_count: bool,
}

impl Host {
    fn new(devices: Vec<((i32, i32), usize, usize)>) -> Self {
        Self {
            devices,
            current: Cell::new(None),
            fail_count: false,
        }
    }

    fn device(&self, ordinal: i32) -> ((i32, i32), usize, usize) {
        self.devices[usize::try_from(ordinal).expect("a valid ordinal")]
    }
}

impl DeviceCalls for Host {
    fn device_count(&self) -> Result<i32, EngineError> {
        if self.fail_count {
            return Err(EngineError::NoDevice);
        }
        Ok(i32::try_from(self.devices.len()).expect("a few devices"))
    }

    fn set_device(&self, ordinal: i32) -> Result<(), EngineError> {
        self.current.set(Some(ordinal));
        Ok(())
    }

    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), EngineError> {
        Ok(self.device(ordinal).0)
    }

    fn memory_info(&self) -> Result<(usize, usize), EngineError> {
        let current = self.current.get().expect("a device is current");
        let (_, free, total) = self.device(current);
        Ok((free, total))
    }
}

/// Two devices of different capabilities and memory are reported in
/// ordinal order, each with its own memory.
#[test]
fn each_device_is_reported_in_order_with_its_own_memory() {
    let host = Host::new(vec![((8, 0), 79, 80), ((7, 0), 15, 16)]);
    assert_eq!(
        device_reports(&host).expect("the devices are read"),
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

/// A host of no device reports none; a count that fails fails the report.
#[test]
fn no_device_reports_none_and_a_failed_count_fails() {
    assert!(
        device_reports(&Host::new(Vec::new()))
            .expect("no device is no failure")
            .is_empty()
    );
    let failing = Host {
        fail_count: true,
        ..Host::new(Vec::new())
    };
    assert!(device_reports(&failing).is_err());
}

/// cuTensorNet runs on compute capability 7.5 and later: 7.0 and 6.1 are
/// not supported, 7.5, 8.0 and 9.0 are.
#[test]
fn compute_capability_7_5_and_later_is_supported() {
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
