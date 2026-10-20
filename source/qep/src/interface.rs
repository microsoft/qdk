// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use smallvec::SmallVec;
use std::fmt;

#[derive(Clone, PartialEq)]
pub enum Parameter {
    Float(f64),
    ResultId(usize),
    QubitId(usize),
}

#[derive(Clone, PartialEq)]
pub struct QuantumBackendInstruction {
    pub instruction_name: &'static str,
    pub parameters: SmallVec<[Parameter; 3]>,
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
    /// Returns an error if the batch cannot be executed.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String>;
}

/// Driver is something that owns a backend and provides access to it.
/// Driver is a generic trait parametrized by type of its backend.
pub trait Driver {
    type Backend: QuantumBackend;

    /// Returns the wrapped backend.
    fn get_backend(&self) -> &Self::Backend;
}

// End of interface definition.

impl From<bool> for QuantumBackendResult {
    /// Converts a measurement bit to its backend result.
    fn from(value: bool) -> Self {
        if value { Self::One } else { Self::Zero }
    }
}

impl fmt::Debug for Parameter {
    /// Formats the contained value without its parameter variant name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Float(value) => write!(f, "{value}"),
            Self::ResultId(value) | Self::QubitId(value) => write!(f, "{value}"),
        }
    }
}

impl fmt::Debug for QuantumBackendInstruction {
    /// Formats the instruction name followed by space-separated qubits and classical values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.instruction_name)?;
        for parameter in &self.parameters {
            write!(f, " {parameter:?}")?;
        }
        Ok(())
    }
}
