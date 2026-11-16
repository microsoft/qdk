// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the size of a device buffer of complex numbers: an empty one is
//! the engine's fault, and one too large to size is a resource limit.

use crate::native_calls::complex_bytes;
use crate::native_calls::error::NativeError;

/// No tensor the engine allocates or copies is empty, so asking for an
/// empty buffer is an invalid call, refused before any native call.
#[test]
fn an_empty_buffer_is_an_invalid_call() {
    assert!(matches!(
        complex_bytes(0, "operator tensor"),
        Err(NativeError::InvalidNativeCall { .. })
    ));
}

/// A buffer whose size in bytes does not fit a `usize` overflows, named by
/// what it was for.
#[test]
fn a_buffer_too_large_to_size_overflows() {
    assert!(matches!(
        complex_bytes(usize::MAX, "complex tensor"),
        Err(NativeError::ResourceSizeOverflow {
            resource: "complex tensor"
        })
    ));
}
