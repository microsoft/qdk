// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{InstructionId, Parameter::QubitId, QuantumBackendInstruction, instructions as instr};
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
            Self::Cy => instr::CY,
            Self::Cz => instr::CZ,
            Self::Sx => instr::SX,
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
                instr::instruction_id_to_name(self.source_instruction_id())
            ));
        }
        let gates = match (self, source.parameters.as_slice()) {
            (Self::Cy, &[QubitId(control), QubitId(target)]) => [
                (instr::SADJ, smallvec![QubitId(target)]),
                (instr::CX, smallvec![QubitId(control), QubitId(target)]),
                (instr::S, smallvec![QubitId(target)]),
            ],
            (Self::Cz, &[QubitId(control), QubitId(target)]) => [
                (instr::H, smallvec![QubitId(target)]),
                (instr::CX, smallvec![QubitId(control), QubitId(target)]),
                (instr::H, smallvec![QubitId(target)]),
            ],
            (Self::Sx, &[QubitId(target)]) => [
                (instr::H, smallvec![QubitId(target)]),
                (instr::S, smallvec![QubitId(target)]),
                (instr::H, smallvec![QubitId(target)]),
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
