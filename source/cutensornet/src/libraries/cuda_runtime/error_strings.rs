// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's error text: the text of a status. Each `XFn` is the
//! type of `cudaX`.

use super::cudart_12;
use std::ffi::c_char;

pub(crate) type GetErrorStringFn = unsafe extern "C" fn(cudart_12::cudaError_t) -> *const c_char;
