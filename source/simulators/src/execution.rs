// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Public API for preparing Adaptive programs, running them on executors of
//! the region ISA, and describing backend-neutral operations and observables.

mod adaptive;
mod operator_matrix;
mod pauli_sum;
mod protocol;
mod region;
mod runtime;
mod terminal;
pub mod tooling;
mod unitary;

pub use adaptive::{
    AdaptiveExecution, AdaptiveExecutionError, MeasuredQubit, MeasurementMetadataError,
    PreparedAdaptiveProgram, RegionPartitionError, RegionSite, partition_unitary_regions,
};
pub use operator_matrix::{OperatorMatrix, basis_operator, unitary_matrix};
pub use pauli_sum::{Pauli, PauliSum, PauliSumError, PauliTerm};
pub use protocol::{
    AdaptiveCommand, AdaptiveResponse, MeasurementKind, MeasurementRequest, RegionId,
    RegionInstruction,
};
pub use region::QuantumEvolutionRegion;
pub use runtime::{RuntimeAdapter, RuntimeError, RuntimeReport};
pub use terminal::{SimulatorReport, SimulatorTerminal};
pub use unitary::UnitaryOperation;
pub(crate) use unitary::{
    OPID_MRESETZ, OPID_MZ, OPID_RESETZ, apply_unitary_immediately, resolve_unitary_operation,
};

#[cfg(test)]
use adaptive::OP_QUANTUM_GATE;

#[cfg(test)]
mod tests;
