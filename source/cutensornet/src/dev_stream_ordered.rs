// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): CUDA's stream-ordered memory
//! allocator (`cudaMallocAsync`, `cudaFreeAsync`), for `CudaMpsEngine`'s
//! device tensors when `with_dev_stream_ordered_allocation` turns it on.
//!
//! It exists to measure whether threads in one process, each with its own
//! engine, can run H₂ shots as fast as processes under CUDA's MPS daemon.
//! In one process every thread shares one device context, and `cudaFree`
//! waits for the whole context, so each free would wait for every other
//! thread's work. `cudaFreeAsync` waits for nothing: the memory returns to
//! a pool once the work before it on its own stream is done. It is safe
//! when every use of a buffer is on the stream it was allocated on, which
//! holds for the engine (its buffers are used on its one stream only).
//!
//! [`open_stream_ordered`] finds the allocator's functions, refusing the
//! option with every missing one named, and sets device 0's default pool
//! up once for all the engines of the process:
//! - its release threshold is the largest, so freed memory stays in the
//!   pool instead of going back to the device at each synchronisation;
//! - its reuse with internal dependencies is off, so the pool never makes
//!   one stream's allocation wait for another stream's work to reuse the
//!   memory it freed: that would bring back, between threads, a wait that
//!   processes do not have.

use crate::LibraryFault;
use crate::libraries::cuda_runtime::cudart_12;
use crate::libraries::loader::{FunctionTable, SymbolResolver, function_table};
use std::ffi::{c_int, c_void};

/// `cudaMallocAsync`.
pub(crate) type MallocAsyncFn = unsafe extern "C" fn(
    *mut *mut c_void,
    usize,
    cudart_12::cudaStream_t,
) -> cudart_12::cudaError_t;
/// `cudaFreeAsync`.
pub(crate) type FreeAsyncFn =
    unsafe extern "C" fn(*mut c_void, cudart_12::cudaStream_t) -> cudart_12::cudaError_t;
type DefaultMemPoolFn =
    unsafe extern "C" fn(*mut cudart_12::cudaMemPool_t, c_int) -> cudart_12::cudaError_t;
type MemPoolSetAttributeFn = unsafe extern "C" fn(
    cudart_12::cudaMemPool_t,
    cudart_12::cudaMemPoolAttr,
    *mut c_void,
) -> cudart_12::cudaError_t;

function_table! {
    /// The CUDA Runtime functions of the stream-ordered allocator.
    #[derive(Clone, Copy)]
    struct StreamOrderedFunctions, declared in cudart_12 {
        required malloc_async: MallocAsyncFn = cudaMallocAsync,
        required free_async: FreeAsyncFn = cudaFreeAsync,
        required default_mem_pool: DefaultMemPoolFn = cudaDeviceGetDefaultMemPool,
        required mem_pool_set_attribute: MemPoolSetAttributeFn = cudaMemPoolSetAttribute,
    }
}

/// The allocator's functions from `resolver`, the CUDA Runtime, with device
/// 0's default pool set up (see the module's documentation).
///
/// # Errors
///
/// When the CUDA Runtime lacks a function, naming every missing one, or
/// when setting the pool up fails, naming the call and its status.
pub(crate) fn open_stream_ordered<R: SymbolResolver>(
    resolver: &R,
) -> Result<StreamOrderedFunctions, String> {
    let functions = StreamOrderedFunctions::resolve(resolver).map_err(|faults| {
        let missing: Vec<String> = faults
            .iter()
            .map(|fault| match fault {
                LibraryFault::MissingRequiredSymbol { symbol } => (*symbol).to_string(),
                other => format!("{other:?}"),
            })
            .collect();
        format!(
            "stream-ordered allocation is refused: the CUDA Runtime lacks {}",
            missing.join(", ")
        )
    })?;
    set_up_default_pool(&functions)?;
    Ok(functions)
}

/// Sets device 0's default pool up, as the module's documentation says.
fn set_up_default_pool(functions: &StreamOrderedFunctions) -> Result<(), String> {
    let mut pool: cudart_12::cudaMemPool_t = std::ptr::null_mut();
    // SAFETY: `pool` is a valid out-pointer; device 0 exists once discovery
    // found a CUDA Runtime with a driver.
    let status = unsafe { (functions.default_mem_pool)(&raw mut pool, 0) };
    check("cudaDeviceGetDefaultMemPool", status)?;
    let mut threshold = u64::MAX;
    // SAFETY: the release threshold is a `cuuint64_t`, which `threshold` is.
    let status = unsafe {
        (functions.mem_pool_set_attribute)(
            pool,
            cudart_12::cudaMemPoolAttr_cudaMemPoolAttrReleaseThreshold,
            (&raw mut threshold).cast(),
        )
    };
    check("cudaMemPoolSetAttribute(release threshold)", status)?;
    let mut internal_dependencies: c_int = 0;
    // SAFETY: this attribute is an `int`, which `internal_dependencies` is.
    let status = unsafe {
        (functions.mem_pool_set_attribute)(
            pool,
            cudart_12::cudaMemPoolAttr_cudaMemPoolReuseAllowInternalDependencies,
            (&raw mut internal_dependencies).cast(),
        )
    };
    check(
        "cudaMemPoolSetAttribute(reuse with internal dependencies)",
        status,
    )
}

fn check(operation: &str, status: cudart_12::cudaError_t) -> Result<(), String> {
    if status == cudart_12::cudaError_cudaSuccess {
        Ok(())
    } else {
        Err(format!(
            "stream-ordered allocation is refused: {operation} returned CUDA status {status}"
        ))
    }
}
