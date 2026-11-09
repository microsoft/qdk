// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the device session: an ordinal no device answers is refused
//! before any device is selected, the device is selected before the stream
//! and the handle are created, a failure releases what was created, and
//! dropping releases the handle before the stream.

use crate::engine::{DeviceSession, EngineError, native_calls::fake_libraries::FakeLibraries};
use std::sync::Arc;

/// Opening a session on device 0 of a host with one device selects it
/// before the stream and the handle are created on it.
#[test]
fn the_device_is_selected_before_the_stream_and_the_handle() {
    let libraries = Arc::new(FakeLibraries::new());
    let _session = DeviceSession::new(libraries.clone(), 0)
        .unwrap_or_else(|error| panic!("device 0 of one opens a session: {error}"));
    assert_eq!(
        libraries.events(),
        [
            "device_count",
            "set_device",
            "create_stream",
            "create_handle"
        ]
    );
}

/// The handle's work runs on the stream, so dropping a session waits for
/// the stream, then releases the handle before the stream.
#[test]
fn dropping_releases_the_handle_before_the_stream() {
    let libraries = Arc::new(FakeLibraries::new());
    let session = DeviceSession::new(libraries.clone(), 0)
        .unwrap_or_else(|error| panic!("device 0 of one opens a session: {error}"));
    let opened = libraries.events().len();
    drop(session);
    assert_eq!(
        libraries.events()[opened..],
        [
            "set_device",
            "synchronize_stream",
            "destroy_handle",
            "destroy_stream"
        ]
    );
    assert_eq!(libraries.live_objects(), 0, "nothing left live");
}

/// A session whose handle cannot be created reports that failure and
/// releases the stream it had created.
#[test]
fn a_failed_handle_creation_releases_the_stream() {
    let libraries = Arc::new(FakeLibraries::new());
    libraries.fail("create_handle", 1);
    let opened = DeviceSession::new(libraries.clone(), 0);
    assert!(
        matches!(
            opened,
            Err(EngineError::NativeCallFailed {
                operation: "create_handle",
                ..
            })
        ),
        "{:?}",
        opened.err()
    );
    assert_eq!(libraries.live_objects(), 0, "the stream is released");
}

/// A host whose CUDA runtime finds no device has no device 0.
#[test]
fn a_host_without_a_cuda_device_is_refused() {
    let libraries = Arc::new(FakeLibraries::new());
    libraries.set_device_count(0);
    assert!(matches!(
        DeviceSession::new(libraries.clone(), 0),
        Err(EngineError::NoDevice)
    ));
    assert_eq!(libraries.events(), ["device_count"], "no device selected");
}

/// On a host with one device, ordinal 1 names no device; it is refused
/// before any device is selected.
#[test]
fn an_ordinal_beyond_the_devices_is_refused_before_selection() {
    let libraries = Arc::new(FakeLibraries::new());
    assert!(matches!(
        DeviceSession::new(libraries.clone(), 1),
        Err(EngineError::NoDevice)
    ));
    assert_eq!(libraries.events(), ["device_count"], "no device selected");
}

/// A negative ordinal names no device; it is refused before any device is
/// selected.
#[test]
fn a_negative_ordinal_is_refused_before_selection() {
    let libraries = Arc::new(FakeLibraries::new());
    assert!(matches!(
        DeviceSession::new(libraries.clone(), -1),
        Err(EngineError::NoDevice)
    ));
    assert_eq!(libraries.events(), ["device_count"], "no device selected");
}
