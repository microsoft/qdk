// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This crate implements a stabilizer simulator for the QDK.

pub mod noise;
pub mod operation;

use crate::{
    MeasurementResult, NearlyZero, QubitID, Simulator,
    noise_config::{CumulativeNoiseConfig, IntrinsicID},
};
pub use noise::Fault;
use operation::Operation;
use paulimer::{
    Simulation, UnitaryOp,
    outcome_specific_simulation::{OutcomeSpecificSimulation, apply_hadamard},
    quantum_core,
};
use rand::{SeedableRng as _, rngs::StdRng};
use std::{
    f64::consts::{FRAC_PI_2, PI, TAU},
    sync::Arc,
};

/// A stabilizer simulator with the ability to simulate atom loss.
pub struct StabilizerSimulator {
    /// The noise configuration for the simulation.
    noise_config: Arc<CumulativeNoiseConfig<Fault>>,
    /// Random number generator used to sample from [`Self::noise_config`].
    rng: StdRng,
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

impl StabilizerSimulator {
    /// Sets the random seed of the simulator.
    pub fn set_seed(&mut self, seed: u64) {
        self.rng = StdRng::seed_from_u64(seed);
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
        for q in targets {
            self.reload_qubit(*q);
        }
    }

    /// Applies a list of gates to the system.
    pub fn apply_gates(&mut self, gates: &[Operation]) {
        for gate in gates {
            self.apply_gate_in_place(gate);
        }
    }

    fn apply_gate_in_place(&mut self, gate: &Operation) {
        match *gate {
            Operation::I { .. } => (),
            Operation::X { target } => self.x(target),
            Operation::Y { target } => self.y(target),
            Operation::Z { target } => self.z(target),
            Operation::H { target } => self.h(target),
            Operation::S { target } => self.s(target),
            Operation::SAdj { target } => self.s_adj(target),
            Operation::SX { target } => self.sx(target),
            Operation::CZ { control, target } => self.cz(control, target),
            Operation::Move { target } => self.mov(target),
            Operation::MResetZ { target, result_id } => self.mresetz(target, result_id),
        }
    }

    fn apply_idle_noise(&mut self, target: QubitID) {
        let idle_time = self.time - self.last_operation_time[target];
        self.last_operation_time[target] = self.time;
        let fault = self.noise_config.gen_idle_fault(&mut self.rng, idle_time);
        self.apply_fault(fault, &[target]);
    }

    fn apply_fault(&mut self, fault: Fault, targets: &[QubitID]) {
        match fault {
            Fault::None => (),
            Fault::Pauli(pauli_observables) => {
                let observable: Vec<_> = pauli_observables
                    .into_iter()
                    .zip(targets)
                    .filter(|(_, q)| !self.loss[**q]) // We don't apply faults on lost qubits.
                    .map(|(pauli, q)| (pauli, *q).into())
                    .collect();
                self.state.pauli(&observable);
            }
            Fault::S => {
                if !self.loss[targets[0]] {
                    self.state.apply_unitary(UnitaryOp::SqrtZ, targets);
                }
            }
            Fault::Loss => {
                for target in targets {
                    self.mresetz_impl(*target);
                    self.loss[*target] = true;
                }
            }
        }
    }

    /// Records a z-measurement on the given `target`.
    fn record_mz(&mut self, target: QubitID, result_id: QubitID) {
        let measurement = self.mz_impl(target);
        self.measurements[result_id] = measurement;
    }

    /// Records a z-measurement on the given `target` and resets the qubit to the zero state.
    fn record_mresetz(&mut self, target: QubitID, result_id: QubitID) {
        let measurement = self.mresetz_impl(target);
        self.measurements[result_id] = measurement;
    }

    /// Measures a Z observable on the given `target`.
    fn mz_impl(&mut self, target: QubitID) -> MeasurementResult {
        if self.loss[target] {
            self.loss[target] = false;
            return MeasurementResult::Loss;
        }

        self.state.measure(&[quantum_core::z(target)]);

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

    /// Measures a Z observable on the given `target` and reset the qubit to the zero state.
    fn mresetz_impl(&mut self, target: QubitID) -> MeasurementResult {
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
}

impl Simulator for StabilizerSimulator {
    type Noise = Arc<CumulativeNoiseConfig<Fault>>;
    type StateDumpData = paulimer::clifford::CliffordUnitary;

    fn new(num_qubits: usize, num_results: usize, seed: u32, noise_config: Self::Noise) -> Self {
        Self {
            noise_config,
            rng: StdRng::seed_from_u64(u64::from(seed)),
            state: OutcomeSpecificSimulation::new_with_random_outcomes(num_qubits, num_results),
            loss: vec![false; num_qubits],
            measurements: vec![MeasurementResult::Zero; num_results],
            last_operation_time: vec![0; num_qubits],
            time: 0,
        }
    }

    fn x(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::X, &[target]);
            let fault = self.noise_config.x.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn y(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::Y, &[target]);
            let fault = self.noise_config.y.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn z(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::Z, &[target]);
            let fault = self.noise_config.z.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn h(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            apply_hadamard(&mut self.state, target);
            let fault = self.noise_config.h.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn s(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::SqrtZ, &[target]);
            let fault = self.noise_config.s.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn s_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::SqrtZInv, &[target]);
            let fault = self.noise_config.s_adj.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn sx(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::SqrtX, &[target]);
            let fault = self.noise_config.sx.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn sx_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::SqrtXInv, &[target]);
            let fault = self.noise_config.sx_adj.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn cx(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state
                .apply_unitary(UnitaryOp::ControlledX, &[control, target]);
        }
        // We still apply operation faults to non-lost qubits.
        let fault = self.noise_config.cx.gen_operation_fault(&mut self.rng);
        self.apply_fault(fault, &[control, target]);
    }

