// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{InstructionId, Parameter::QubitId, QuantumBackendInstruction, instructions as instr};
use smallvec::smallvec;

/// Decomposes CCX into seven T/T-adjoint gates and six CNOTs without ancillas.
///
/// Uses Figure 1 of Shende and Markov, "On the CNOT-cost of TOFFOLI gates":
/// <https://arxiv.org/abs/0803.2316>.
#[derive(Default)]
pub struct DecomposeToffoliTranspiler;

impl Transpiler for DecomposeToffoliTranspiler {
    /// Returns the Toffoli instruction ID.
    fn source_instruction_id(&self) -> InstructionId {
        instr::CCX
    }

    /// Appends the Clifford+T decomposition of a Toffoli instruction.
    fn transpile(
        &mut self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_id != self.source_instruction_id() {
            return Err(format!("Expected a CCX instruction, got {source:?}"));
        }
        let &[QubitId(ctl0), QubitId(ctl1), QubitId(target)] = source.parameters.as_slice() else {
            return Err(format!("Malformed CCX instruction: {source:?}"));
        };
        output.extend(
            [
                (instr::H, smallvec![QubitId(target)]),
                (instr::CX, smallvec![QubitId(ctl1), QubitId(target)]),
                (instr::TADJ, smallvec![QubitId(target)]),
                (instr::CX, smallvec![QubitId(ctl0), QubitId(target)]),
                (instr::T, smallvec![QubitId(target)]),
                (instr::CX, smallvec![QubitId(ctl1), QubitId(target)]),
                (instr::TADJ, smallvec![QubitId(target)]),
                (instr::CX, smallvec![QubitId(ctl0), QubitId(target)]),
                (instr::T, smallvec![QubitId(ctl1)]),
                (instr::T, smallvec![QubitId(target)]),
                (instr::H, smallvec![QubitId(target)]),
                (instr::CX, smallvec![QubitId(ctl0), QubitId(ctl1)]),
                (instr::T, smallvec![QubitId(ctl0)]),
                (instr::TADJ, smallvec![QubitId(ctl1)]),
                (instr::CX, smallvec![QubitId(ctl0), QubitId(ctl1)]),
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
