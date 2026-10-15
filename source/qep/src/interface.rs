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

impl From<bool> for QuantumBackendResult {
    /// Converts a measurement bit to its backend result.
    fn from(value: bool) -> Self {
        if value { Self::One } else { Self::Zero }
    }
}

impl From<Option<bool>> for BatchResult {
    /// Stores an optional QIR measurement without allocating.
    fn from(value: Option<bool>) -> Self {
        Self {
            results: value.into_iter().map(QuantumBackendResult::from).collect(),
        }
    }
}

pub trait QuantumBackend {
    /// Executes an ordered batch whose optional `read_result` instruction is last.
    ///
    /// Returns no results without `read_result`, or exactly one result when present.
    fn execute_batch(&mut self, instructions: &[QirInstruction]) -> BatchResult;
}

#[cfg(test)]
mod tests {
    use super::{BatchResult, QuantumBackendResult};

    #[test]
    /// Keeps empty and single-result batches inline.
    fn qir_results_do_not_allocate() {
        for (value, expected) in [
            (None, None),
            (Some(false), Some(QuantumBackendResult::Zero)),
            (Some(true), Some(QuantumBackendResult::One)),
        ] {
            let batch = BatchResult::from(value);
            assert_eq!(batch.results.as_slice(), expected.as_slice());
            assert!(!batch.results.spilled());
        }
        let batch = BatchResult::default();
        assert!(batch.results.is_empty());
        assert!(!batch.results.spilled());
    }

    #[test]
    /// Preserves the order of larger result lists, including errors.
    fn supports_multiple_results() {
        let expected = [
            QuantumBackendResult::One,
            QuantumBackendResult::Error,
            QuantumBackendResult::Zero,
        ];
        let batch = BatchResult {
            results: expected.into_iter().collect(),
        };
        assert_eq!(batch.results.as_slice(), expected);
        assert!(batch.results.spilled());
    }
}
