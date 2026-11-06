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
        &self,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Profiler, QuantumBackend, QuantumBackends, TemporalProfiler, TemporalProfilerConfig,
        TranspilingAdapter,
    };
    use qsc_eval::backend::Backend;

    /// Checks exact decompositions on basis states and superpositions.
    #[test]
    fn matches_native_clifford_gates() -> Result<(), String> {
        for (name, parameters) in [
            ("cy", smallvec![QubitId(0), QubitId(1)]),
            ("cz", smallvec![QubitId(0), QubitId(1)]),
            ("sx", smallvec![QubitId(1)]),
        ] {
            for initial in 0..8 {
                let mut instructions = Vec::new();
                for qubit in 0..2 {
                    instructions.push(QuantumBackendInstruction {
                        instruction_name: if initial & (1 << qubit) == 0 {
                            "z"
                        } else {
                            "x"
                        },
                        parameters: smallvec![QubitId(qubit)],
                    });
                    if initial >= 4 {
                        instructions.push(QuantumBackendInstruction {
                            instruction_name: "h",
                            parameters: smallvec![QubitId(qubit)],
                        });
                    }
                }
                instructions.push(QuantumBackendInstruction {
                    instruction_name: name,
                    parameters: parameters.clone(),
                });
                let mut expected = QuantumBackends::sparse_sim();
                let mut actual = TranspilingAdapter::with_defaults(QuantumBackends::sparse_sim());
                expected.execute_batch(&instructions)?;
                actual.execute_batch(&instructions)?;
                let expected = expected.backend_mut().capture_quantum_state()?.0;
                let actual = actual
                    .get_backend_mut()
                    .backend_mut()
                    .capture_quantum_state()?
                    .0;
                assert_eq!(actual.len(), expected.len(), "{name}");
                for ((actual_index, actual), (expected_index, expected)) in
                    actual.iter().zip(&expected)
                {
                    assert_eq!(actual_index, expected_index);
                    assert!(
                        (actual - expected).norm() < 1e-12,
                        "{name}, input {initial}"
                    );
                }
            }
        }
        Ok(())
    }

    /// Profiles the decomposed gates instead of charging a single native Clifford step.
    #[test]
    fn profiles_decomposed_cliffords() -> Result<(), String> {
        for (name, parameters) in [
            ("cy", smallvec![QubitId(0), QubitId(1)]),
            ("cz", smallvec![QubitId(0), QubitId(1)]),
            ("sx", smallvec![QubitId(1)]),
        ] {
            let mut profiler =
                TranspilingAdapter::with_defaults(TemporalProfiler::new(TemporalProfilerConfig {
                    seed: Some(42),
                    ..TemporalProfilerConfig::default()
                }));
            profiler.execute_batch(&[QuantumBackendInstruction {
                instruction_name: name,
                parameters,
            }])?;
            assert!((profiler.get_result().totals.total_runtime_sec - 15_000e-9).abs() < 1e-15);
        }
        Ok(())
    }
}
