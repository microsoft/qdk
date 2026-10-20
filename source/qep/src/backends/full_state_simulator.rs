// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult,
    Parameter::{Float, QubitId, ResultId},
    QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
};
use qdk_simulators::{MeasurementResult, Simulator, cpu_full_state_simulator::FullStateSimulator};
use rustc_hash::FxHashMap;

/// Executes QIR instructions with a dense simulator and persistent result slots.
pub struct FullStateSimulatorBackend {
    pub simulator: FullStateSimulator,
    results: FxHashMap<usize, QuantumBackendResult>,
}

impl FullStateSimulatorBackend {
    /// Creates a dense simulator with a private scratch slot for all measurements.
    pub fn new(
        num_qubits: usize,
        seed: u32,
        noise: <FullStateSimulator as Simulator>::Noise,
    ) -> Self {
        Self {
            simulator: FullStateSimulator::new(num_qubits, 1, seed, noise),
            results: FxHashMap::default(),
        }
    }

    /// Measures into the scratch slot, preserving stored QIR results independently.
    fn measure(&mut self, target: usize, reset: bool) -> Result<QuantumBackendResult, String> {
        if reset {
            self.simulator.mresetz(target, 0);
        } else {
            self.simulator.mz(target, 0);
        }
        match self.simulator.measurements()[0] {
            MeasurementResult::Zero => Ok(QuantumBackendResult::Zero),
            MeasurementResult::One => Ok(QuantumBackendResult::One),
            MeasurementResult::Loss => Err(format!("Measurement of qubit {target} reported loss")),
        }
    }

    /// Implements ancilla-free Toffoli using seven T/T-adjoint gates and six CNOTs.
    fn ccx(&mut self, control0: usize, control1: usize, target: usize) -> Result<(), String> {
        use crate::DecomposeToffoliAdapter;
        // Reuse the shared decomposition rather than duplicating its gate sequence.
        let mut adapter = DecomposeToffoliAdapter::new(self);
        adapter.execute_batch(&[QuantumBackendInstruction {
            instruction_name: "ccx",
            parameters: smallvec::smallvec![QubitId(control0), QubitId(control1), QubitId(target)],
        }])?;
        Ok(())
    }
}

impl QuantumBackend for &mut FullStateSimulatorBackend {
    /// Forwards borrowed execution so the shared Toffoli adapter can wrap this backend.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        FullStateSimulatorBackend::execute_batch(self, instructions)
    }
}

