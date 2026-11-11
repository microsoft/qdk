// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The device reads on the loaded libraries: `DeviceCalls`, implemented for
//! `LoadedLibraries` through the CUDA Runtime's table.
//!
//! Counting the devices, selecting one and reading its memory are the CUDA
//! Runtime's own calls (`libraries::cuda_runtime`), which the MPS engine
//! makes too; the compute capability, which only the setup check reads, is
//! read here. It compiles only on Linux x86-64, and no host test reaches
//! it: the rules run on a stand-in host (`tests/devices.rs`).

use super::DeviceCalls;
use crate::EngineError;
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12};

impl DeviceCalls for LoadedLibraries {
    fn device_count(&self) -> Result<i32, EngineError> {
        Ok(self.cuda_runtime.device_count()?)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), EngineError> {
        Ok(self.cuda_runtime.set_device(ordinal)?)
    }

    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), EngineError> {
        let cuda_runtime = &self.cuda_runtime;
        let attribute = |attribute| -> Result<i32, EngineError> {
            let mut value = 0;
            // SAFETY: `value` is a valid writable `int` out-pointer and the
            // attribute is one of the header's; CUDA checks the ordinal,
            // returning an error for an invalid one.
            let status = unsafe {
                (cuda_runtime.functions.device_get_attribute)(&raw mut value, attribute, ordinal)
            };
            cuda_runtime.check("cudaDeviceGetAttribute", status)?;
            Ok(value)
        };
        Ok((
            attribute(cudart_12::cudaDeviceAttr_cudaDevAttrComputeCapabilityMajor)?,
            attribute(cudart_12::cudaDeviceAttr_cudaDevAttrComputeCapabilityMinor)?,
        ))
    }

    fn memory_info(&self) -> Result<(usize, usize), EngineError> {
        Ok(self.cuda_runtime.memory_info()?)
    }
}
