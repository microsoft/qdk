// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{InstructionId, Parameter::QubitId, QuantumBackendInstruction, instructions};
use smallvec::smallvec;

/// Decomposes a SWAP into three controlled-X instructions.
#[derive(Default)]
pub struct SwapTransplier;

impl Transpiler for SwapTransplier {
    /// Returns the SWAP instruction ID.
    fn source_instruction_id(&self) -> InstructionId {
        instructions::SWAP
    }

    /// Appends three controlled-X gates that exchange the source qubits.
    fn transpile(
        &mut self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_id != self.source_instruction_id() {
            return Err(format!("Expected a SWAP instruction, got {source:?}"));
        }
        let &[QubitId(q1), QubitId(q2)] = source.parameters.as_slice() else {
            return Err(format!("Malformed SWAP instruction: {source:?}"));
        };
        output.extend(
            [
                (instructions::CX, smallvec![QubitId(q1), QubitId(q2)]),
                (instructions::CX, smallvec![QubitId(q2), QubitId(q1)]),
                (instructions::CX, smallvec![QubitId(q1), QubitId(q2)]),
            ]
            .into_iter()
            .map(|(instruction_id, parameters)| QuantumBackendInstruction {
                instruction_id,
                parameters,
            }),
        );
        Ok(())
    }
}