impl QuantumBackend for FullStateSimulatorBackend {
    /// Executes supported gates and rejects unexpected instructions.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        let mut batch_result = BatchResult::default();
        for instruction in instructions {
            match (
                instruction.instruction_name,
                instruction.parameters.as_slice(),
            ) {
                ("ccx", &[QubitId(a), QubitId(b), QubitId(c)]) => self.ccx(a, b, c)?,
                ("x", &[QubitId(q)]) => self.simulator.x(q),
                ("y", &[QubitId(q)]) => self.simulator.y(q),
                ("z", &[QubitId(q)]) => self.simulator.z(q),
                ("h", &[QubitId(q)]) => self.simulator.h(q),
                ("s", &[QubitId(q)]) => self.simulator.s(q),
                ("sadj", &[QubitId(q)]) => self.simulator.s_adj(q),
                ("sx", &[QubitId(q)]) => self.simulator.sx(q),
                ("t", &[QubitId(q)]) => self.simulator.t(q),
                ("tadj", &[QubitId(q)]) => self.simulator.t_adj(q),
                ("cx", &[QubitId(control), QubitId(target)]) => self.simulator.cx(control, target),
                ("cy", &[QubitId(control), QubitId(target)]) => self.simulator.cy(control, target),
                ("cz", &[QubitId(control), QubitId(target)]) => self.simulator.cz(control, target),
                ("rx", &[QubitId(q), Float(angle)]) => self.simulator.rx(angle, q),
                ("ry", &[QubitId(q), Float(angle)]) => self.simulator.ry(angle, q),
                ("rz", &[QubitId(q), Float(angle)]) => self.simulator.rz(angle, q),
                ("rxx", &[QubitId(a), QubitId(b), Float(angle)]) => self.simulator.rxx(angle, a, b),
                ("ryy", &[QubitId(a), QubitId(b), Float(angle)]) => self.simulator.ryy(angle, a, b),
                ("rzz", &[QubitId(a), QubitId(b), Float(angle)]) => self.simulator.rzz(angle, a, b),
                ("swap", &[QubitId(a), QubitId(b)]) => self.simulator.swap(a, b),
                ("reset", &[QubitId(q)]) => self.simulator.resetz(q),
                ("mz_store", &[QubitId(q), ResultId(result)]) => {
                    let value = self.measure(q, false)?;
                    self.results.insert(result, value);
                }

                ("mz_store_reset", &[QubitId(q), ResultId(result)]) => {
                    let value = self.measure(q, true)?;
                    self.results.insert(result, value);
                }
                ("mz", &[QubitId(q)]) => batch_result.results.push(self.measure(q, false)?),
                ("mz_reset", &[QubitId(q)]) => batch_result.results.push(self.measure(q, true)?),
                ("read_result", &[ResultId(result)]) => {
                    let value = self
                        .results
                        .get(&result)
                        .copied()
                        .ok_or_else(|| format!("Result ID {result} has not been written"))?;
                    batch_result.results.push(value);
                }
                _ => {
                    return Err(format!(
                        "Unsupported full-state simulator instruction: {instruction:?}"
                    ));
                }
            }
        }
        Ok(batch_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SparseSimBackend;
    use num_bigint::BigUint;
    use num_complex::Complex64;
    use smallvec::smallvec;

    #[test]
    /// Compares every unitary instruction against the sparse simulator on a complex state.
    fn unitary_instructions_match_sparse_simulator() -> Result<(), String> {
        let mut dense = FullStateSimulatorBackend::new(3, 0, Default::default());
        let mut sparse = SparseSimBackend::default();
        let instructions = [
            ("h", smallvec![QubitId(0)]),
            ("h", smallvec![QubitId(1)]),
            ("h", smallvec![QubitId(2)]),
            ("t", smallvec![QubitId(0)]),
            ("s", smallvec![QubitId(1)]),
            ("rz", smallvec![QubitId(2), Float(0.37)]),
            ("ccx", smallvec![QubitId(0), QubitId(1), QubitId(2)]),
            ("cx", smallvec![QubitId(0), QubitId(1)]),
            ("cy", smallvec![QubitId(1), QubitId(2)]),
            ("cz", smallvec![QubitId(2), QubitId(0)]),
            ("rx", smallvec![QubitId(0), Float(0.31)]),
            ("ry", smallvec![QubitId(1), Float(-0.47)]),
            ("rxx", smallvec![QubitId(0), QubitId(2), Float(0.61)]),
            ("ryy", smallvec![QubitId(0), QubitId(1), Float(-0.29)]),
            ("rzz", smallvec![QubitId(1), QubitId(2), Float(0.83)]),
            ("sadj", smallvec![QubitId(0)]),
            ("sx", smallvec![QubitId(1)]),
            ("tadj", smallvec![QubitId(2)]),
            ("x", smallvec![QubitId(0)]),
            ("y", smallvec![QubitId(1)]),
            ("z", smallvec![QubitId(2)]),
            ("swap", smallvec![QubitId(0), QubitId(2)]),
        ];
        for (instruction_name, parameters) in instructions {
            let instruction = QuantumBackendInstruction {
                instruction_name,
                parameters,
            };
            dense.execute_batch(std::slice::from_ref(&instruction))?;
            sparse.execute_batch(std::slice::from_ref(&instruction))?;
            let expected = sparse.wavefunction();
            for (index, actual) in dense.simulator.state_dump().data().iter().enumerate() {
                let expected_amplitude = expected
                    .iter()
                    .find(|(basis, _)| *basis == BigUint::from(index))
                    .map_or(Complex64::new(0.0, 0.0), |(_, amplitude)| *amplitude);
                assert!(
                    (actual - expected_amplitude).norm() < 1e-12,
                    "{instruction_name}: amplitude {index}: {actual} != {expected_amplitude}"
                );
            }
        }
        Ok(())
    }

    #[test]
    /// Preserves result slots across batches and keeps immediate results in instruction order.
    fn measurements_and_resets() -> Result<(), String> {
        use QuantumBackendResult::{One, Zero};
        let mut backend = FullStateSimulatorBackend::new(1, 0, Default::default());
        for name in ["mz", "mz_reset"] {
            backend.execute_batch(&[QuantumBackendInstruction {
                instruction_name: name,
                parameters: smallvec![QubitId(0)],
            }])?;
            assert!(
                backend
                    .execute_batch(&[QuantumBackendInstruction {
                        instruction_name: "read_result",
                        parameters: smallvec![ResultId(0)],
                    }])
                    .is_err()
            );
        }
        let mut results = Vec::new();
        for (instruction_name, parameters) in [
            ("x", smallvec![QubitId(0)]),
            ("mz_store", smallvec![QubitId(0), ResultId(7)]),
            ("mz_reset", smallvec![QubitId(0)]),
            ("mz", smallvec![QubitId(0)]),
            ("read_result", smallvec![ResultId(7)]),
            ("x", smallvec![QubitId(0)]),
            ("mz_store_reset", smallvec![QubitId(0), ResultId(0)]),
            ("mz", smallvec![QubitId(0)]),
            ("read_result", smallvec![ResultId(0)]),
            ("mz_store", smallvec![QubitId(0), ResultId(7)]),
            ("read_result", smallvec![ResultId(7)]),
            ("x", smallvec![QubitId(0)]),
            ("reset", smallvec![QubitId(0)]),
            ("mz", smallvec![QubitId(0)]),
        ] {
            results.extend(
                backend
                    .execute_batch(&[QuantumBackendInstruction {
                        instruction_name,
                        parameters,
                    }])?
                    .results,
            );
        }
        assert_eq!(results, vec![One, Zero, One, Zero, One, Zero, Zero]);
        let result = backend.execute_batch(&[
            QuantumBackendInstruction {
                instruction_name: "x",
                parameters: smallvec![QubitId(0)],
            },
            QuantumBackendInstruction {
                instruction_name: "mz_reset",
                parameters: smallvec![QubitId(0)],
            },
            QuantumBackendInstruction {
                instruction_name: "mz",
                parameters: smallvec![QubitId(0)],
            },
            QuantumBackendInstruction {
                instruction_name: "read_result",
                parameters: smallvec![ResultId(0)],
            },
        ])?;
        assert_eq!(result.results.as_slice(), &[One, Zero, One]);
        Ok(())
    }

    #[test]
    /// Rejects unknown instructions and malformed operands explicitly.
    fn invalid_instructions_return_errors() {
        let mut backend = FullStateSimulatorBackend::new(1, 0, Default::default());
        for (instruction_name, parameters) in [
            ("unknown", smallvec![]),
            ("rx", smallvec![QubitId(0)]),
            ("mz_store", smallvec![QubitId(0), QubitId(0)]),
        ] {
            assert!(
                backend
                    .execute_batch(&[QuantumBackendInstruction {
                        instruction_name,
                        parameters,
                    }])
                    .is_err()
            );
        }
    }
}
