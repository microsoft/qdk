// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::QirBackend;
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
        self.ensure_qubit(target);
        let value = self.simulator.measure(target);
        self.results.insert(result, value);
        value
    }
}

impl QirBackend for SparseSimBackend {
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
    fn mz(&mut self, target: usize, result: usize) -> Result<(), String> {
        self.measure(target, result);
        Ok(())
    }

    /// Resets a qubit to the zero state.
    fn reset(&mut self, target: usize) -> Result<(), String> {
        self.ensure_qubit(target);
        if self.simulator.measure(target) {
            self.simulator.x(target);
        }
        Ok(())
    }

    /// Measures a qubit into a result slot and resets it to zero.
    fn mresetz(&mut self, target: usize, result: usize) -> Result<(), String> {
        if self.measure(target, result) {
            self.simulator.x(target);
        }
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
