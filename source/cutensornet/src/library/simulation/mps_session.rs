//! MPS policy bound to a thread-confined native resource owner.

use crate::{
    library::CuTensorNetApi,
    simulation::{
        ExecutionPolicy, OpaqueHandle, SimulationError, Stream, resources::SessionResources,
    },
};
use std::sync::Arc;

/// MPS execution context: validated policy plus the shared native prerequisites.
///
/// Creates shorter-lived MPS executions; it is neither a shot nor a quantum
/// state. Contained resources enforce `!Send + !Sync`. General contraction
/// borrows `SessionResources` directly and requires no MPS policy.
pub struct MpsSession {
    resources: SessionResources<CuTensorNetApi>,
    policy: ExecutionPolicy,
}

impl MpsSession {
    pub(crate) fn new(
        api: Arc<CuTensorNetApi>,
        policy: ExecutionPolicy,
    ) -> Result<Self, SimulationError> {
        let policy = policy.validate()?;
        Ok(Self {
            resources: SessionResources::new(api, policy.device_ordinal)?,
            policy,
        })
    }

    #[must_use]
    pub fn device_ordinal(&self) -> i32 {
        self.resources.device_ordinal()
    }

    /// Consumes the session and reports native cleanup failures.
    pub(crate) fn close(self) -> Result<(), SimulationError> {
        self.resources.close()
    }

    pub(crate) fn api(&self) -> &CuTensorNetApi {
        self.resources.api()
    }

    pub(crate) fn stream(&self) -> Stream {
        self.resources.stream()
    }

    pub(crate) fn handle(&self) -> OpaqueHandle {
        self.resources.handle()
    }

    pub(crate) fn policy(&self) -> ExecutionPolicy {
        self.policy
    }
}
