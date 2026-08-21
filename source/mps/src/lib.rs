// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Backend-neutral matrix product state simulation contracts.

mod engine;
mod error;
mod factory;
mod observable;
mod operation;
mod policy;
mod report;
mod rng;
mod simulator;

#[cfg(all(feature = "tensor4all-cpu", not(target_arch = "wasm32")))]
pub mod tensor4all;

pub use engine::{Matrix2, Matrix4, MpsEngine, SiteId};
pub use error::MpsError;
pub use factory::MpsEngineFactory;
pub use observable::{Pauli, PauliObservable, PauliTerm, SitePauli};
pub use operation::{
    Measurement, OneQubitGate, Operation, OperationOutcome, QubitId, TwoQubitGate,
};
pub use policy::{ExecutionPolicy, Precision, ResourcePolicy, TruncationPolicy};
pub use report::{
    CapStatus, CapabilityStatus, EngineDescriptor, EngineInfo, ExecutionReport, MpsCapabilities,
    OperationCounts, ReleaseOutcome, ResourceResolution, ResourceResolutionSource, TimingReport,
};
pub use rng::derive_shot_seed;
pub use simulator::MpsSimulator;
