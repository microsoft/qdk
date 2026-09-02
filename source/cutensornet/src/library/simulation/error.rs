use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("invalid circuit: {reason}")]
    InvalidCircuit { reason: String },

    #[error("invalid native execution policy: {reason}")]
    InvalidExecutionPolicy { reason: &'static str },

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

    #[error("execution failed ({execution}); cleanup also failed ({cleanup})")]
    ExecutionAndCleanupFailed {
        execution: Box<Self>,
        cleanup: Box<Self>,
    },
}
