// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): blocking synchronisation, so that a
//! host thread waiting for the GPU sleeps instead of spinning on a core.
//!
//! It exists to measure threads in one process, each with its own
//! `CudaMpsEngine`, against processes under CUDA's MPS daemon. By default
//! CUDA spins while it waits for a stream when the process has no more
//! contexts than cores, so 22 threads waiting on their streams keep 22
//! cores busy. [`use_blocking_sync`] sets `cudaDeviceScheduleBlockingSync`
//! for the process, before any engine is made, and reads the flags back,
//! so a run cannot measure spinning by mistake.
//!
//! The setting is the process's, not an engine's: CUDA keeps it on the
//! device's primary context, which every thread of the process shares.

use crate::libraries::cuda_runtime::cudart_12;
use crate::libraries::loader::{FunctionTable, SymbolResolver, function_table};
use std::ffi::c_uint;

type SetDeviceFlagsFn = unsafe extern "C" fn(c_uint) -> cudart_12::cudaError_t;
type GetDeviceFlagsFn = unsafe extern "C" fn(*mut c_uint) -> cudart_12::cudaError_t;

function_table! {
    /// The CUDA Runtime functions that set and read the device's flags.
    struct DeviceFlagsFunctions, declared in cudart_12 {
        required set_device_flags: SetDeviceFlagsFn = cudaSetDeviceFlags,
        required get_device_flags: GetDeviceFlagsFn = cudaGetDeviceFlags,
    }
}

/// Makes every later wait of the process for the GPU block its thread,
/// on the libraries `availability` loaded; returns the device's flags as
/// CUDA reads them back. Call it before any engine is made.
///
/// # Errors
///
/// When the CUDA Runtime lacks a function, when CUDA refuses the flag
/// (naming its status), or when the flags read back do not schedule
/// blocking.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn use_blocking_sync(availability: &crate::Availability) -> Result<u32, String> {
    set_blocking_sync(&availability.libraries().cuda_runtime.library)
}

/// [`use_blocking_sync`] on the CUDA Runtime `resolver` finds.
///
/// It calls `cudaSetDeviceFlags` without `cudaSetDevice` first: with no
/// current device, CUDA then initialises device 0 with the flags.
pub(crate) fn set_blocking_sync<R: SymbolResolver>(resolver: &R) -> Result<u32, String> {
    let functions = DeviceFlagsFunctions::resolve(resolver).map_err(|missing| {
        format!("blocking synchronisation is refused: the CUDA Runtime lacks {missing:?}")
    })?;
    // SAFETY: the flag is one `cudaSetDeviceFlags` documents.
    let status = unsafe { (functions.set_device_flags)(cudart_12::cudaDeviceScheduleBlockingSync) };
    if status != cudart_12::cudaError_cudaSuccess {
        return Err(format!(
            "blocking synchronisation is refused: cudaSetDeviceFlags returned CUDA status {status}"
        ));
    }
    let mut flags: c_uint = 0;
    // SAFETY: `flags` is a valid out-pointer.
    let status = unsafe { (functions.get_device_flags)(&raw mut flags) };
    if status != cudart_12::cudaError_cudaSuccess {
        return Err(format!(
            "blocking synchronisation is unconfirmed: cudaGetDeviceFlags returned CUDA status \
             {status}"
        ));
    }
    if flags & cudart_12::cudaDeviceScheduleMask != cudart_12::cudaDeviceScheduleBlockingSync {
        return Err(format!(
            "blocking synchronisation did not take: the device's flags read back as {flags:#x}"
        ));
    }
    Ok(flags)
}
