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
//! This module is the engine's only code that calls cuTensorNet and the
//! CUDA Runtime, so it is where the engine's foreign-function obligations
//! are kept: each `unsafe` call says what makes it sound (the handles live,
//! the buffers large enough and kept alive, the values in the audited
//! representation). The CUDA Runtime calls the setup check's device reads
//! make too (count the devices, select one, read its memory), and the check
//! of a CUDA status, are the CUDA Runtime's own, in its module
//! (`libraries::cuda_runtime`), which both call. This module owns nothing:
//! `DeviceSession` and `NativeState` create the streams, handles, states and
//! buffers, and release them. Discovery has checked the versions, so this
//! code assumes cuTensorNet 2.13 and the CUDA Runtime 12.9. It compiles only
//! on Linux x86-64, and no host test reaches it; a new native call needs its
//! function's row in its library's table first
//! (`libraries::cuda_runtime::CudaFunctions`,
//! `libraries::cutensornet::CuTensorNetFunctions`).

use crate::engine::EngineError;
use crate::libraries::{
    LoadedLibraries, copy_message, cuda_runtime::cudart_12, cutensornet::v2_13,
};

mod memory_calls;
mod session_calls;
mod state_calls;

impl LoadedLibraries {
    fn cutensornet_message(&self, status: v2_13::cutensornetStatus_t) -> String {
        // SAFETY: the pointer was resolved with its audited signature, and
        // cuTensorNet returns null or a null-terminated string it owns,
        // copied before the next call.
        let mut message = unsafe {
            copy_message(
                (self.cutensornet.functions.get_error_string)(status),
                "cuTensorNet returned a null error string",
            )
        };
        if let Some(get_last_error) = self.cutensornet.functions.get_last_error {
            // SAFETY: the optional pointer has its exact no-argument
            // signature, and its string is copied as the one above.
            let detail = unsafe { copy_message(get_last_error(), "") };
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
        Ok(self.cuda_runtime.check(operation, status)?)
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
