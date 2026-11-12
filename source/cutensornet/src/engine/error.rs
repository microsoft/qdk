// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`EngineError`]: what a caller of the cuTensorNet engine learns when a
//! call does not succeed, and what it can do about it.
//!
//! - **Refused** ([`EngineError::Refused`], a `tensornet::Refusal`): the
//!   call broke the `MpsEngine` contract, as every engine refuses it. It is
//!   the caller's bug; the state is unchanged and may be used again.
//! - **Failed in the engine** (every other variant): no device, a native
//!   call that returned an error, a result the engine cannot use, a limit
//!   of memory, or a state of another engine. The state is then unspecified,
//!   as the contract says; a native failure while the state changed leaves
//!   it [`EngineError::Poisoned`], so the caller drops it (an MPS simulation
//!   method fails the shot).
//!
//! `MpsEngineError::refusal` tells the two apart without matching variants.
//! When an operation and the cleanup after it both fail, both errors are
//! kept (`combine_execution_and_cleanup`), so a cleanup never hides the
//! cause. A cleanup attempts every release and returns the first failure
//! (`first_failure`), so a failed release leaks nothing else.

use tensornet::{MpsEngineError, Refusal, sampling::TooManyValues};
use thiserror::Error;

/// Why the cuTensorNet engine refused or failed an operation.
///
/// A refused call leaves the state unchanged. After a failure the state is
/// unspecified, as the contract says: a native call that failed while the
/// state was changed or computed makes every later call on it return
/// [`EngineError::Poisoned`], and only dropping it is left.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The call's arguments break the `MpsEngine` contract, refused as
    /// every engine refuses them.
    #[error(transparent)]
    Refused(#[from] Refusal),

    /// The state was used with an engine that did not create it.
    #[error("the state belongs to another engine")]
    ForeignState,

    /// The engine was about to make a native call whose arguments do not
    /// fit it: a defect of the engine, not of its caller.
    #[error("invalid native call: {reason}")]
    InvalidNativeCall { reason: String },

    /// A sample's values do not fit in memory.
    #[error(transparent)]
    TooManyValues(#[from] TooManyValues),

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

    /// A native call succeeded but returned a null handle.
    #[error("{operation} succeeded without returning a {resource}")]
    MissingNativeResource {
        operation: &'static str,
        resource: &'static str,
    },

    /// A size does not fit the native integer type.
    #[error("{resource} size overflows the native address space")]
    ResourceSizeOverflow { resource: &'static str },

    /// cuTensorNet asked for more workspace than the policy's budget or the
    /// device's free memory.
    #[error("native workspace requires {required} bytes, exceeding the {maximum}-byte limit")]
    WorkspaceLimitExceeded { required: usize, maximum: usize },

    /// cuTensorNet returned a value the engine cannot use, such as a
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

pub(super) fn combine_execution_and_cleanup<T>(
    execution: Result<T, EngineError>,
    cleanup: Result<(), EngineError>,
) -> Result<T, EngineError> {
    match (execution, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(execution), Err(cleanup)) => Err(EngineError::ExecutionAndCleanupFailed {
            execution: Box::new(execution),
            cleanup: Box::new(cleanup),
        }),
    }
}

/// The first failure of `releases`, which are all attempted: a failed
/// release never skips the next, so it leaks nothing but its own object.
pub(super) fn first_failure(
    releases: impl IntoIterator<Item = Result<(), EngineError>>,
) -> Result<(), EngineError> {
    releases.into_iter().fold(Ok(()), Result::and)
}

/// A release that failed while dropping `what`: written to stderr in debug
/// builds, and otherwise not reported. Why that is enough is on
/// `CuTensorNetState`.
pub(super) fn report_failed_release(what: &str, released: Result<(), EngineError>) {
    #[cfg(debug_assertions)]
    if let Err(error) = released {
        eprintln!("qdk_cutensornet: releasing {what} failed: {error}");
    }
    #[cfg(not(debug_assertions))]
    let _ = (what, released);
}

impl MpsEngineError for EngineError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            _ => None,
        }
    }
}
