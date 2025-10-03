// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This crate implements a stabilizer simulator for the QDK.

pub mod operation;

use operation::Operation;
use paulimer::{
    Simulation, UnitaryOp,
    outcome_specific_simulation::{OutcomeSpecificSimulation, apply_hadamard},
    quantum_core,
};
use std::fmt::Write;

use crate::noise_config::{CumulativeNoiseConfig, Fault, NoiseConfig};

/// A qubit ID.
pub type QubitID = usize;

/// The result of a mesasurement in the Z-basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementResult {
    Zero,
    One,
    Loss,
}

/// A stabilizer simulator with the ability to simulate atom loss.
pub struct Simulator {
    /// The noise configuration for the simulation.
    noise_config: CumulativeNoiseConfig,
    /// The current inverse state of the simulation.
    state: OutcomeSpecificSimulation,
    /// A vector storing whether a qubit was lost or not.
    loss: Vec<bool>,
    /// Measurement results.
    measurements: Vec<MeasurementResult>,
    /// The last time each qubit was operated upon.
    last_operation_time: Vec<u32>,
    /// Current simulation time.
    time: u32,
}

impl Simulator {
    /// Creates a new Simulator with `num_qubits` qubits.
    #[must_use]
    pub fn new(num_qubits: usize, num_results: usize, noise_config: NoiseConfig) -> Self {
        Self {
            noise_config: noise_config.into(),
            state: OutcomeSpecificSimulation::new_with_random_outcomes(num_qubits, num_results),
            loss: vec![false; num_qubits],
            measurements: vec![MeasurementResult::Zero; num_results],
            last_operation_time: vec![0; num_qubits],
            time: 0,
        }
    }

    /// Increment the simulation time by one.
    /// This is used to compute the idle noise on qubits.
    pub fn step(&mut self) {
        self.time += 1;
    }

    /// Increment the simulation time by `steps`.
    /// This is used to compute the idle noise on qubits.
    pub fn steps(&mut self, steps: u32) {
        self.time += steps;
    }

    /// Reload a qubit.
    pub fn reload_qubit(&mut self, target: QubitID) {
        self.loss[target] = false;
    }

    /// Reload a list of qubits.
    pub fn reload_qubits(&mut self, targets: &[QubitID]) {
        targets.iter().for_each(|q| self.reload_qubit(*q));
    }

