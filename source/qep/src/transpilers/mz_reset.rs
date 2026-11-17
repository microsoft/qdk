// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{InstructionId, Parameter::QubitId, QuantumBackendInstruction, instructions};
use smallvec::smallvec;

/// Decomposes measurement with reset into an immediate measurement followed by reset.
#[derive(Default)]
pub struct MzResetTranspiler;

impl Transpiler for MzResetTranspiler {
    /// Returns the measurement-with-reset instruction ID.
    fn source_instruction_id(&self) -> InstructionId {
        instructions::MZ_RESET
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
            [instructions::MZ, instructions::RESET].map(|instruction_id| {
                QuantumBackendInstruction {
                    instruction_id,
                    parameters: smallvec![QubitId(qubit)],
                }
            }),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Parameter, Profiler, QuantumBackend,
        QuantumBackendResult::{One, Zero},
        QuantumBackends, ResultAdapter, TemporalProfilerConfig, TranspilingAdapter,
        make_profiler_for_demo_model,
    };

    /// Rejects invalid source IDs and operands without emitting partial output.
    #[test]
    fn rejects_invalid_instructions() {
        for (instruction_id, parameters) in [
            (instructions::MZ, smallvec![QubitId(0)]),
            (instructions::MZ_RESET, smallvec![]),
            (instructions::MZ_RESET, smallvec![Parameter::ResultId(0)]),
            (instructions::MZ_RESET, smallvec![QubitId(0), QubitId(1)]),
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
            (instructions::X, smallvec![QubitId(0)]),
            (instructions::MZ_RESET, smallvec![QubitId(0)]),
            (instructions::MZ, smallvec![QubitId(0)]),
            (instructions::X, smallvec![QubitId(0)]),
            (
                instructions::MZ_STORE_RESET,
                smallvec![QubitId(0), Parameter::ResultId(0)],
            ),
            (instructions::READ_RESULT, smallvec![Parameter::ResultId(0)]),
            (instructions::MZ, smallvec![QubitId(0)]),
        ]
        .map(|(instruction_id, parameters)| QuantumBackendInstruction {
            instruction_id,
            parameters,
        });
        let results = backend.execute_batch(&instructions)?;
        assert_eq!(results.results.as_slice(), &[One, Zero, One, Zero]);
        let results = backend.execute_batch(&[QuantumBackendInstruction {
            instruction_id: instructions::READ_RESULT,
            parameters: smallvec![Parameter::ResultId(0)],
        }])?;
        assert_eq!(results.results.as_slice(), &[One]);
        Ok(())
    }

    /// Charges separate measurement and reset intervals while returning only one result.
    #[test]
    fn profiles_measurement_then_reset() -> Result<(), String> {
        let mut profiler = TranspilingAdapter::with_defaults(make_profiler_for_demo_model(
            TemporalProfilerConfig {
                record_trace: true,
                seed: Some(42),
                ..TemporalProfilerConfig::default()
            },
        ));
        let result = profiler.execute_batch(&[QuantumBackendInstruction {
            instruction_id: instructions::MZ_RESET,
            parameters: smallvec![QubitId(2)],
        }])?;
        assert_eq!(result.results.len(), 1);
        let profile = serde_json::to_value(profiler.get_result()).expect("profile serializes");
        let instructions: Vec<_> = profile["trace"]["instructions"]
            .as_array()
            .expect("trace instructions")
            .iter()
            .filter(|entry| entry["instruction"]["instruction_name"] != "round")
            .collect();
        assert_eq!(instructions.len(), 2);
        for (entry, name, start, end) in [
            (instructions[0], "mz", 0, 1_000),
            (instructions[1], "reset", 1_000, 2_000),
        ] {
            assert_eq!(entry["instruction"]["instruction_name"], name);
            assert_eq!(entry["instruction"]["parameters"][0]["QubitId"], 2);
            assert_eq!(entry["start_time_nanos"], start);
            assert_eq!(entry["end_time_nanos"], end);
        }
        Ok(())
    }
}
