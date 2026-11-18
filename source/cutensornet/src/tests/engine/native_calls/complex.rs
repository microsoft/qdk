// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the size of a device buffer of complex numbers: an empty one is
//! the algebra's fault, and one too large to size is a resource limit.

use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::complex_bytes;

/// No tensor the algebra allocates or copies is empty, so asking for an
/// empty buffer, or an empty copy (the loaded libraries size each copy
/// with `complex_bytes` before calling CUDA), is an invalid call, refused
/// before any native call.
#[test]
fn an_empty_buffer_is_an_invalid_call() {
    assert!(matches!(
        complex_bytes(0, "complex tensor"),
        Err(CudaAlgebraError::InvalidNativeCall { .. })
    ));
}

/// A buffer whose size in bytes does not fit a `usize` overflows, named by
/// what it was for.
#[test]
fn a_buffer_too_large_to_size_overflows() {
    assert!(matches!(
        complex_bytes(usize::MAX, "complex tensor"),
        Err(CudaAlgebraError::ResourceSizeOverflow {
            resource: "complex tensor"
        })
    ));
}
