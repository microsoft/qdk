// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{BatchResult, QuantumBackend, QuantumBackendInstruction, interface::Driver};
use smallvec::smallvec;

/// Decomposes CCX instructions into Clifford+T gates and forwards results unchanged.
///
/// Uses the ancilla-free decomposition with seven T/T-adjoint gates and six CNOTs
/// from Figure 1 of Shende and Markov, "On the CNOT-cost of TOFFOLI gates":
/// <https://arxiv.org/abs/0803.2316>.
pub struct DecomposeToffoliAdapter<B> {
    backend: B,
}

impl<B: QuantumBackend> DecomposeToffoliAdapter<B> {
    /// Wraps a backend that can execute the Clifford+T decomposition.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B: QuantumBackend> Driver for DecomposeToffoliAdapter<B> {
    type Backend = B;

    /// Returns the wrapped backend.
    fn get_backend(&self) -> &B {
        &self.backend
    }
}

impl<B: QuantumBackend> QuantumBackend for DecomposeToffoliAdapter<B> {
    /// Replaces each CCX in place and returns the downstream batch result unchanged.
    /// See fig.1 in https://arxiv.org/pdf/0803.2316
    fn execute_batch(&mut self, instructions: &[QuantumBackendInstruction]) -> BatchResult {
        let mut transformed = Vec::with_capacity(instructions.len());
        for instruction in instructions {
            #[cfg(debug_assertions)]
            crate::validate_instruction(instruction);

            if instruction.instruction_name != "ccx" {
                transformed.push(instruction.clone());
                continue;
            }

            let &[ctl0, ctl1, target] = instruction.targets.as_slice() else {
                panic!("Malformed CCX instruction: {instruction:?}");
            };
            transformed.extend(
                [
                    ("h", smallvec![target]),
                    ("cx", smallvec![ctl1, target]),
                    ("tadj", smallvec![target]),
                    ("cx", smallvec![ctl0, target]),
                    ("t", smallvec![target]),
                    ("cx", smallvec![ctl1, target]),
                    ("tadj", smallvec![target]),
                    ("cx", smallvec![ctl0, target]),
                    ("t", smallvec![ctl1]),
                    ("t", smallvec![target]),
                    ("h", smallvec![target]),
                    ("cx", smallvec![ctl0, ctl1]),
                    ("t", smallvec![ctl0]),
                    ("tadj", smallvec![ctl1]),
                    ("cx", smallvec![ctl0, ctl1]),
                ]
                .into_iter()
                .map(|(instruction_name, targets)| QuantumBackendInstruction {
                    instruction_name,
                    targets,
                    classical_parameters: smallvec![],
                }),
            );
        }
        self.backend.execute_batch(&transformed)
    }
}
