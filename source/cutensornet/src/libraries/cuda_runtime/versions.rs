// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's versions: its own, and the driver's. Each `XFn` is the
//! type of `cudaX`; both are a `VersionProbeFn`.

use super::cudart_12;
use std::ffi::c_int;

/// A CUDA version function: it writes the version to its argument and
/// returns CUDA's status.
pub(crate) type VersionProbeFn = unsafe extern "C" fn(*mut c_int) -> cudart_12::cudaError_t;

pub(crate) type RuntimeGetVersionFn = VersionProbeFn;

pub(crate) type DriverGetVersionFn = VersionProbeFn;
