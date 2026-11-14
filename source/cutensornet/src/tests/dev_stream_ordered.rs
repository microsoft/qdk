// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The stream-ordered allocator's refusal and its pool's setup, on stand-in
//! functions.

use super::stand_ins::StandInResolver;
use crate::dev_stream_ordered::open_stream_ordered;
use crate::libraries::cuda_runtime::cudart_12;
use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_void};
use std::ptr::NonNull;

thread_local! {
    /// The pool's attributes the stand-in `cudaMemPoolSetAttribute` was
    /// given, in order, each value widened to `u64`.
    static ATTRIBUTES: RefCell<Vec<(cudart_12::cudaMemPoolAttr, u64)>> =
        const { RefCell::new(Vec::new()) };
    /// The attribute the refusing stand-in refuses.
    static REFUSED: Cell<cudart_12::cudaMemPoolAttr> = const { Cell::new(0) };
}

/// Writes a pool handle, as `cudaDeviceGetDefaultMemPool` does.
unsafe extern "C" fn default_mem_pool(
    pool: *mut cudart_12::cudaMemPool_t,
    _device: c_int,
) -> cudart_12::cudaError_t {
    // SAFETY: the caller passes a writable pool handle.
    unsafe { pool.write(NonNull::dangling().as_ptr()) };
    cudart_12::cudaError_cudaSuccess
}

/// Records the attribute and its value, read at the width CUDA gives it.
unsafe extern "C" fn record_attribute(
    _pool: cudart_12::cudaMemPool_t,
    attribute: cudart_12::cudaMemPoolAttr,
    value: *mut c_void,
) -> cudart_12::cudaError_t {
    let value = if attribute == cudart_12::cudaMemPoolAttr_cudaMemPoolAttrReleaseThreshold {
        // SAFETY: CUDA's contract: this attribute's value is a `cuuint64_t`.
        unsafe { value.cast::<u64>().read() }
    } else {
        // SAFETY: CUDA's contract: the reuse attributes' values are `int`s.
        u64::try_from(unsafe { value.cast::<c_int>().read() }).expect("a flag is 0 or 1")
    };
    ATTRIBUTES.with_borrow_mut(|attributes| attributes.push((attribute, value)));
    cudart_12::cudaError_cudaSuccess
}

/// Refuses the attribute `REFUSED` names, and accepts the others.
unsafe extern "C" fn refuse_attribute(
    _pool: cudart_12::cudaMemPool_t,
    attribute: cudart_12::cudaMemPoolAttr,
    _value: *mut c_void,
) -> cudart_12::cudaError_t {
    if attribute == REFUSED.get() {
        cudart_12::cudaError_cudaErrorNotSupported
    } else {
        cudart_12::cudaError_cudaSuccess
    }
}

/// A CUDA Runtime whose pool functions record the setup.
fn recording_runtime() -> StandInResolver {
    StandInResolver::audited()
        .with("cudaDeviceGetDefaultMemPool", default_mem_pool as *const ())
        .with("cudaMemPoolSetAttribute", record_attribute as *const ())
}

/// The value the setup gave `attribute`, if it set it.
fn attribute_set(attribute: cudart_12::cudaMemPoolAttr) -> Option<u64> {
    ATTRIBUTES.with_borrow(|attributes| {
        attributes
            .iter()
            .rev()
            .find(|&&(set, _)| set == attribute)
            .map(|&(_, value)| value)
    })
}

/// A CUDA Runtime older than 11.2 has no stream-ordered allocator: the
/// option is refused, naming each function it lacks so that one message
/// says what is missing.
#[test]
fn stream_ordered_allocation_is_refused_naming_every_missing_function() {
    let runtime = recording_runtime()
        .without("cudaFreeAsync")
        .without("cudaMemPoolSetAttribute");

    let Err(message) = open_stream_ordered(&runtime) else {
        panic!("a CUDA Runtime without cudaFreeAsync is refused");
    };

    assert!(message.contains("cudaFreeAsync"), "{message}");
    assert!(message.contains("cudaMemPoolSetAttribute"), "{message}");
    assert!(!message.contains("cudaMallocAsync"), "{message}");
}

/// Freed memory stays in device 0's pool for the next allocation, rather
/// than going back to the device at each synchronisation of the stream.
#[test]
fn the_default_pool_keeps_freed_memory() {
    open_stream_ordered(&recording_runtime()).expect("the stand-in runtime has the allocator");

    assert_eq!(
        attribute_set(cudart_12::cudaMemPoolAttr_cudaMemPoolAttrReleaseThreshold),
        Some(u64::MAX)
    );
}

/// Threads' engines each have their own stream: the pool never makes one
/// stream's allocation wait for another's work to reuse its memory.
#[test]
fn the_default_pool_never_makes_a_stream_wait_for_another() {
    open_stream_ordered(&recording_runtime()).expect("the stand-in runtime has the allocator");

    assert_eq!(
        attribute_set(cudart_12::cudaMemPoolAttr_cudaMemPoolReuseAllowInternalDependencies),
        Some(0)
    );
}

/// A pool CUDA cannot set up (a device without memory pools) refuses the
/// option, whichever of the pool's settings fails, naming the call and
/// CUDA's status.
#[test]
fn a_failed_pool_setup_names_the_call_and_its_status() {
    let runtime =
        recording_runtime().with("cudaMemPoolSetAttribute", refuse_attribute as *const ());

    for attribute in [
        cudart_12::cudaMemPoolAttr_cudaMemPoolAttrReleaseThreshold,
        cudart_12::cudaMemPoolAttr_cudaMemPoolReuseAllowInternalDependencies,
    ] {
        REFUSED.set(attribute);

        let Err(message) = open_stream_ordered(&runtime) else {
            panic!("a pool that refuses attribute {attribute} is refused");
        };

        assert!(message.contains("cudaMemPoolSetAttribute"), "{message}");
        assert!(
            message.contains(&cudart_12::cudaError_cudaErrorNotSupported.to_string()),
            "{message}"
        );
    }
}
