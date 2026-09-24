use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("invalid circuit: {reason}")]
    InvalidCircuit { reason: String },

    #[error("invalid native execution policy: {reason}")]
    InvalidExecutionPolicy { reason: &'static str },

    #[error("invalid sampler configuration: {reason}")]
    InvalidSamplerConfiguration { reason: &'static str },

    #[error("invalid contraction configuration: {reason}")]
    InvalidContractionConfiguration { reason: &'static str },

    #[error("contraction executable is unusable after an input or execution failure")]
    UnusableContraction,

    #[error("invalid portable contraction plan: {error}")]
    InvalidContractionPlan {
        #[source]
        error: tensornet::PlanError,
    },

    #[error("unsupported contraction capability: {reason}")]
    UnsupportedContraction { reason: &'static str },

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

    #[error("failed to allocate {bytes} bytes of host scratch")]
    HostScratchAllocationFailed { bytes: usize },

    #[error("invalid native result: {reason}")]
    InvalidNativeResult { reason: String },

    #[error("execution failed ({execution}); cleanup also failed ({cleanup})")]
    ExecutionAndCleanupFailed {
        execution: Box<Self>,
        cleanup: Box<Self>,
    },
}

pub(super) fn combine_execution_and_cleanup<T>(
    execution: Result<T, SimulationError>,
    cleanup: Result<(), SimulationError>,
) -> Result<T, SimulationError> {
    match (execution, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(execution), Err(cleanup)) => Err(SimulationError::ExecutionAndCleanupFailed {
            execution: Box::new(execution),
            cleanup: Box::new(cleanup),
        }),
    }
}
