// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    EngineDescriptor, ExecutionPolicy, MpsCapabilities, MpsEngine, MpsError, MpsSimulator,
};

pub trait MpsEngineFactory: Send + Sync {
    type Engine: MpsEngine;

    fn descriptor(&self) -> EngineDescriptor;
    fn capabilities(&self) -> MpsCapabilities;
    fn create_engine(&self, policy: &ExecutionPolicy) -> Result<Self::Engine, MpsError>;

    fn create_simulator(
        &self,
        policy: ExecutionPolicy,
    ) -> Result<MpsSimulator<Self::Engine>, MpsError>
    where
        Self: Sized,
    {
        policy.validate()?;
        let initialization_start = std::time::Instant::now();
        let engine = self.create_engine(&policy)?;
        MpsSimulator::from_resolved(
            policy,
            engine,
            self.descriptor(),
            self.capabilities(),
            initialization_start.elapsed(),
        )
    }
}
