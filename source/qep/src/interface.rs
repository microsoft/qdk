// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use smallvec::SmallVec;

type QubitID = usize;

#[derive(Clone, Debug, PartialEq)]
pub enum ClassicalParameter {
    Float(f64),
    ResultId(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuantumBackendInstruction {
    pub instruction_name: &'static str,
    pub targets: SmallVec<[QubitID; 3]>,
    pub classical_parameters: SmallVec<[ClassicalParameter; 1]>,
}

/// A measurement value or a backend-reported error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantumBackendResult {
    Zero,
    One,
    Error,
}

/// Ordered backend results, with zero or one result stored without heap allocation.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BatchResult {
    pub results: SmallVec<[QuantumBackendResult; 1]>,
}

pub trait QuantumBackend {
    /// Produces one result per `read_result`, `mz`, or `mz_reset`, in input order.
    fn execute_batch(&mut self, instructions: &[QuantumBackendInstruction]) -> BatchResult;
}

// End of interface definition.

impl From<bool> for QuantumBackendResult {
    /// Converts a measurement bit to its backend result.
    fn from(value: bool) -> Self {
        if value { Self::One } else { Self::Zero }
    }
}
