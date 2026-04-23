// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use thiserror::Error;

mod isa;
mod pareto;
pub use pareto::{
    ParetoFrontier as ParetoFrontier2D, ParetoFrontier3D, ParetoItem2D, ParetoItem3D,
};
mod result;
pub use isa::property_keys;
pub use isa::property_keys::{property_name, property_name_to_key};
pub use isa::{
    ConstraintBound, Encoding, ISA, ISARequirements, Instruction, InstructionConstraint, LockedISA,
    ProvenanceGraph, VariableArityFunction,
};
pub use result::{EstimationCollection, EstimationResult, FactoryResult, ResultSummary};
mod trace;
pub use trace::instruction_ids;
pub use trace::instruction_ids::instruction_name;
pub use trace::{
    Block, LatticeSurgery, PSSPC, Property, Trace, TraceTransform, estimate_parallel,
    estimate_with_graph,
};
mod utils;
pub use utils::{binom_ppf, float_from_bits, float_to_bits};

/// A resource estimation error.
#[derive(Clone, Debug, Error, Diagnostic, PartialEq)]
pub enum Error {
    /// The resource estimation exceeded the maximum allowed error.
    #[error("resource estimation exceeded the maximum allowed error: {actual_error} > {max_error}")]
    #[diagnostic(code("Qre.MaximumErrorExceeded"))]
    MaximumErrorExceeded { actual_error: f64, max_error: f64 },
    /// Missing instruction in the ISA.
    #[error("requested instruction {0} not present in ISA")]
    #[diagnostic(code("Qre.InstructionNotFound"))]
    InstructionNotFound(u64),
    /// Cannot extract space from instruction.
    #[error("cannot extract space from instruction {0} for fixed arity")]
    #[diagnostic(code("Qre.CannotExtractSpace"))]
    CannotExtractSpace(u64),
    /// Cannot extract time from instruction.
    #[error("cannot extract time from instruction {0} for fixed arity")]
    #[diagnostic(code("Qre.CannotExtractTime"))]
    CannotExtractTime(u64),
    /// Cannot extract error rate from instruction.
    #[error("cannot extract error rate from instruction {0} for fixed arity")]
    #[diagnostic(code("Qre.CannotExtractErrorRate"))]
    CannotExtractErrorRate(u64),
    /// Factory time exceeds algorithm runtime
    #[error(
        "factory instruction {id} time {factory_time} exceeds algorithm runtime {algorithm_runtime}"
    )]
    #[diagnostic(code("Qre.FactoryTimeExceedsAlgorithmRuntime"))]
    FactoryTimeExceedsAlgorithmRuntime {
        id: u64,
        factory_time: u64,
        algorithm_runtime: u64,
    },
    /// Unsupported instruction in trace transformation
    #[error("unsupported instruction {} in trace transformation '{name}'", instruction_name(*id).unwrap_or(&id.to_string()))]
    #[diagnostic(code("Qre.UnsupportedInstruction"))]
    UnsupportedInstruction { id: u64, name: &'static str },
}
