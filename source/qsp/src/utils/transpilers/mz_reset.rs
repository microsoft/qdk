// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{InstructionId, Parameter::QubitId, QuantumBackendInstruction, instructions as instr};
use smallvec::smallvec;

/// Decomposes measurement with reset into an immediate measurement followed by reset.
#[derive(Default)]
pub struct MzResetTranspiler;

impl Transpiler for MzResetTranspiler {
    /// Returns the measurement-with-reset instruction ID.
    fn source_instruction_id(&self) -> InstructionId {
        instr::MZ_RESET
    }

    /// Appends one measurement result and a reset on the same qubit.
    fn transpile(
        &mut self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_id != self.source_instruction_id() {
            return Err(format!("Expected an MZ_RESET instruction, got {source:?}"));
        }
        let &[QubitId(qubit)] = source.parameters.as_slice() else {
            return Err(format!("Malformed MZ_RESET instruction: {source:?}"));
        };
        output.extend(
            [instr::MZ, instr::RESET].map(|instruction_id| QuantumBackendInstruction {
                instruction_id,
                parameters: smallvec![QubitId(qubit)],
            }),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Parameter, QuantumBackend,
        QuantumBackendResult::{One, Zero},
        QuantumBackends, ResultAdapter, TranspilingAdapter,
    };

    /// Rejects invalid source IDs and operands without emitting partial output.
    #[test]
    fn rejects_invalid_instructions() {
        for (instruction_id, parameters) in [
            (instr::MZ, smallvec![QubitId(0)]),
            (instr::MZ_RESET, smallvec![]),
            (instr::MZ_RESET, smallvec![Parameter::ResultId(0)]),
            (instr::MZ_RESET, smallvec![QubitId(0), QubitId(1)]),
        ] {
            let mut output = Vec::new();
            assert!(
                MzResetTranspiler
                    .transpile(
                        &QuantumBackendInstruction {
                            instruction_id,
                            parameters
                        },
                        &mut output,
                    )
                    .is_err()
            );
            assert!(output.is_empty());
        }
    }

    /// Preserves measurement order, stored results, and reset effects through default adapters.
    #[test]
    fn adapts_immediate_and_stored_measurements() -> Result<(), String> {
        let mut backend = ResultAdapter::new(TranspilingAdapter::with_defaults(
            QuantumBackends::sparse_sim(),
        ));
        let instructions = [
            (instr::X, smallvec![QubitId(0)]),
            (instr::MZ_RESET, smallvec![QubitId(0)]),
            (instr::MZ, smallvec![QubitId(0)]),
            (instr::X, smallvec![QubitId(0)]),
            (
                instr::MZ_STORE_RESET,
                smallvec![QubitId(0), Parameter::ResultId(0)],
            ),
            (instr::READ_RESULT, smallvec![Parameter::ResultId(0)]),
            (instr::MZ, smallvec![QubitId(0)]),
        ]
        .map(|(instruction_id, parameters)| QuantumBackendInstruction {
            instruction_id,
            parameters,
        });
        let results = backend.execute_batch(&instructions)?;
        assert_eq!(results.results.as_slice(), &[One, Zero, One, Zero]);
        let results = backend.execute_batch(&[QuantumBackendInstruction {
            instruction_id: instr::READ_RESULT,
            parameters: smallvec![Parameter::ResultId(0)],
        }])?;
        assert_eq!(results.results.as_slice(), &[One]);
        Ok(())
    }

}
