// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `DeviceCalls` on the loaded libraries: each read is the GPU engine's own
//! native call (`engine::native_calls`), so the setup check and the engine
//! reach each CUDA function by one path. It compiles only on Linux x86-64,
//! and no host test reaches it: the rules run on a fake of these calls
//! (`FakeDevices`, in the devices' tests).

use super::DeviceCalls;
use crate::CudaAlgebraError;
use crate::engine::native_calls::{memory_calls::MemoryCalls, session_calls::SessionCalls};
use crate::libraries::LoadedLibraries;

impl DeviceCalls for LoadedLibraries {
    fn device_count(&self) -> Result<i32, CudaAlgebraError> {
        SessionCalls::device_count(self)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), CudaAlgebraError> {
        SessionCalls::set_device(self, ordinal)
    }

    fn compute_capability(&self, ordinal: i32) -> Result<(i32, i32), CudaAlgebraError> {
        SessionCalls::compute_capability(self, ordinal)
    }

    fn memory_info(&self) -> Result<(usize, usize), CudaAlgebraError> {
        MemoryCalls::memory_info(self)
    }
}
