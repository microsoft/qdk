// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult,
    Parameter::{Float, QubitId, ResultId},
    QuantumBackend, QuantumBackendInstruction, QuantumBackendResult,
};
use num_bigint::BigUint;
use num_complex::Complex64;
use qdk_simulators::SparseStateSim;
use rustc_hash::FxHashMap;

/// A state-vector backend powered by the QDK sparse-state simulator.
#[derive(Default)]
pub struct SparseSimBackend {
    simulator: SparseStateSim,
    qubits: usize,
    results: FxHashMap<usize, bool>,
}

impl SparseSimBackend {
    /// Returns the wavefunction as sorted basis-state indices and amplitudes.
    #[must_use]
    pub fn wavefunction(&mut self) -> Vec<(BigUint, Complex64)> {
        self.simulator.get_state().0
    }

    /// Allocates simulator qubits through the requested static QIR ID.
    fn ensure_qubit(&mut self, id: usize) {
        while self.qubits <= id {
            let allocated = self.simulator.allocate();
            debug_assert_eq!(allocated, self.qubits);
            self.qubits += 1;
        }
    }

    /// Allocates simulator qubits for every requested static QIR ID.
    fn ensure_qubits(&mut self, ids: &[usize]) {
        if let Some(id) = ids.iter().max() {
            self.ensure_qubit(*id);
        }
    }

    /// Measures a qubit and stores its value in a QIR result slot.
    fn measure(&mut self, target: usize, result: usize) -> bool {
        let value = self.mz(target);
        self.results.insert(result, value);
        value
    }

    /// Measures a qubit without writing a result slot.
    fn mz(&mut self, target: usize) -> bool {
        self.ensure_qubit(target);
        self.simulator.measure(target)
    }

    /// Returns the pre-reset measurement without writing a result slot.
    fn mz_reset(&mut self, target: usize) -> bool {
        let value = self.mz(target);
        if value {
            self.simulator.x(target);
        }
        value
    }
}

impl SparseSimBackend {
    /// Applies a doubly controlled X gate.
    fn ccx(&mut self, control1: usize, control2: usize, target: usize) -> Result<(), String> {
        self.ensure_qubits(&[control1, control2, target]);
        self.simulator.mcx(&[control1, control2], target);
        Ok(())
    }

    /// Applies a controlled X gate.
    fn cx(&mut self, control: usize, target: usize) -> Result<(), String> {
        self.ensure_qubits(&[control, target]);
        self.simulator.mcx(&[control], target);
        Ok(())
    }

    /// Applies a controlled Y gate.
    fn cy(&mut self, control: usize, target: usize) -> Result<(), String> {
        self.ensure_qubits(&[control, target]);
        self.simulator.mcy(&[control], target);
        Ok(())
    }

    /// Applies a controlled Z gate.
    fn cz(&mut self, control: usize, target: usize) -> Result<(), String> {
        self.ensure_qubits(&[control, target]);
        self.simulator.mcz(&[control], target);
        Ok(())
    }

    /// Applies an X-axis rotation.
    fn rx(&mut self, angle: f64, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.rx(angle, target);
        Ok(())
    }

