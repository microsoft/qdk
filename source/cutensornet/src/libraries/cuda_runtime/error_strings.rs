// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's error text: the text of a status. Each type is the
//! pointer type of the C function of its name.

use super::cudart_12;
use std::ffi::c_char;

pub(crate) type cudaGetErrorString = unsafe extern "C" fn(cudart_12::cudaError_t) -> *const c_char;