    fn cy(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state.apply_unitary(UnitaryOp::SqrtZInv, &[target]);
            self.state
                .apply_unitary(UnitaryOp::ControlledX, &[control, target]);
            self.state.apply_unitary(UnitaryOp::SqrtZ, &[target]);
        }
        // We still apply operation faults to non-lost qubits.
        let fault = self.noise_config.cy.gen_operation_fault(&mut self.rng);
        self.apply_fault(fault, &[control, target]);
    }

    fn cz(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state
                .apply_unitary(UnitaryOp::ControlledZ, &[control, target]);
        }
        // We still apply operation faults to non-lost qubits.
        let fault = self.noise_config.cz.gen_operation_fault(&mut self.rng);
        self.apply_fault(fault, &[control, target]);
    }

    fn rx(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);

            // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
            // and check to see if it is supported.
            let unitary = unitary_from_normalized_angle(
                angle,
                UnitaryOp::X,
                UnitaryOp::SqrtX,
                UnitaryOp::SqrtXInv,
            );
            self.state.apply_unitary(unitary, &[target]);

            let fault = self.noise_config.rx.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn ry(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);

            // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
            // and check to see if it is supported.
            let unitary = unitary_from_normalized_angle(
                angle,
                UnitaryOp::Y,
                UnitaryOp::SqrtY,
                UnitaryOp::SqrtYInv,
            );
            self.state.apply_unitary(unitary, &[target]);

            let fault = self.noise_config.ry.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn rz(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);

            // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
            // and check to see if it is supported.
            let unitary = unitary_from_normalized_angle(
                angle,
                UnitaryOp::Z,
                UnitaryOp::SqrtZ,
                UnitaryOp::SqrtZInv,
            );
            self.state.apply_unitary(unitary, &[target]);

            let fault = self.noise_config.rz.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        if !self.loss[q1] && !self.loss[q2] {
            self.apply_idle_noise(q1);
            self.apply_idle_noise(q2);

            // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
            // and check to see if it is supported.
            let unitary = unitary_from_normalized_angle(
                angle,
                UnitaryOp::ControlledX,
                UnitaryOp::SqrtX,
                UnitaryOp::SqrtXInv,
            );
            // NOTE: We perform the Rxx gate by changing basis to Y and performing the decomposition of Rzz.
            self.state.apply_unitary(UnitaryOp::SqrtY, &[q1]);
            self.state.apply_unitary(UnitaryOp::SqrtY, &[q2]);
            self.state.apply_unitary(UnitaryOp::ControlledX, &[q2, q1]);
            self.state.apply_unitary(unitary, &[q1]);
            self.state.apply_unitary(UnitaryOp::ControlledX, &[q2, q1]);
            self.state.apply_unitary(UnitaryOp::SqrtYInv, &[q1]);
            self.state.apply_unitary(UnitaryOp::SqrtYInv, &[q2]);

            let fault = self.noise_config.rxx.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[q1, q2]);
        }
    }

    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        if !self.loss[q1] && !self.loss[q2] {
            self.apply_idle_noise(q1);
            self.apply_idle_noise(q2);

            // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
            // and check to see if it is supported.
            let unitary = unitary_from_normalized_angle(
                angle,
                UnitaryOp::ControlledZ,
                UnitaryOp::SqrtZ,
                UnitaryOp::SqrtZInv,
            );
            // NOTE: We perform the Ryy gate by changing basis to Z and performing the decomposition of Rzz.
            self.state.apply_unitary(UnitaryOp::SqrtX, &[q1]);
            self.state.apply_unitary(UnitaryOp::SqrtX, &[q2]);
            self.state.apply_unitary(UnitaryOp::ControlledX, &[q2, q1]);
            self.state.apply_unitary(unitary, &[q1]);
            self.state.apply_unitary(UnitaryOp::ControlledX, &[q2, q1]);
            self.state.apply_unitary(UnitaryOp::SqrtXInv, &[q1]);
            self.state.apply_unitary(UnitaryOp::SqrtXInv, &[q2]);

            let fault = self.noise_config.ryy.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[q1, q2]);
        }
    }

    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        if !self.loss[q1] && !self.loss[q2] {
            self.apply_idle_noise(q1);
            self.apply_idle_noise(q2);

            // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
            // and check to see if it is supported.
            let unitary = unitary_from_normalized_angle(
                angle,
                UnitaryOp::ControlledZ,
                UnitaryOp::SqrtZ,
                UnitaryOp::SqrtZInv,
            );
            self.state.apply_unitary(UnitaryOp::ControlledX, &[q2, q1]);
            self.state.apply_unitary(unitary, &[q1]);
            self.state.apply_unitary(UnitaryOp::ControlledX, &[q2, q1]);

            let fault = self.noise_config.rzz.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[q1, q2]);
        }
    }

    fn swap(&mut self, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) => {
                self.apply_idle_noise(q2);
                self.state.apply_permutation(&[1, 0], &[q1, q2]);
            }
            (false, true) => {
                self.apply_idle_noise(q1);
                self.state.apply_permutation(&[1, 0], &[q1, q2]);
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                self.state.apply_permutation(&[1, 0], &[q1, q2]);
            }
        }
        // There are three kinds of swaps:
        //   1. A logical swap, also called a relabel.
        //   2. A swap by physically exchanging the location of the qubits.
        //   3. An exchange of information by doing three CX.
        //
        // This method is concerned with the kinds (1) and (2), since (3)
        // gets decomposed into other instructions before making it to the simulator.
        // In both (1) and (2), the loss state of the qubits gets exchanged.
        self.loss.swap(q1, q2);

        // Is up to the user if swap is a virtual operation or not.
        // If they don't specify noise/loss probability for swap, then it is virtual.
        let fault = self.noise_config.swap.gen_operation_fault(&mut self.rng);
        self.apply_fault(fault, &[q1, q2]);
    }

    fn mz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_idle_noise(target);
        self.record_mz(target, result_id);
        let fault = self.noise_config.mz.gen_operation_fault(&mut self.rng);
        self.apply_fault(fault, &[target]);
    }

    fn mresetz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_idle_noise(target);
        self.record_mresetz(target, result_id);
        let fault = self.noise_config.mresetz.gen_operation_fault(&mut self.rng);
        self.apply_fault(fault, &[target]);
    }

    fn resetz(&mut self, target: QubitID) {
        self.apply_idle_noise(target);
        self.mresetz_impl(target);
        let fault = self.noise_config.mresetz.gen_operation_fault(&mut self.rng);
        self.apply_fault(fault, &[target]);
    }

    fn mov(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            let fault = self.noise_config.mov.gen_operation_fault(&mut self.rng);
            self.apply_fault(fault, &[target]);
        }
    }

    fn correlated_noise_intrinsic(&mut self, intrinsic_id: IntrinsicID, targets: &[usize]) {
        let fault = match self.noise_config.intrinsics.get(&intrinsic_id) {
            Some(correlated_noise) => correlated_noise.sample(&mut self.rng),
            None => return,
        };
        self.apply_fault(fault, targets);
    }

    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }

    fn t(&mut self, _target: QubitID) {
        unimplemented!("unssuported instruction in stabilizer simulator: T")
    }

    fn t_adj(&mut self, _target: QubitID) {
        unimplemented!("unssuported instruction in stabilizer simulator: T_ADJ")
    }

    fn state_dump(&self) -> &Self::StateDumpData {
        self.state.clifford()
    }
}

