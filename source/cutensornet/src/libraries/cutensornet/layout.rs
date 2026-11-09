// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compile-time checks of the layout of `cuDoubleComplex`, the one type a
//! cuTensorNet function takes by value.
//!
//! `cutensornetNetworkOperatorAppendProduct` takes its coefficient as a
//! `cuDoubleComplex` by value, so a wrong size, alignment or field offset in
//! the Rust declaration would pass the call a wrong coefficient, with no
//! error. The type must have the layout of NVIDIA's `double2`: 16 bytes,
//! aligned to 16, the real part `x` at offset 0 and the imaginary part `y` at
//! 8. These assertions fail the build otherwise; a new cuTensorNet or CUDA
//! release is checked against them.

use super::v2_13;
use std::mem::{align_of, offset_of, size_of};

const _: () = assert!(size_of::<v2_13::cuDoubleComplex>() == 16);
const _: () = assert!(align_of::<v2_13::cuDoubleComplex>() == 16);
const _: () = assert!(offset_of!(v2_13::cuDoubleComplex, x) == 0);
const _: () = assert!(offset_of!(v2_13::cuDoubleComplex, y) == 8);
