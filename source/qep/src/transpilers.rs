// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod clifford;
mod mz_reset;
mod rz;
mod rz_qft;
mod swap;
mod tolffoli;

pub use clifford::CliffordTranspiler;
pub use mz_reset::MzResetTranspiler;
pub use rz::RzTranspiler;
pub use rz_qft::RzQftTranspiler;
pub use swap::SwapTransplier;
pub use tolffoli::DecomposeToffoliTranspiler;

use crate::{InstructionId, QuantumBackendInstruction};

/// Replaces an instruction of one type with an exact or approximate sequence.
pub trait Transpiler: Send + Sync {
    /// Returns the instruction ID accepted by this transpiler.
    fn source_instruction_id(&self) -> InstructionId;

    /// Appends replacement instructions, updating transpiler state and rejecting invalid input.
    fn transpile(
        &mut self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String>;
}
