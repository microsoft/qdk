// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{Parameter::QubitId, QuantumBackendInstruction};
use smallvec::smallvec;

/// Decomposes CCX into seven T/T-adjoint gates and six CNOTs without ancillas.
///
/// Uses Figure 1 of Shende and Markov, "On the CNOT-cost of TOFFOLI gates":
/// <https://arxiv.org/abs/0803.2316>.
#[derive(Default)]
pub struct DecomposeToffoliTranspiler;

impl Transpiler for DecomposeToffoliTranspiler {
    /// Returns the Toffoli instruction name.
    fn source_instruction_name(&self) -> &'static str {
        "ccx"
    }

    /// Appends the Clifford+T decomposition of a Toffoli instruction.
    fn transpile(
        &self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_name != self.source_instruction_name() {
            return Err(format!("Expected a CCX instruction, got {source:?}"));
        }
        let &[QubitId(ctl0), QubitId(ctl1), QubitId(target)] = source.parameters.as_slice() else {
            return Err(format!("Malformed CCX instruction: {source:?}"));
        };
        output.extend(
            [
                ("h", smallvec![QubitId(target)]),
                ("cx", smallvec![QubitId(ctl1), QubitId(target)]),
                ("tadj", smallvec![QubitId(target)]),
                ("cx", smallvec![QubitId(ctl0), QubitId(target)]),
                ("t", smallvec![QubitId(target)]),
                ("cx", smallvec![QubitId(ctl1), QubitId(target)]),
                ("tadj", smallvec![QubitId(target)]),
                ("cx", smallvec![QubitId(ctl0), QubitId(target)]),
                ("t", smallvec![QubitId(ctl1)]),
                ("t", smallvec![QubitId(target)]),
                ("h", smallvec![QubitId(target)]),
                ("cx", smallvec![QubitId(ctl0), QubitId(ctl1)]),
                ("t", smallvec![QubitId(ctl0)]),
                ("tadj", smallvec![QubitId(ctl1)]),
                ("cx", smallvec![QubitId(ctl0), QubitId(ctl1)]),
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
