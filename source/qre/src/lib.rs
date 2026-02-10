// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use thiserror::Error;

mod isa;
mod pareto;
pub use pareto::{
    ParetoFrontier as ParetoFrontier2D, ParetoFrontier3D, ParetoItem2D, ParetoItem3D,
};
mod result;
pub use result::{EstimationCollection, EstimationResult, FactoryResult};
mod trace;
pub use isa::{
    ConstraintBound, Encoding, ISA, ISARequirements, Instruction, InstructionConstraint,
    ProvenanceGraph, VariableArityFunction,
};
pub use trace::instruction_ids;
pub use trace::instruction_ids::instruction_name;
pub use trace::{Block, LatticeSurgery, PSSPC, Property, Trace, TraceTransform, estimate_parallel};
mod utils;
pub use utils::binom_ppf;

/// A resourc estimation error.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum Error {
    /// The resource estimation exceeded the maximum allowed error.
    #[error("resource estimation exceeded the maximum allowed error: {actual_error} > {max_error}")]
    MaximumErrorExceeded { actual_error: f64, max_error: f64 },
    /// Missing instruction in the ISA.
    #[error("requested instruction {0} not present in ISA")]
    InstructionNotFound(u64),
    /// Cannot extract space from instruction.
    #[error("cannot extract space from instruction {0} for fixed arity")]
    CannotExtractSpace(u64),
    /// Cannot extract time from instruction.
    #[error("cannot extract time from instruction {0} for fixed arity")]
    CannotExtractTime(u64),
    /// Cannot extract error rate from instruction.
    #[error("cannot extract error rate from instruction {0} for fixed arity")]
    CannotExtractErrorRate(u64),
    /// Factory time exceeds algorithm runtime
    #[error(
        "factory instruction {id} time {factory_time} exceeds algorithm runtime {algorithm_runtime}"
    )]
    FactoryTimeExceedsAlgorithmRuntime {
        id: u64,
        factory_time: u64,
        algorithm_runtime: u64,
    },
    /// Unsupported instruction in trace transformation
    #[error("unsupported instruction {id} in trace transformation '{name}'")]
    UnsupportedInstruction { id: u64, name: &'static str },
}