    /// Single qubit X gate.
    pub fn x(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::X { target });
    }

    /// Single qubit X gate.
    pub fn y(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::Y { target });
    }

    /// Single qubit Z gate.
    pub fn z(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::Z { target });
    }

    /// Single qubit H gate.
    pub fn h(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::H { target });
    }

    /// Single qubit S gate.
    pub fn s(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::S { target });
    }

    /// Single qubit S adjoint gate.
    pub fn s_adj(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::SAdj { target });
    }

    /// Single qubit SX gate.
    pub fn sx(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::SX { target });
    }

    /// Controlled-Z gate.
    pub fn cz(&mut self, control: QubitID, target: QubitID) {
        self.apply_gate_in_place(&Operation::CZ { control, target });
    }

    /// `MResetZ` operation.
    pub fn mresetz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_gate_in_place(&Operation::MResetZ { target, result_id });
    }

    /// Move operation. The purpose of this operation is modeling
    /// the noise coming from qubit movement in neutral atom machines.
    pub fn mov(&mut self, target: QubitID) {
        self.apply_gate_in_place(&Operation::Move { target });
    }

    /// Applies a gate to the system.
    pub fn apply_gate(&mut self, gate: &Operation) {
        self.apply_gate_in_place(gate);
    }

    /// Applies a list of gates to the system.
    pub fn apply_gates(&mut self, gates: &[Operation]) {
        gates.iter().for_each(|gate| self.apply_gate_in_place(gate));
    }

    fn apply_gate_in_place(&mut self, gate: &Operation) {
        match *gate {
            Operation::I { .. } => (),
            Operation::X { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    self.state.apply_unitary(UnitaryOp::X, &[target]);
                    self.apply_fault(self.noise_config.x.gen_operation_fault(), target);
                }
            }
            Operation::Y { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    self.state.apply_unitary(UnitaryOp::Y, &[target]);
                    self.apply_fault(self.noise_config.y.gen_operation_fault(), target);
                }
            }
            Operation::Z { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    self.state.apply_unitary(UnitaryOp::Z, &[target]);
                    self.apply_fault(self.noise_config.z.gen_operation_fault(), target);
                }
            }
            Operation::H { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    apply_hadamard(&mut self.state, target);
                    self.apply_fault(self.noise_config.h.gen_operation_fault(), target);
                }
            }
            Operation::S { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    self.state.apply_unitary(UnitaryOp::SqrtZ, &[target]);
                    self.apply_fault(self.noise_config.s.gen_operation_fault(), target);
                }
            }
            Operation::SX { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    self.state.apply_unitary(UnitaryOp::SqrtX, &[target]);
                    self.apply_fault(self.noise_config.sx.gen_operation_fault(), target);
                }
            }
            Operation::SAdj { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    self.state.apply_unitary(UnitaryOp::SqrtZInv, &[target]);
                    self.apply_fault(self.noise_config.s_adj.gen_operation_fault(), target);
                }
            }
            Operation::CZ { control, target } => {
                if !self.loss[control] && !self.loss[target] {
                    self.apply_idle_noise(control);
                    self.apply_idle_noise(target);
                    self.state
                        .apply_unitary(UnitaryOp::ControlledZ, &[control, target]);
                    self.apply_fault(self.noise_config.cz.gen_operation_fault(), control);
                    self.apply_fault(self.noise_config.cz.gen_operation_fault(), target);
                }
            }
            Operation::Move { target } => {
                if !self.loss[target] {
                    self.apply_idle_noise(target);
                    self.apply_fault(self.noise_config.mov.gen_operation_fault(), target);
                }
            }
            Operation::MResetZ { target, result_id } => {
                self.apply_idle_noise(target);
                self.record_z_measurement(target, result_id);
                self.apply_fault(self.noise_config.mresetz.gen_operation_fault(), target);
            }
        }
    }

    fn apply_idle_noise(&mut self, target: QubitID) {
        let idle_time = self.time - self.last_operation_time[target];
        self.last_operation_time[target] = self.time;
        let fault = self.noise_config.gen_idle_fault(idle_time);
        self.apply_fault(fault, target);
    }

    fn apply_fault(&mut self, fault: Fault, target: QubitID) {
        match fault {
            Fault::None => (),
            Fault::X => self.state.apply_unitary(UnitaryOp::X, &[target]),
            Fault::Y => self.state.apply_unitary(UnitaryOp::Y, &[target]),
            Fault::Z => self.state.apply_unitary(UnitaryOp::Z, &[target]),
            Fault::S => self.state.apply_unitary(UnitaryOp::SqrtZ, &[target]),
            Fault::Loss => {
                self.measure_z(target);
                self.loss[target] = true;
            }
        }
    }

    /// Records a z-measurement on the given `target`.
    fn record_z_measurement(&mut self, target: QubitID, result_id: QubitID) {
        let measurement = self.measure_z(target);
        self.measurements[result_id] = measurement;
    }

    /// Measures a Z observable on the given `target`.
    fn measure_z(&mut self, target: QubitID) -> MeasurementResult {
        if self.loss[target] {
            self.loss[target] = false;
            return MeasurementResult::Loss;
        }

        let r = self.state.measure(&[quantum_core::z(target)]);
        self.state
            .conditional_pauli(&[quantum_core::x(target)], &[r], true);

        if *self
            .state
            .outcome_vector()
            .last()
            .expect("there should be at least one measurement")
        {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        }
    }

    /// Returns a list of the measurements recorded during the simulation.
    #[must_use]
    pub fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    pub fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }

    /// Returns a string of 0s, 1s, and Ls representing the |0⟩, |1⟩, and Loss
    /// results during the simulation.
    #[must_use]
    pub fn measurements_str(&self) -> String {
        let mut buffer = String::new();
        for m in &self.measurements {
            match m {
                MeasurementResult::Zero => write!(&mut buffer, "0").expect("write should succeed"),
                MeasurementResult::One => write!(&mut buffer, "1").expect("write should succeed"),
                MeasurementResult::Loss => write!(&mut buffer, "L").expect("write should succeed"),
            }
        }
        buffer
    }
}
