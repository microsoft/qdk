// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CudaAlgebraError`]: why a call of the CUDA algebra failed, the
//! algebra's `SiteAlgebra::Error`, which the engine on it wraps as
//! `CanonicalMpsError::Algebra`.
//!
//! Most of its failures are the device's or the libraries', none of them
//! the caller's fault: no device, a native call that returned an error
//! status, a size or a result that does not fit, host memory the allocator
//! refuses, a workspace larger than the device's free memory, or an
//! algebra an earlier failure left unusable. Beside them are the algebra's
//! two refusals of its caller's tensors: one another algebra made
//! (`ForeignTensor`), and one shorter than the call reads
//! (`InvalidNativeCall`).
//!
//! Three parts of the engine make it, so it sits beside them rather than in
//! any one: the CUDA algebra (`cuda_algebra/`), the device session it runs
//! in (`device_session.rs`), and the native calls they make
//! (`native_calls/`). The setup check's device step calls three of those
//! native calls too, and makes a `NativeCallFailed` itself, from its own
//! read of a device's compute capability (`gpu_setup/devices/`). Each
//! case's doc names the parts that make it.
//!
//! When an operation and the cleanup after it both fail, both errors are
//! kept (`combine_execution_and_cleanup`), so a cleanup never hides the
//! cause. A cleanup attempts every release and returns the first failure
//! (`first_failure`), so a failed release skips no other; its own object
//! is leaked, never retried.

use thiserror::Error;

/// Why a native call, or the work that prepares it or reads its result,
/// failed.
#[derive(Debug, Error)]
pub enum CudaAlgebraError {
    /// No CUDA device is available: made by the device session's open.
    #[error("no CUDA-capable device is available")]
    NoDevice,

    /// A CUDA Runtime or cuTensorNet call returned an error status: made by
    /// the native calls, and by the setup check's compute-capability read,
    /// from the status a loaded library returns (or one the fake libraries
    /// are told to return).
    #[error("{component} {operation} failed with status {status}: {message}")]
    NativeCallFailed {
        component: &'static str,
        operation: &'static str,
        status: u32,
        message: String,
    },

    /// A native call was about to be made with arguments that do not fit
    /// it: a defect of the code making it, or of a caller that broke a
    /// documented precondition. Made by the CUDA algebra, for a tensor
    /// shorter than its call reads, and by the native calls, for an empty
    /// buffer or a tensor whose extents and mode labels differ in number.
    #[error("a call was refused before reaching the CUDA device: {reason}")]
    InvalidNativeCall { reason: String },

    /// A native call succeeded but returned a null handle: made by the
    /// native calls of the loaded libraries.
    #[error("{operation} succeeded without returning a {resource}")]
    MissingNativeResource {
        operation: &'static str,
        resource: &'static str,
    },

    /// A size does not fit the native integer type: made by the CUDA
    /// algebra (a tensor's extents or element count, its scratch's sizes)
    /// and by the native calls (a buffer's bytes, a tensor's mode count or
    /// element count).
    #[error("{resource} size overflows the native address space")]
    ResourceSizeOverflow { resource: &'static str },

    /// The host's allocator refused the memory a native call asked for,
    /// such as a host slot of a workspace: `bytes`, as asked, before the
    /// slot's alignment padding, which the refused allocation included.
    /// Made by the CUDA algebra's scratch, as it grows.
    #[error("the host's allocator refused {bytes} bytes of memory")]
    HostAllocationFailed { bytes: usize },

    /// cuTensorNet asked for more workspace than the device's free memory:
    /// `required` bytes, beyond the `maximum` free when the call grew its
    /// scratch. Made by the CUDA algebra's scratch, as it grows.
    #[error(
        "cuTensorNet's workspace for this call needs {required} bytes, more than the \
         {maximum} bytes free on the CUDA device"
    )]
    WorkspaceLimitExceeded { required: usize, maximum: usize },

    /// A native call returned a value that cannot be used: a gate split's
    /// report out of range, a workspace slot's negative size, or an
    /// allocation that returned the buffer of a tensor the call reads,
    /// which it must leave unchanged. Made by the CUDA algebra, which
    /// checks each such value before it uses it.
    #[error("cuTensorNet or the CUDA Runtime returned an unusable result: {reason}")]
    InvalidNativeResult { reason: String },

    /// A CUDA algebra was given a tensor another algebra made, which it
    /// refuses before any native call: made by the CUDA algebra.
    #[error(
        "a site tensor was made by another CUDA algebra (another GPU engine), whose \
         device work this one cannot wait for"
    )]
    ForeignTensor,

    /// An earlier failure left the algebra unusable; only dropping it, and
    /// its tensors, is left. Made by the CUDA algebra's session, on every
    /// call after the failure.
    #[error("the CUDA algebra is unusable after an earlier failure: {reason}")]
    Poisoned { reason: String },

    /// An operation failed, and so did the cleanup after it; both errors are
    /// kept. Made by `combine_execution_and_cleanup`, wherever a cleanup
    /// follows an operation: in the CUDA algebra, the device session and the
    /// native calls.
    #[error("execution failed ({execution}); cleanup also failed ({cleanup})")]
    ExecutionAndCleanupFailed {
        execution: Box<Self>,
        cleanup: Box<Self>,
    },
}

/// `execution`'s result, unless `cleanup` failed: both failures are kept, so
/// a cleanup never hides the cause.
pub(crate) fn combine_execution_and_cleanup<T>(
    execution: Result<T, CudaAlgebraError>,
    cleanup: Result<(), CudaAlgebraError>,
) -> Result<T, CudaAlgebraError> {
    match (execution, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(execution), Err(cleanup)) => Err(CudaAlgebraError::ExecutionAndCleanupFailed {
            execution: Box::new(execution),
            cleanup: Box::new(cleanup),
        }),
    }
}

/// The first failure of `releases`, which are all attempted: a failed
/// release never skips the next. The object whose release failed is
/// leaked, never retried.
pub(crate) fn first_failure(
    releases: impl IntoIterator<Item = Result<(), CudaAlgebraError>>,
) -> Result<(), CudaAlgebraError> {
    releases.into_iter().fold(Ok(()), Result::and)
}

/// A release that failed while dropping `what`: written to stderr in debug
/// builds, and otherwise not reported: `Drop` cannot return an error, and a
/// broken device makes the algebra's next call fail and report it.
pub(crate) fn report_failed_release(what: &str, released: Result<(), CudaAlgebraError>) {
    #[cfg(debug_assertions)]
    if let Err(error) = released {
        eprintln!("qdk_cutensornet: releasing {what} failed: {error}");
    }
    #[cfg(not(debug_assertions))]
    let _ = (what, released);
}
