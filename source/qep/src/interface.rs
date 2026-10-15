// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_isa::QirInstruction;
use smallvec::SmallVec;

#[derive(Debug)]
pub enum QuantumInstruction {
    QirInstruction(QirInstruction),
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
    /// Must produce exactly one result per each read_result in input, in the same order.
    fn execute_batch(&mut self, instructions: &[QirInstruction]) -> BatchResult;
}

////// End of interface definition.

impl From<bool> for QuantumBackendResult {
    /// Converts a measurement bit to its backend result.
    fn from(value: bool) -> Self {
        if value { Self::One } else { Self::Zero }
    }
}
