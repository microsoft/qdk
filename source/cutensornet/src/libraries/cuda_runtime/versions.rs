// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's versions: its own, and the driver's. Each type is the
//! pointer type of the C function of its name; both are a `VersionProbeFn`.

use super::cudart_12;
use std::ffi::c_int;

/// A CUDA version function: it writes the version to its argument and
/// returns CUDA's status.
pub(crate) type VersionProbeFn = unsafe extern "C" fn(*mut c_int) -> cudart_12::cudaError_t;

pub(crate) type cudaRuntimeGetVersion = VersionProbeFn;

pub(crate) type cudaDriverGetVersion = VersionProbeFn;
