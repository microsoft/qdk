// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the device session: the device is selected before the
//! stream and the handle are created, a failure releases what was created,
//! and dropping releases the handle before the stream.

use crate::engine::{
    EngineError, OpaqueHandle, Stream,
    double::{DeviceSession, SessionCalls},
};
use std::{ptr::NonNull, sync::Arc, sync::Mutex};

struct DoubleSessionCalls {
    events: Mutex<Vec<&'static str>>,
    device_count: i32,
    fail_handle_creation: bool,
}

impl DoubleSessionCalls {
    fn new(device_count: i32, fail_handle_creation: bool) -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            device_count,
            fail_handle_creation,
        }
    }

    fn record(&self, event: &'static str) {
        self.events
            .lock()
            .expect("event lock should succeed")
            .push(event);
    }

    fn events(&self) -> Vec<&'static str> {
        self.events
            .lock()
            .expect("event lock should succeed")
            .clone()
    }
}

impl SessionCalls for DoubleSessionCalls {
    fn device_count(&self) -> Result<i32, EngineError> {
        self.record("device_count");
        Ok(self.device_count)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), EngineError> {
        assert_eq!(ordinal, 0);
        self.record("set_device");
        Ok(())
    }

    fn create_stream(&self) -> Result<Stream, EngineError> {
        self.record("create_stream");
        Ok(NonNull::dangling())
    }

    fn synchronize_stream(&self, _stream: Stream) -> Result<(), EngineError> {
        self.record("synchronize_stream");
        Ok(())
    }

    fn destroy_stream(&self, _stream: Stream) -> Result<(), EngineError> {
        self.record("destroy_stream");
        Ok(())
    }

    fn create_handle(&self) -> Result<OpaqueHandle, EngineError> {
        self.record("create_handle");
        if self.fail_handle_creation {
            Err(EngineError::NativeCallFailed {
                component: "cuTensorNet",
                operation: "cutensornetCreate",
                status: 13,
                message: "simulated failure".to_string(),
            })
        } else {
            Ok(NonNull::dangling())
        }
    }

    fn destroy_handle(&self, _handle: OpaqueHandle) -> Result<(), EngineError> {
        self.record("destroy_handle");
        Ok(())
    }
}

#[test]
fn session_selects_device_and_drops_in_dependency_order() {
    let calls = Arc::new(DoubleSessionCalls::new(1, false));
    let session = DeviceSession::new(calls.clone(), 0).expect("the double opens a session");
    drop(session);

    assert_eq!(
        calls.events(),
        [
            "device_count",
            "set_device",
            "create_stream",
            "create_handle",
            "set_device",
            "synchronize_stream",
            "destroy_handle",
            "destroy_stream",
        ]
    );
}

#[test]
fn failed_handle_creation_cleans_up_the_stream() {
    let calls = Arc::new(DoubleSessionCalls::new(1, true));
    let error = DeviceSession::new(calls.clone(), 0)
        .err()
        .expect("handle creation should fail");
    assert!(matches!(
        error,
        EngineError::NativeCallFailed {
            operation: "cutensornetCreate",
            status: 13,
            ..
        }
    ));
    assert_eq!(
        calls.events(),
        [
            "device_count",
            "set_device",
            "create_stream",
            "create_handle",
            "set_device",
            "synchronize_stream",
            "destroy_stream",
        ]
    );
}

#[test]
fn session_rejects_a_host_without_a_cuda_device() {
    let calls = Arc::new(DoubleSessionCalls::new(0, false));
    assert!(matches!(
        DeviceSession::new(calls.clone(), 0),
        Err(EngineError::NoDevice)
    ));
    assert_eq!(calls.events(), ["device_count"]);
}

#[test]
fn session_rejects_an_unavailable_device_ordinal_before_selection() {
    let calls = Arc::new(DoubleSessionCalls::new(1, false));

    assert!(matches!(
        DeviceSession::new(calls.clone(), 1),
        Err(EngineError::NoDevice)
    ));
    assert_eq!(calls.events(), ["device_count"]);
}
