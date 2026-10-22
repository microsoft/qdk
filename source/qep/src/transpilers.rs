// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod rz_qft;
mod swap;
mod tolffoli;

pub use rz_qft::RzQftTranspiler;
pub use swap::SwapTransplier;
pub use tolffoli::DecomposeToffoliTranspiler;

use crate::QuantumBackendInstruction;

/// Replaces an instruction of one type with an exact or approximate sequence.
pub trait Transpiler: Send + Sync {
    /// Returns the instruction name accepted by this transpiler.
    fn source_instruction_name(&self) -> &'static str;

    /// Appends replacement instructions, returning an error for invalid input.
    fn transpile(
        &self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String>;
}
