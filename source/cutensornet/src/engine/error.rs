//! Errors of the cuTensorNet engine.
//!
//! [`EngineError`] covers invalid requests, native call failures and invalid
//! native results. `combine_execution_and_cleanup` keeps both errors when an
//! execution and its cleanup fail, so a cleanup failure never hides the
//! execution's.

use thiserror::Error;

/// Why the cuTensorNet engine refused or failed an operation.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid request: {reason}")]
    InvalidRequest { reason: String },

    #[error("invalid native execution policy: {reason}")]
    InvalidExecutionPolicy { reason: &'static str },

    #[error("invalid sampler configuration: {reason}")]
    InvalidSamplerConfiguration { reason: &'static str },

    #[error("no CUDA-capable device is available")]
    NoDevice,

    #[error("{component} {operation} failed with status {status}: {message}")]
    NativeCallFailed {
        component: &'static str,
        operation: &'static str,
        status: u32,
        message: String,
    },

    #[error("{operation} succeeded without returning a {resource}")]
    MissingNativeResource {
        operation: &'static str,
        resource: &'static str,
    },

    #[error("{resource} size overflows the native address space")]
    ResourceSizeOverflow { resource: &'static str },

    #[error("native workspace requires {required} bytes, exceeding the {maximum}-byte limit")]
    WorkspaceLimitExceeded { required: usize, maximum: usize },

    #[error("invalid native result: {reason}")]
    InvalidNativeResult { reason: String },

    #[error("the engine's state is unusable after an earlier failure: {reason}")]
    Poisoned { reason: String },

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
