// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The real native calls, the ones the engine makes on a GPU:
//! `SessionCalls`, `MemoryCalls` and `TensorCalls`, implemented for
//! `LoadedLibraries` (`crate::libraries`) through the function tables
//! discovery looked up.
//! Each trait has its own file (`loaded_libraries/session_calls.rs`,
//! `loaded_libraries/memory_calls.rs`, `loaded_libraries/tensor_calls.rs`).
//! This file holds what they share: turning a status into a
//! `CudaAlgebraError` with the library's message.
//!
//! This module holds every call the engine makes to cuTensorNet and the
//! CUDA Runtime, so it keeps their foreign-function obligations. Each
//! `unsafe` call says what makes it sound: the handles live, the buffers
//! large enough and kept alive, the values in the audited representation.
//!
//! - It owns nothing: its callers create the streams, handles, objects and
//!   buffers, and release them.
//! - It assumes cuTensorNet 2.13 and the CUDA Runtime 12.9, which discovery
//!   has checked.
//! - It compiles only on Linux x86-64, and no host test reaches it.
//! - A new native call needs its function's row in its library's table
//!   first (`libraries::cuda_runtime::CudaFunctions`,
//!   `libraries::cutensornet::CuTensorNetFunctions`).

use crate::engine::CudaAlgebraError;
use crate::engine::native_calls::NativePointer;
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12, cutensornet::v2_13};
use std::{
    ffi::{CStr, c_void},
    ptr::NonNull,
};

mod memory_calls;
mod session_calls;
mod tensor_calls;

impl LoadedLibraries {
    /// CUDA's description of `status` (`cudaGetErrorString`).
    fn cuda_message(&self, status: cudart_12::cudaError_t) -> String {
        // SAFETY: the pointer was resolved with the audited CUDA signature and
        // the returned library-owned string is copied before this call returns.
        let message = unsafe { (self.cuda_runtime.functions.cudaGetErrorString)(status) };
        copy_error_message(message, "CUDA returned a null error string")
    }

    /// cuTensorNet's description of `status` (`cutensornetGetErrorString`),
    /// followed by the detail of the last error (`cutensornetGetLastError`),
    /// if the loaded version provides that function and it returns something.
    fn cutensornet_message(&self, status: v2_13::cutensornetStatus_t) -> String {
        // SAFETY: both pointers were resolved with their audited signatures;
        // returned library-owned strings are copied immediately.
        let stable = unsafe { (self.cutensornet.functions.cutensornetGetErrorString)(status) };
        let mut message = copy_error_message(stable, "cuTensorNet returned a null error string");
        if let Some(get_last_error) = self.cutensornet.functions.cutensornetGetLastError {
            // SAFETY: the optional pointer has its exact no-argument signature.
            let detail = copy_error_message(unsafe { get_last_error() }, "");
            if !detail.is_empty() {
                message.push_str(": ");
                message.push_str(&detail);
            }
        }
        message
    }

    /// `Ok` if the CUDA Runtime call `operation` succeeded; otherwise the
    /// error, with CUDA's message for `status`. Every CUDA Runtime call on
    /// `LoadedLibraries` that returns a status ends with it, so that every
    /// failure reads the same.
    pub(crate) fn check_cuda(
        &self,
        operation: &'static str,
        status: cudart_12::cudaError_t,
    ) -> Result<(), CudaAlgebraError> {
        if status == cudart_12::cudaError_cudaSuccess {
            Ok(())
        } else {
            Err(CudaAlgebraError::NativeCallFailed {
                component: "CUDA Runtime",
                operation,
                status,
                message: self.cuda_message(status),
            })
        }
    }

    /// `Ok` if the cuTensorNet call `operation` succeeded; otherwise the
    /// error, with cuTensorNet's message for `status`. Every cuTensorNet
    /// call in `loaded_libraries/` ends with it.
    fn check_cutensornet(
        &self,
        operation: &'static str,
        status: v2_13::cutensornetStatus_t,
    ) -> Result<(), CudaAlgebraError> {
        if status == v2_13::cutensornetStatus_t_CUTENSORNET_STATUS_SUCCESS {
            Ok(())
        } else {
            Err(CudaAlgebraError::NativeCallFailed {
                component: "cuTensorNet",
                operation,
                status,
                message: self.cutensornet_message(status),
            })
        }
    }
}

/// The object a successful `operation` created, of the kind `O`, unless
/// it returned null.
fn created<O: NativePointer>(
    object: *mut c_void,
    operation: &'static str,
    resource: &'static str,
) -> Result<O, CudaAlgebraError> {
    NonNull::new(object)
        .map(O::from_pointer)
        .ok_or(CudaAlgebraError::MissingNativeResource {
            operation,
            resource,
        })
}

/// A copy of a library's error string, or `null_message` if the library
/// returned none.
fn copy_error_message(pointer: *const std::ffi::c_char, null_message: &str) -> String {
    if pointer.is_null() {
        null_message.to_string()
    } else {
        // SAFETY: callers pass non-null pointers returned by CUDA or
        // cuTensorNet error APIs, both documented as null-terminated strings.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}
