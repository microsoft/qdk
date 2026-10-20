// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{Parameter::QubitId, QuantumBackendInstruction};
use smallvec::smallvec;

/// Decomposes a SWAP into three controlled-X instructions.
#[derive(Default)]
pub struct SwapTransplier;

impl Transpiler for SwapTransplier {
    /// Returns the SWAP instruction name.
    fn source_instruction_name(&self) -> &'static str {
        "swap"
    }

    /// Appends three controlled-X gates that exchange the source qubits.
    fn transpile(
        &self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_name != self.source_instruction_name() {
            return Err(format!("Expected a SWAP instruction, got {source:?}"));
        }
        let &[QubitId(q1), QubitId(q2)] = source.parameters.as_slice() else {
            return Err(format!("Malformed SWAP instruction: {source:?}"));
        };
        output.extend(
            [
                ("cx", smallvec![QubitId(q1), QubitId(q2)]),
                ("cx", smallvec![QubitId(q2), QubitId(q1)]),
                ("cx", smallvec![QubitId(q1), QubitId(q2)]),
            ]
            .into_iter()
            .map(|(instruction_name, parameters)| QuantumBackendInstruction {
                instruction_name,
                parameters,
            }),
        );
        Ok(())
    }
}
