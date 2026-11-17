// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{InstructionId, Parameter::QubitId, QuantumBackendInstruction, instructions};
use smallvec::smallvec;

/// Decomposes non-native Clifford gates into H, S, S-adjoint, and CNOT.
pub enum CliffordTranspiler {
    Cy,
    Cz,
    Sx,
}

impl Transpiler for CliffordTranspiler {
    /// Returns the source gate selected by this decomposition.
    fn source_instruction_id(&self) -> InstructionId {
        match self {
            Self::Cy => instructions::CY,
            Self::Cz => instructions::CZ,
            Self::Sx => instructions::SX,
        }
    }

    /// Appends an exact three-gate decomposition after validating the operands.
    fn transpile(
        &mut self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_id != self.source_instruction_id() {
            return Err(format!(
                "Expected {}, got {source:?}",
                instructions::instruction_id_to_name(self.source_instruction_id())
            ));
        }
        let gates = match (self, source.parameters.as_slice()) {
            (Self::Cy, &[QubitId(control), QubitId(target)]) => [
                (instructions::SADJ, smallvec![QubitId(target)]),
                (
                    instructions::CX,
                    smallvec![QubitId(control), QubitId(target)],
                ),
                (instructions::S, smallvec![QubitId(target)]),
            ],
            (Self::Cz, &[QubitId(control), QubitId(target)]) => [
                (instructions::H, smallvec![QubitId(target)]),
                (
                    instructions::CX,
                    smallvec![QubitId(control), QubitId(target)],
                ),
                (instructions::H, smallvec![QubitId(target)]),
            ],
            (Self::Sx, &[QubitId(target)]) => [
                (instructions::H, smallvec![QubitId(target)]),
                (instructions::S, smallvec![QubitId(target)]),
                (instructions::H, smallvec![QubitId(target)]),
            ],
            _ => return Err(format!("Malformed Clifford instruction: {source:?}")),
        };
        output.extend(gates.into_iter().map(|(instruction_id, parameters)| {
            QuantumBackendInstruction {
                instruction_id,
                parameters,
            }
        }));
        Ok(())
    }
}
