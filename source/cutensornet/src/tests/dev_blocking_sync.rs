// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Setting blocking synchronisation, on stand-in functions.

use super::stand_ins::StandInResolver;
use crate::dev_blocking_sync::set_blocking_sync;
use crate::libraries::cuda_runtime::cudart_12;
use std::cell::Cell;
use std::ffi::c_uint;

thread_local! {
    /// The flags the stand-in `cudaSetDeviceFlags` was last given.
    static FLAGS_SET: Cell<Option<c_uint>> = const { Cell::new(None) };
}

/// Records the flags, as a device that accepts them.
unsafe extern "C" fn record_flags(flags: c_uint) -> cudart_12::cudaError_t {
    FLAGS_SET.set(Some(flags));
    cudart_12::cudaError_cudaSuccess
}

/// Refuses the flags, as CUDA 11 did on a device already in use.
unsafe extern "C" fn refuse_flags(_flags: c_uint) -> cudart_12::cudaError_t {
    cudart_12::cudaError_cudaErrorSetOnActiveProcess
}

/// Reads back the recorded flags with `cudaDeviceMapHost`, which the CUDA
/// Runtime adds itself.
unsafe extern "C" fn recorded_flags(flags: *mut c_uint) -> cudart_12::cudaError_t {
    let set = FLAGS_SET
        .get()
        .expect("the flags are set before they are read");
    // SAFETY: the caller passes a writable `unsigned int`.
    unsafe { flags.write(set | cudart_12::cudaDeviceMapHost) };
    cudart_12::cudaError_cudaSuccess
}

/// Reads back spinning, as a device that ignored the flags.
unsafe extern "C" fn spinning_flags(flags: *mut c_uint) -> cudart_12::cudaError_t {
    // SAFETY: the caller passes a writable `unsigned int`.
    unsafe { flags.write(cudart_12::cudaDeviceScheduleSpin | cudart_12::cudaDeviceMapHost) };
    cudart_12::cudaError_cudaSuccess
}

/// Reads back blocking, whatever was set: the refusal alone must decide.
unsafe extern "C" fn blocking_flags(flags: *mut c_uint) -> cudart_12::cudaError_t {
    // SAFETY: the caller passes a writable `unsigned int`.
    unsafe {
        flags.write(cudart_12::cudaDeviceScheduleBlockingSync | cudart_12::cudaDeviceMapHost);
    };
    cudart_12::cudaError_cudaSuccess
}

fn runtime(set: *const (), get: *const ()) -> StandInResolver {
    StandInResolver::audited()
        .with("cudaSetDeviceFlags", set)
        .with("cudaGetDeviceFlags", get)
}

/// The process's waits block: CUDA is given the blocking schedule, and the
/// flags it reads back are returned for the run's log.
#[test]
fn blocking_sync_asks_cuda_for_the_blocking_schedule() {
    let flags = set_blocking_sync(&runtime(
        record_flags as *const (),
        recorded_flags as *const (),
    ))
    .expect("a device that accepts the flags");

    assert_eq!(
        FLAGS_SET.get(),
        Some(cudart_12::cudaDeviceScheduleBlockingSync)
    );
    assert_eq!(
        flags,
        cudart_12::cudaDeviceScheduleBlockingSync | cudart_12::cudaDeviceMapHost
    );
}

/// CUDA refusing the flags ends the setting with CUDA's status, so a run
/// does not go on spinning unannounced.
#[test]
fn a_refused_flag_names_cudas_status() {
    let Err(message) = set_blocking_sync(&runtime(
        refuse_flags as *const (),
        blocking_flags as *const (),
    )) else {
        panic!("a refused flag is an error");
    };

    assert!(
        message.contains(&cudart_12::cudaError_cudaErrorSetOnActiveProcess.to_string()),
        "{message}"
    );
}

/// A device whose flags read back without the blocking schedule did not
/// take it, so the setting is refused rather than trusted.
#[test]
fn flags_read_back_without_blocking_are_refused() {
    let Err(message) = set_blocking_sync(&runtime(
        record_flags as *const (),
        spinning_flags as *const (),
    )) else {
        panic!("flags read back as spinning are an error");
    };

    assert!(message.contains("0x9"), "{message}");
}

/// A CUDA Runtime without the flags' functions is refused, naming them.
#[test]
fn blocking_sync_is_refused_naming_a_missing_function() {
    let resolver = runtime(record_flags as *const (), recorded_flags as *const ())
        .without("cudaGetDeviceFlags");

    let Err(message) = set_blocking_sync(&resolver) else {
        panic!("a CUDA Runtime without cudaGetDeviceFlags is refused");
    };

    assert!(message.contains("cudaGetDeviceFlags"), "{message}");
}
