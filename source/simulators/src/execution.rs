// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Public facade for shared simulator execution contracts and Adaptive control.

mod adaptive;
mod amplitude_contraction;
mod contraction;
mod fixed_outcome;
mod immediate;
mod operator_matrix;
mod pauli_sum;
mod protocol;
mod region;
mod tensor_network;
mod unitary;

pub use adaptive::{
    AdaptiveExecution, AdaptiveExecutionError, MeasuredQubit, MeasurementMetadataError,
    PreparedAdaptiveProgram, RegionPartitionError, RegionSite, partition_unitary_regions,
};
pub use amplitude_contraction::{
    AmplitudeContractionError, ContractedAmplitude, ContractionCost, ContractionReports,
    ExecutionFailure, PlannedContraction, closed_amplitude_query, contract_amplitude,
    contraction_cost,
};
pub use contraction::{
    ContractionContext, ContractionOptimizer, CostEstimate, EstimateKind, ExecutableContraction,
    ExecutionLimits, InputMutability, PlanningConstraints, PlanningReport, PreparationFailure,
    ResourceReport,
};
pub use fixed_outcome::{
    FixedOutcomeCircuit, FixedOutcomeCircuitError, FixedOutcomeError, FixedOutcomeOperation,
};
pub use immediate::{
    ImmediateExecutionReport, ImmediatePreparedRegion, ImmediateRegionReport,
    ImmediateSimulatorConsumer, ShotExecutionError, ShotExecutionOutput, ShotExecutionResult,
    drive_prepared_shot, run_prepared_shot,
};
pub use operator_matrix::{OperatorMatrix, basis_operator, unitary_matrix};
pub use pauli_sum::{Pauli, PauliSum, PauliSumError, PauliTerm};
pub use protocol::{
    AdaptiveCommand, AdaptiveResponse, MeasurementKind, MeasurementRequest, RegionId,
};
pub use region::{QuantumEvolutionRegion, RegionConsumer};
pub use tensor_network::{CircuitTensorNetwork, TensorNetworkBuildError};
pub use unitary::UnitaryOperation;
pub(crate) use unitary::{
    OPID_MRESETZ, OPID_MZ, OPID_RESETZ, apply_unitary_immediately, resolve_unitary_operation,
};

#[cfg(test)]
use adaptive::OP_QUANTUM_GATE;

#[cfg(test)]
mod tests;
