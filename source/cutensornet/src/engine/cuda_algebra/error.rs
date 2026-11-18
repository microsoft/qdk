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
//! (`InvalidNativeCall`). The native calls
//! (`engine/native_calls`) return it too, since the algebra and the device
//! session are their only users.
//!
//! When an operation and the cleanup after it both fail, both errors are
//! kept (`combine_execution_and_cleanup`), so a cleanup never hides the
//! cause. A cleanup attempts every release and returns the first failure
//! (`first_failure`), so a failed release leaks nothing else.

use thiserror::Error;

/// Why a native call, or the work that prepares it or reads its result,
/// failed.
#[derive(Debug, Error)]
pub enum CudaAlgebraError {
    /// No CUDA device is available.
    #[error("no CUDA-capable device is available")]
    NoDevice,

    /// A CUDA Runtime or cuTensorNet call returned an error status.
    #[error("{component} {operation} failed with status {status}: {message}")]
    NativeCallFailed {
        component: &'static str,
        operation: &'static str,
        status: u32,
        message: String,
    },

    /// A native call was about to be made with arguments that do not fit
    /// it: a defect of the code making it, or of a caller that broke a
    /// documented precondition, such as a tensor shorter than a CUDA
    /// algebra's call reads.
    #[error("invalid native call: {reason}")]
    InvalidNativeCall { reason: String },

    /// A native call succeeded but returned a null handle.
    #[error("{operation} succeeded without returning a {resource}")]
    MissingNativeResource {
        operation: &'static str,
        resource: &'static str,
    },

    /// A size does not fit the native integer type.
    #[error("{resource} size overflows the native address space")]
    ResourceSizeOverflow { resource: &'static str },

    /// The host's allocator refused the memory a native call asked for,
    /// such as a host slot of a workspace: `bytes`, as asked, before the
    /// slot's alignment padding, which the refused allocation included.
    #[error("the host's allocator refused {bytes} bytes of memory")]
    HostAllocationFailed { bytes: usize },

    /// cuTensorNet asked for more workspace than the device's free memory:
    /// `required` bytes, beyond the `maximum` free when the call grew its
    /// scratch.
    #[error("native workspace requires {required} bytes, more than the {maximum} bytes free")]
    WorkspaceLimitExceeded { required: usize, maximum: usize },

    /// A native call returned a value that cannot be used: a gate split's
    /// report out of range, or an allocation that returned the buffer of
    /// a tensor the call reads, which it must leave unchanged.
    #[error("invalid native result: {reason}")]
    InvalidNativeResult { reason: String },

    /// A CUDA algebra was given a tensor another algebra made, which it
    /// refuses before any native call.
    #[error("the tensor belongs to another CUDA algebra")]
    ForeignTensor,

    /// An earlier failure left the algebra unusable; only dropping it, and
    /// its tensors, is left.
    #[error("the CUDA algebra is unusable after an earlier failure: {reason}")]
    Poisoned { reason: String },

    /// An operation failed, and so did the cleanup after it; both errors are
    /// kept.
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
/// release never skips the next, so it leaks nothing but its own object.
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
