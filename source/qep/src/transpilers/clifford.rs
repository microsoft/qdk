// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{Parameter::QubitId, QuantumBackendInstruction};
use smallvec::smallvec;

/// Decomposes non-native Clifford gates into H, S, S-adjoint, and CNOT.
pub enum CliffordTranspiler {
    Cy,
    Cz,
    Sx,
}

impl Transpiler for CliffordTranspiler {
    /// Returns the source gate selected by this decomposition.
    fn source_instruction_name(&self) -> &'static str {
        match self {
            Self::Cy => "cy",
            Self::Cz => "cz",
            Self::Sx => "sx",
        }
    }

    /// Appends an exact three-gate decomposition after validating the operands.
    fn transpile(
        &mut self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_name != self.source_instruction_name() {
            return Err(format!(
                "Expected {}, got {source:?}",
                self.source_instruction_name()
            ));
        }
        let gates = match (self, source.parameters.as_slice()) {
            (Self::Cy, &[QubitId(control), QubitId(target)]) => [
                ("sadj", smallvec![QubitId(target)]),
                ("cx", smallvec![QubitId(control), QubitId(target)]),
                ("s", smallvec![QubitId(target)]),
            ],
            (Self::Cz, &[QubitId(control), QubitId(target)]) => [
                ("h", smallvec![QubitId(target)]),
                ("cx", smallvec![QubitId(control), QubitId(target)]),
                ("h", smallvec![QubitId(target)]),
            ],
            (Self::Sx, &[QubitId(target)]) => [
                ("h", smallvec![QubitId(target)]),
                ("s", smallvec![QubitId(target)]),
                ("h", smallvec![QubitId(target)]),
            ],
            _ => return Err(format!("Malformed Clifford instruction: {source:?}")),
        };
        output.extend(gates.into_iter().map(|(instruction_name, parameters)| {
            QuantumBackendInstruction {
                instruction_name,
                parameters,
            }
        }));
        Ok(())
    }
}
