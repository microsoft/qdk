// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Public facade for shared simulator execution contracts and Adaptive control.

mod adaptive;
mod contraction;
mod immediate;
mod protocol;
mod region;
mod tensor_network;
mod unitary;

pub use adaptive::{
    AdaptiveExecution, AdaptiveExecutionError, MeasuredQubit, MeasurementMetadataError,
    PreparedAdaptiveProgram, RegionPartitionError, RegionSite, partition_unitary_regions,
};
pub use contraction::{
    ContractionContext, ContractionOptimizer, CostEstimate, EstimateKind, ExecutableContraction,
    ExecutionLimits, InputMutability, PlanningConstraints, PlanningReport, PreparationFailure,
    ResourceReport,
};
pub use immediate::{
    ImmediateExecutionReport, ImmediatePreparedRegion, ImmediateRegionReport,
    ImmediateSimulatorConsumer, ShotExecutionError, ShotExecutionOutput, ShotExecutionResult,
    drive_prepared_shot, run_prepared_shot,
};
pub use protocol::{
    AdaptiveCommand, AdaptiveResponse, MeasurementKind, MeasurementRequest, RegionId,
};
pub use region::{QuantumEvolutionRegion, RegionConsumer};
pub use tensor_network::{CircuitTensorNetwork, TensorNetworkBuildError};
pub use unitary::UnitaryOperation;
pub(crate) use unitary::{
    OPID_MRESETZ, OPID_MZ, apply_unitary_immediately, resolve_unitary_operation,
};

#[cfg(test)]
use adaptive::OP_QUANTUM_GATE;

#[cfg(test)]
mod tests;
