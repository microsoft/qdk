// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the size of a device buffer of complex numbers: one too large
//! to size is a resource limit. No public input reaches it, so it is
//! pinned here: no slice to upload holds that many values, and a longer
//! duplicate is refused first, for reading past its tensor's elements.
//! That an empty buffer is an invalid call is pinned where users meet it,
//! through the algebra (`src/engine/cuda_algebra/tests.rs`).

use super::*;

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
