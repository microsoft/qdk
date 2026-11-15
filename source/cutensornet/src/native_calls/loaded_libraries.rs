// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's native calls on the loaded libraries: `SessionCalls`,
//! `MemoryCalls` and `StateCalls`, implemented for `LoadedLibraries`
//! (`crate::libraries`) through the function tables discovery looked up,
//! one file per trait (`loaded_libraries/session_calls.rs`,
//! `loaded_libraries/memory_calls.rs`, `loaded_libraries/state_calls.rs`).
//! This file holds what they share: turning a status into an `EngineError`
//! with the library's message.
//!
//! This module is the only code that calls cuTensorNet and the CUDA Runtime,
//! so it is where every foreign-function obligation is kept: each `unsafe`
//! call says what makes it sound (the handles live, the buffers large enough
//! and kept alive, the values in the audited representation). It owns nothing:
//! `DeviceSession` and `NativeState` create the streams, handles, states
//! and buffers, and release them. Discovery has checked the versions, so
//! this code assumes cuTensorNet 2.13 and the CUDA Runtime 12.9. It compiles
//! only on Linux x86-64, and no host test reaches it; a new native call
//! needs its function's row in its library's table first
//! (`libraries::cuda_runtime::CudaFunctions`,
//! `libraries::cutensornet::CuTensorNetFunctions`).

use crate::engine::EngineError;
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12, cutensornet::v2_13};
use std::ffi::CStr;

mod memory_calls;
mod session_calls;
mod state_calls;

impl LoadedLibraries {
    fn cuda_message(&self, status: cudart_12::cudaError_t) -> String {
        // SAFETY: the pointer was resolved with the audited CUDA signature and
        // the returned library-owned string is copied before this call returns.
        let message = unsafe { (self.cuda_runtime.functions.get_error_string)(status) };
        copy_error_message(message, "CUDA returned a null error string")
    }

    fn cutensornet_message(&self, status: v2_13::cutensornetStatus_t) -> String {
        // SAFETY: both pointers were resolved with their audited signatures;
        // returned library-owned strings are copied immediately.
        let stable = unsafe { (self.cutensornet.functions.get_error_string)(status) };
        let mut message = copy_error_message(stable, "cuTensorNet returned a null error string");
        if let Some(get_last_error) = self.cutensornet.functions.get_last_error {
            // SAFETY: the optional pointer has its exact no-argument signature.
            let detail = copy_error_message(unsafe { get_last_error() }, "");
            if !detail.is_empty() {
                message.push_str(": ");
                message.push_str(&detail);
            }
        }
        message
    }

    fn check_cuda(
        &self,
        operation: &'static str,
        status: cudart_12::cudaError_t,
    ) -> Result<(), EngineError> {
        if status == cudart_12::cudaError_cudaSuccess {
            Ok(())
        } else {
            Err(EngineError::NativeCallFailed {
                component: "CUDA Runtime",
                operation,
                status,
                message: self.cuda_message(status),
            })
        }
    }

    fn check_cutensornet(
        &self,
        operation: &'static str,
        status: v2_13::cutensornetStatus_t,
    ) -> Result<(), EngineError> {
        if status == v2_13::cutensornetStatus_t_CUTENSORNET_STATUS_SUCCESS {
            Ok(())
        } else {
            Err(EngineError::NativeCallFailed {
                component: "cuTensorNet",
                operation,
                status,
                message: self.cutensornet_message(status),
            })
        }
    }
}

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
