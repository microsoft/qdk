// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_isa::QirInstruction;

#[derive(Debug)]
pub enum QuantumInstruction {
    QirInstruction(QirInstruction),
}

/// By contract, a QIR runner batch may have no more than one `read_result` instruction.
/// When present, its value is written to `QirBatchResult.read_result`.
/// A batch without `read_result` instructions has no result.
#[derive(Debug)]
pub struct QirBatchResult {
    /// Value returned by the final `read_result` instruction, if present.
    pub read_result: Option<bool>,
}

#[derive(Debug)]
pub enum BatchResult {
    /// Result of executing a batch of QIR instructions.
    QirBatchResult(QirBatchResult),
}

pub trait QuantumBackend {
    /// Executes an ordered batch whose optional `read_result` instruction is last.
    fn execute_batch(&mut self, instructions: &[QirInstruction]) -> BatchResult;
}