    /// Applies a two-qubit XX rotation.
    fn rxx(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String> {
        self.ensure_qubits(&[target1, target2]);
        self.simulator.h(target1);
        self.simulator.h(target2);
        self.simulator.mcx(&[target2], target1);
        self.simulator.rz(angle, target1);
        self.simulator.mcx(&[target2], target1);
        self.simulator.h(target2);
        self.simulator.h(target1);
        Ok(())
    }

    /// Applies a Y-axis rotation.
    fn ry(&mut self, angle: f64, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.ry(angle, target);
        Ok(())
    }

    /// Applies a two-qubit YY rotation.
    fn ryy(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String> {
        self.ensure_qubits(&[target1, target2]);
        self.simulator.h(target1);
        self.simulator.s(target1);
        self.simulator.h(target1);
        self.simulator.h(target2);
        self.simulator.s(target2);
        self.simulator.h(target2);
        self.simulator.mcx(&[target2], target1);
        self.simulator.rz(angle, target1);
        self.simulator.mcx(&[target2], target1);
        self.simulator.h(target2);
        self.simulator.sadj(target2);
        self.simulator.h(target2);
        self.simulator.h(target1);
        self.simulator.sadj(target1);
        self.simulator.h(target1);
        Ok(())
    }

    /// Applies a Z-axis rotation.
    fn rz(&mut self, angle: f64, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.rz(angle, target);
        Ok(())
    }

    /// Applies a two-qubit ZZ rotation.
    fn rzz(&mut self, angle: f64, target1: usize, target2: usize) -> Result<(), String> {
        self.ensure_qubits(&[target1, target2]);
        self.simulator.mcx(&[target2], target1);
        self.simulator.rz(angle, target1);
        self.simulator.mcx(&[target2], target1);
        Ok(())
    }

    /// Applies a Hadamard gate.
    fn h(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.h(target);
        Ok(())
    }

    /// Applies an S gate.
    fn s(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.s(target);
        Ok(())
    }

    /// Applies an adjoint S gate.
    fn sadj(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.sadj(target);
        Ok(())
    }

    /// Applies a square-root X gate.
    fn sx(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.h(target);
        self.simulator.s(target);
        self.simulator.h(target);
        Ok(())
    }

    /// Applies a T gate.
    fn t(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.t(target);
        Ok(())
    }

    /// Applies an adjoint T gate.
    fn tadj(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.tadj(target);
        Ok(())
    }

    /// Applies an X gate.
    fn x(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.x(target);
        Ok(())
    }

    /// Applies a Y gate.
    fn y(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.y(target);
        Ok(())
    }

    /// Applies a Z gate.
    fn z(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        self.simulator.z(target);
        Ok(())
    }

    /// Exchanges two qubit states.
    fn swap(&mut self, target1: usize, target2: usize) -> Result<(), String> {
        self.ensure_qubits(&[target1, target2]);
        self.simulator.swap_qubit_ids(target1, target2);
        Ok(())
    }

    /// Measures a qubit into the requested result slot.
    fn mz_store(&mut self, target: usize, result: usize) -> Result<(), String> {
        self.measure(target, result);
        Ok(())
    }

    /// Resets a qubit to the zero state.
    fn reset(&mut self, target: usize) -> Result<(), String> {
        self.mz_reset(target);
        Ok(())
    }

    /// Measures a qubit into a result slot and resets it to zero.
    fn mz_store_reset(&mut self, target: usize, result: usize) -> Result<(), String> {
        let value = self.mz_reset(target);
        self.results.insert(result, value);
        Ok(())
    }

    /// Returns the value stored in a previously written result slot.
    fn read_result(&mut self, result: usize) -> Result<bool, String> {
        self.results
            .get(&result)
            .copied()
            .ok_or_else(|| format!("Result ID {result} has not been written"))
    }
}

impl QuantumBackend for SparseSimBackend {
    /// Simulates QIR instructions and returns each requested measurement in order.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        let mut batch_result = BatchResult::default();
        for instruction in instructions {
            let result = match (
                instruction.instruction_name,
                instruction.parameters.as_slice(),
            ) {
                ("ccx", &[QubitId(a), QubitId(b), QubitId(c)]) => self.ccx(a, b, c),
                ("cx", &[QubitId(a), QubitId(b)]) => self.cx(a, b),
                ("cy", &[QubitId(a), QubitId(b)]) => self.cy(a, b),
                ("cz", &[QubitId(a), QubitId(b)]) => self.cz(a, b),
                ("rx", &[QubitId(target), Float(angle)]) => self.rx(angle, target),
                ("rxx", &[QubitId(a), QubitId(b), Float(angle)]) => self.rxx(angle, a, b),
                ("ry", &[QubitId(target), Float(angle)]) => self.ry(angle, target),
                ("ryy", &[QubitId(a), QubitId(b), Float(angle)]) => self.ryy(angle, a, b),
                ("rz", &[QubitId(target), Float(angle)]) => self.rz(angle, target),
                ("rzz", &[QubitId(a), QubitId(b), Float(angle)]) => self.rzz(angle, a, b),
                ("h", &[QubitId(target)]) => self.h(target),
                ("s", &[QubitId(target)]) => self.s(target),
                ("sadj", &[QubitId(target)]) => self.sadj(target),
                ("sx", &[QubitId(target)]) => self.sx(target),
                ("t", &[QubitId(target)]) => self.t(target),
                ("tadj", &[QubitId(target)]) => self.tadj(target),
                ("x", &[QubitId(target)]) => self.x(target),
                ("y", &[QubitId(target)]) => self.y(target),
                ("z", &[QubitId(target)]) => self.z(target),
                ("swap", &[QubitId(a), QubitId(b)]) => self.swap(a, b),
                ("reset", &[QubitId(target)]) => self.reset(target),
                ("mz_store", &[QubitId(target), ResultId(result)]) => self.mz_store(target, result),
                ("mz_store_reset", &[QubitId(target), ResultId(result)]) => {
                    self.mz_store_reset(target, result)
                }
                ("mz", &[QubitId(target)]) => {
                    batch_result
                        .results
                        .push(QuantumBackendResult::from(self.mz(target)));
                    Ok(())
                }
                ("mz_reset", &[QubitId(target)]) => {
                    batch_result
                        .results
                        .push(QuantumBackendResult::from(self.mz_reset(target)));
                    Ok(())
                }
                ("read_result", &[ResultId(result)]) => {
                    let value = self
                        .read_result(result)
                        .unwrap_or_else(|error| panic!("{error}"));
                    batch_result.results.push(QuantumBackendResult::from(value));
                    Ok(())
                }
                _ => panic!("Unsupported or malformed QIR instruction: {instruction:?}"),
            };
            result.unwrap_or_else(|error| panic!("{error}"));
        }
        Ok(batch_result)
    }
}
