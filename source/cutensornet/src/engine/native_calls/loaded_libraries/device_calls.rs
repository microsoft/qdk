// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The setup check's `DeviceCalls` on the loaded libraries: the device count
//! and selection and the memory, shared with the engine's calls, and each
//! device's compute capability.

use crate::devices::DeviceCalls;
use crate::engine::{
    EngineError,
    native_calls::{memory_calls::MemoryCalls, session_calls::SessionCalls},
};
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12};

impl DeviceCalls for LoadedLibraries {
    fn device_count(&self) -> Result<i32, EngineError> {
        SessionCalls::device_count(self)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), EngineError> {
        SessionCalls::set_device(self, ordinal)
    }

    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), EngineError> {
        let attribute = |attribute| -> Result<i32, EngineError> {
            let mut value = 0;
            // SAFETY: `value` is a valid writable `int` out-pointer and the
            // attribute is one of the header's; CUDA checks the ordinal,
            // returning an error for an invalid one.
            let status = unsafe {
                (self.cuda_runtime.functions.device_get_attribute)(
                    &raw mut value,
                    attribute,
                    ordinal,
                )
            };
            self.check_cuda("cudaDeviceGetAttribute", status)?;
            Ok(value)
        };
        Ok((
            attribute(cudart_12::cudaDeviceAttr_cudaDevAttrComputeCapabilityMajor)?,
            attribute(cudart_12::cudaDeviceAttr_cudaDevAttrComputeCapabilityMinor)?,
        ))
    }

    fn memory_info(&self) -> Result<(usize, usize), EngineError> {
        MemoryCalls::memory_info(self)
    }
}
