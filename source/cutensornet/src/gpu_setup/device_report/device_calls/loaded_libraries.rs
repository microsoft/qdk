// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `DeviceCalls` on the loaded libraries. The count, the selection and the
//! memory read are the GPU engine's own native calls
//! (`engine::native_calls`), so the setup check and the engine reach each
//! by one path. The engine does not read the compute capability, so it is
//! read here. It compiles only on Linux x86-64, the one platform where the
//! libraries load.

use super::DeviceCalls;
use crate::CudaAlgebraError;
use crate::engine::native_calls::{memory_calls::MemoryCalls, session_calls::SessionCalls};
use crate::libraries::setup_guide::ComputeCapability;
use crate::libraries::{LoadedLibraries, cuda_runtime::cudart_12};

impl DeviceCalls for LoadedLibraries {
    /// The engine's `SessionCalls::device_count` (`cudaGetDeviceCount`).
    fn device_count(&self) -> Result<i32, CudaAlgebraError> {
        SessionCalls::device_count(self)
    }

    /// The engine's `SessionCalls::set_device` (`cudaSetDevice`).
    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError> {
        SessionCalls::set_device(self, ordinal)
    }

    /// Both parts through [`device_attribute`] (`cudaDeviceGetAttribute`),
    /// the major first.
    fn compute_capability(&self, ordinal: i32) -> Result<ComputeCapability, CudaAlgebraError> {
        Ok(ComputeCapability {
            major: device_attribute(
                self,
                cudart_12::cudaDeviceAttr_cudaDevAttrComputeCapabilityMajor,
                ordinal,
            )?,
            minor: device_attribute(
                self,
                cudart_12::cudaDeviceAttr_cudaDevAttrComputeCapabilityMinor,
                ordinal,
            )?,
        })
    }

    /// The engine's `MemoryCalls::memory_info` (`cudaMemGetInfo`).
    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError> {
        MemoryCalls::memory_info(self)
    }
}

/// The value of `attribute` of device `ordinal` (`cudaDeviceGetAttribute`).
/// CUDA reads each part of a compute capability as an attribute of its
/// own. Fails when CUDA returns an error status, which `check_cuda` turns
/// into the error, as for the engine's own calls.
fn device_attribute(
    libraries: &LoadedLibraries,
    attribute: cudart_12::cudaDeviceAttr,
    ordinal: i32,
) -> Result<i32, CudaAlgebraError> {
    let mut value = 0;
    // SAFETY: `value` is a valid writable `int` out-pointer, and the
    // attribute is one of the header's. The ordinal is taken by value:
    // cudaDeviceGetAttribute's documentation lists cudaErrorInvalidDevice
    // among its returns, so an ordinal no device has is an error status.
    let status = unsafe {
        (libraries.cuda_runtime.functions.cudaDeviceGetAttribute)(
            &raw mut value,
            attribute,
            ordinal,
        )
    };
    libraries.check_cuda("cudaDeviceGetAttribute", status)?;
    Ok(value)
}
