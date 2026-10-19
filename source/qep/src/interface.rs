// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use smallvec::SmallVec;
use std::fmt;

type QubitID = usize;

#[derive(Clone, PartialEq)]
pub enum ClassicalParameter {
    Float(f64),
    ResultId(usize),
}

#[derive(Clone, PartialEq)]
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

impl fmt::Debug for ClassicalParameter {
    /// Formats the contained value without its parameter variant name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Float(value) => write!(f, "{value}"),
            Self::ResultId(value) => write!(f, "{value}"),
        }
    }
}

impl fmt::Debug for QuantumBackendInstruction {
    /// Formats the instruction name followed by space-separated qubits and classical values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.instruction_name)?;
        for qubit in &self.targets {
            write!(f, " {qubit}")?;
        }
        for parameter in &self.classical_parameters {
            write!(f, " {parameter:?}")?;
        }
        Ok(())
    }
}
