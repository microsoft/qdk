// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::Serialize;
use smallvec::SmallVec;
use std::fmt;

use crate::{InstructionId, instructions};

#[derive(Clone, PartialEq, Serialize)]
pub enum Parameter {
    Float(f64),
    ResultId(usize),
    QubitId(usize),
}

#[derive(Clone, PartialEq, Serialize)]
pub struct QuantumBackendInstruction {
    pub instruction_id: InstructionId,
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
    type Backend: QuantumBackend + ?Sized;

    /// Returns the wrapped backend.
    fn get_backend(&self) -> &Self::Backend;
}

// End of interface definition.

impl<T: QuantumBackend + ?Sized> QuantumBackend for Box<T> {
    /// Forwards instruction execution to the boxed backend.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        (**self).execute_batch(instructions)
    }
}

impl<T: QuantumBackend + ?Sized> Driver for Box<T> {
    type Backend = T;

    /// Returns the boxed backend, including dynamically dispatched backends.
    fn get_backend(&self) -> &Self::Backend {
        self.as_ref()
    }
}

impl<T: QuantumBackend + ?Sized> QuantumBackend for &mut T {
    /// Forwards instruction execution to the borrowed backend.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        (**self).execute_batch(instructions)
    }
}

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
            Self::ResultId(value) | Self::QubitId(value) => {
                write!(f, "{value}")
            }
        }
    }
}

impl fmt::Debug for QuantumBackendInstruction {
    /// Formats the instruction name followed by space-separated qubits and classical values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.instruction_id < instructions::OPERATION_COUNT {
            f.write_str(instructions::instruction_id_to_name(self.instruction_id))?;
        } else {
            write!(f, "unknown instruction ID {}", self.instruction_id)?;
        }
        for parameter in &self.parameters {
            write!(f, " {parameter:?}")?;
        }
        Ok(())
    }
}
