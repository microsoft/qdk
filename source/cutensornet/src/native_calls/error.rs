// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NativeError`]: why a native call, or the work around it, failed.
//!
//! Every user of the native calls, today the engine and the CUDA algebra,
//! must report these failures, and none of them is its caller's fault: no
//! device, a call that returned an error status, a function the library
//! lacks, a size or a result that does not fit, a workspace larger than the
//! budget, or a native object an earlier failure left unusable. So they are
//! one type, which each user wraps in its own error beside the refusals of
//! its contract (`EngineError::Native`), or returns as it is (the CUDA
//! algebra's `SiteAlgebra::Error`).
//!
//! When an operation and the cleanup after it both fail, both errors are
//! kept (`combine_execution_and_cleanup`), so a cleanup never hides the
//! cause. A cleanup attempts every release and returns the first failure
//! (`first_failure`), so a failed release leaks nothing else.

use crate::LibraryFault;
use thiserror::Error;

/// Why a native call, or the work that prepares it or reads its result,
/// failed.
#[derive(Debug, Error)]
pub enum NativeError {
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
    /// it: a defect of the code making it, not of its caller.
    #[error("invalid native call: {reason}")]
    InvalidNativeCall { reason: String },

    /// The loaded library lacks functions a call needs, which discovery did
    /// not require: today cuTensorNet's tensor API, which the engine never
    /// calls and the CUDA algebra needs.
    #[error("{library} does not export {}", .symbols.join(", "))]
    MissingNativeFunctions {
        library: &'static str,
        symbols: Vec<&'static str>,
    },

    /// A native call succeeded but returned a null handle.
    #[error("{operation} succeeded without returning a {resource}")]
    MissingNativeResource {
        operation: &'static str,
        resource: &'static str,
    },

    /// A size does not fit the native integer type.
    #[error("{resource} size overflows the native address space")]
    ResourceSizeOverflow { resource: &'static str },

    /// cuTensorNet asked for more workspace than the budget or the device's
    /// free memory.
    #[error("native workspace requires {required} bytes, exceeding the {maximum}-byte limit")]
    WorkspaceLimitExceeded { required: usize, maximum: usize },

    /// cuTensorNet returned a value that cannot be used, such as a
    /// non-finite or complex norm.
    #[error("invalid native result: {reason}")]
    InvalidNativeResult { reason: String },

    /// An earlier failure left the state unusable; only dropping it is left.
    #[error("the engine's state is unusable after an earlier failure: {reason}")]
    Poisoned { reason: String },

    /// An operation failed, and so did the cleanup after it; both errors are
    /// kept.
    #[error("execution failed ({execution}); cleanup also failed ({cleanup})")]
    ExecutionAndCleanupFailed {
        execution: Box<Self>,
        cleanup: Box<Self>,
    },
}

impl NativeError {
    /// The failure of a call that needs functions `library` lacks, whose
    /// lookup found `faults`: each missing function is named.
    pub(crate) fn missing_functions(library: &'static str, faults: &[LibraryFault]) -> Self {
        Self::MissingNativeFunctions {
            library,
            symbols: faults
                .iter()
                .filter_map(|fault| match fault {
                    LibraryFault::MissingRequiredSymbol { symbol } => Some(*symbol),
                    _ => None,
                })
                .collect(),
        }
    }
}

/// `execution`'s result, unless `cleanup` failed: both failures are kept, so
/// a cleanup never hides the cause.
pub(crate) fn combine_execution_and_cleanup<T>(
    execution: Result<T, NativeError>,
    cleanup: Result<(), NativeError>,
) -> Result<T, NativeError> {
    match (execution, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(execution), Err(cleanup)) => Err(NativeError::ExecutionAndCleanupFailed {
            execution: Box::new(execution),
            cleanup: Box::new(cleanup),
        }),
    }
}

/// The first failure of `releases`, which are all attempted: a failed
/// release never skips the next, so it leaks nothing but its own object.
pub(crate) fn first_failure(
    releases: impl IntoIterator<Item = Result<(), NativeError>>,
) -> Result<(), NativeError> {
    releases.into_iter().fold(Ok(()), Result::and)
}

/// A release that failed while dropping `what`: written to stderr in debug
/// builds, and otherwise not reported. Each caller says why that is enough
/// (the engine, on `CuTensorNetState`).
pub(crate) fn report_failed_release(what: &str, released: Result<(), NativeError>) {
    #[cfg(debug_assertions)]
    if let Err(error) = released {
        eprintln!("qdk_cutensornet: releasing {what} failed: {error}");
    }
    #[cfg(not(debug_assertions))]
    let _ = (what, released);
}
