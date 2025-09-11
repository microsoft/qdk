//! This crate implements a stabilizer simulator for the QDK.

#![allow(dead_code)]

pub mod noise_config;
pub mod operation;

use noise_config::{CumulativeNoiseConfig, NoiseConfig};
use operation::Operation;
use paulimer::{
    outcome_specific_simulation::{apply_hadamard, OutcomeSpecificSimulation},
    quantum_core, Simulation, UnitaryOp,
};
use std::fmt::Write;

pub enum Fault {
    /// No fault occurred.
    None,
    /// A Pauli-X fault.
    X,
    /// A Pauli-Y fault.
    Y,
    /// A Pauli-Z fault.
    Z,
    /// A gradual dephasing fault. Qubits are always slowly
    /// rotating along the Z-axis with an unknown rate,
    /// eventually resulting in an `S` gate.
    S,
    /// The qubit was lost.
    Loss,
}

/// A qubit ID.
pub type QubitID = usize;

/// The result of a mesasurement in the Z-basis.
#[derive(Debug)]
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
    ///
    /// TODO: When does the loss flag gets unset?
    loss: Vec<bool>,
    /// Measurement results.
    measurements: Vec<MeasurementResult>,
}

impl Simulator {
    /// Creates a new Simulator with `num_qubits` qubits.
    pub fn new(num_qubits: usize, noise_config: NoiseConfig) -> Self {
        Self {
            noise_config: noise_config.into(),
            state: OutcomeSpecificSimulation::new_with_random_outcomes(
                num_qubits,
                num_qubits * 2,
                num_qubits * 2,
            ),
            loss: vec![false; num_qubits],
            measurements: Vec::with_capacity(2048),
        }
    }

    /// Applies a gate to the system.
    pub fn apply_gate(&mut self, gate: &Operation) {
        self.apply_gate_in_place(gate);
        // self.apply_noise(gate);
    }

    /// Applies a list of gates to the system.
    pub fn apply_gates(&mut self, gates: &[Operation]) {
        gates.iter().for_each(|gate| self.apply_gate(gate));
    }

    fn apply_gate_in_place(&mut self, gate: &Operation) {
        match *gate {
            Operation::I { .. } => (),
            Operation::X { target } => {
                if !self.loss[target] {
                    self.state.apply_unitary(UnitaryOp::X, &[target]);
                }
            }
            Operation::Y { target } => {
                if !self.loss[target] {
                    self.state.apply_unitary(UnitaryOp::Y, &[target]);
                }
            }
            Operation::Z { target } => {
                if !self.loss[target] {
                    self.state.apply_unitary(UnitaryOp::Z, &[target]);
                }
            }
            Operation::H { target } => {
                if !self.loss[target] {
                    apply_hadamard(&mut self.state, target);
                }
            }
            Operation::S { target } => {
                if !self.loss[target] {
                    self.state.apply_unitary(UnitaryOp::SqrtZ, &[target]);
                }
            }
            Operation::CZ { control, target } => {
                if !self.loss[control] && !self.loss[target] {
                    self.state
                        .apply_unitary(UnitaryOp::ControlledZ, &[control, target]);
                }
            }
            Operation::Move { .. } => (),
            Operation::MResetZ { target } => {
                self.record_z_measurement(target);
            }
        }
    }

    fn apply_noise(&mut self, gate: &Operation) {
        match *gate {
            Operation::I { target } => {
                self.apply_fault(self.noise_config.id.gen_operation_fault(), target)
            }
            Operation::X { target } => {
                self.apply_fault(self.noise_config.x.gen_operation_fault(), target)
            }
            Operation::Y { target } => {
                self.apply_fault(self.noise_config.y.gen_operation_fault(), target)
            }
            Operation::Z { target } => {
                self.apply_fault(self.noise_config.z.gen_operation_fault(), target)
            }
            Operation::H { target } => {
                self.apply_fault(self.noise_config.h.gen_operation_fault(), target)
            }
            Operation::S { target } => {
                self.apply_fault(self.noise_config.s.gen_operation_fault(), target)
            }
            Operation::CZ { control, target } => {
                self.apply_fault(self.noise_config.cz.gen_operation_fault(), control);
                self.apply_fault(self.noise_config.cz.gen_operation_fault(), target);
            }
            Operation::Move { target } => {
                self.apply_fault(self.noise_config.move_.gen_operation_fault(), target)
            }
            Operation::MResetZ { target } => {
                self.apply_fault(self.noise_config.mresetz.gen_operation_fault(), target)
            }
        }
    }

    fn apply_fault(&mut self, fault: Fault, target: QubitID) {
        match fault {
            Fault::None => (),
            Fault::X => self.apply_gate_in_place(&Operation::X { target }),
            Fault::Y => self.apply_gate_in_place(&Operation::Y { target }),
            Fault::Z => self.apply_gate_in_place(&Operation::Z { target }),
            Fault::S => self.apply_gate_in_place(&Operation::S { target }),
            Fault::Loss => {
                self.measure_z(target);
                self.loss[target] = true;
            }
        }
    }

    /// Records a z-measurement on the given `target`.
    fn record_z_measurement(&mut self, target: QubitID) {
        let measurement = self.measure_z(target);
        self.measurements.push(measurement);
    }

    /// Measures a Z observable on the given `target`.
    fn measure_z(&mut self, target: QubitID) -> MeasurementResult {
        if self.loss[target] {
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
    pub fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    pub fn measurements_str(&self) -> String {
        let mut buffer = String::new();
        for m in &self.measurements {
            match m {
                crate::MeasurementResult::Zero => {
                    write!(&mut buffer, "0").expect("write should succeed")
                }
                crate::MeasurementResult::One => {
                    write!(&mut buffer, "1").expect("write should succeed")
                }
                crate::MeasurementResult::Loss => {
                    write!(&mut buffer, "L").expect("write should succeed")
                }
            }
        }
        buffer
    }
}
