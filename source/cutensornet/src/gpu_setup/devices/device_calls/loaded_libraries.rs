// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `DeviceCalls` on the loaded libraries. The count, the selection and the
//! memory read are the GPU engine's own native calls
//! (`engine::native_calls`), so the setup check and the engine reach each
//! by one path; the compute capability, which the engine does not read, is
//! read here. It compiles only on Linux x86-64, the one platform where the
//! libraries load.

use super::DeviceCalls;
use crate::CudaAlgebraError;
use crate::engine::native_calls::{memory_calls::MemoryCalls, session_calls::SessionCalls};
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12};

impl DeviceCalls for LoadedLibraries {
    fn device_count(&self) -> Result<i32, CudaAlgebraError> {
        SessionCalls::device_count(self)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError> {
        SessionCalls::set_device(self, ordinal)
    }

    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), CudaAlgebraError> {
        let attribute = |attribute| -> Result<i32, CudaAlgebraError> {
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

    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError> {
        MemoryCalls::memory_info(self)
    }
}