fn unitary_from_normalized_angle(
    angle: f64,
    pauli: UnitaryOp,
    sqrt_pauli: UnitaryOp,
    sqrt_pauli_inv: UnitaryOp,
) -> UnitaryOp {
    let mut normalized_angle = angle % TAU;
    if normalized_angle < 0.0 {
        normalized_angle += TAU;
    }
    if normalized_angle.is_nearly_zero() || (normalized_angle - TAU / 2.0).is_nearly_zero() {
        // The angle is a multiple of 2 * PI, so the operation is effectively an identity.
        UnitaryOp::I
    } else if (normalized_angle - PI).is_nearly_zero() {
        // The angle is an odd multiple of PI, so the operation is effectively a Pauli gate.
        pauli
    } else if (normalized_angle - FRAC_PI_2).is_nearly_zero() {
        // The angle is an odd multiple of PI / 2, so the operation is effectively a sqrt(Pauli) gate.
        sqrt_pauli
    } else if (normalized_angle - 3.0 * FRAC_PI_2).is_nearly_zero() {
        // The angle is an odd multiple of 3 * PI / 2, so the operation is effectively a sqrt(Pauli) adjoint gate.
        sqrt_pauli_inv
    } else {
        unimplemented!("unsupported rotation angle in stabilizer simulator: {angle}");
    }
}
