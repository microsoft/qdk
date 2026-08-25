// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This crate implements a stabilizer simulator for the QDK.

pub mod branching_state;
pub mod operation;

use crate::{
    MeasurementResult, NearlyZero, QubitID, Simulator,
    noise_config::{CumulativeNoiseConfig, Fault, FaultTerm, IntrinsicID, LossPolicy},
};
use branching_state::BranchingState;
use operation::Operation;
use paulimer::{PauliObservable, UnitaryOp};
use rand::{RngExt as _, SeedableRng as _, rngs::StdRng};
use std::{
    f64::consts::{FRAC_PI_2, PI, TAU},
    sync::Arc,
};

fn seeded_randomness(seed: u64) -> (StdRng, StdRng) {
    let mut seed_rng = StdRng::seed_from_u64(seed);
    (
        StdRng::from_rng(&mut seed_rng),
        StdRng::from_rng(&mut seed_rng),
    )
}

/// A stabilizer simulator with the ability to simulate atom loss.
pub struct StabilizerSimulator {
    /// The noise configuration for the simulation.
    noise_config: Arc<CumulativeNoiseConfig>,
    /// Random number generator used to sample from [`Self::noise_config`].
    rng: StdRng,
    /// Random number generator used to sample measurement outcomes.
    measurement_rng: StdRng,
    /// The current state of the simulation.
    state: BranchingState,
    /// A vector storing whether a qubit was lost or not.
    loss: Vec<bool>,
    /// Measurement results.
    measurements: Vec<MeasurementResult>,
    /// The last time each qubit was operated upon.
    last_operation_time: Vec<u32>,
    /// Current simulation time.
    time: u32,
}

/// Design decision: Why is this a macro?
///   Rust doesn't allow taking a mutable reference and an inmutable
///   reference to `self` at the same time. So, the obvious way express
///   this,
///   ```ignore
///   fn apply_noise(&mut self, noise_table: &CumulativeNoiseTable, targets: &[QubitID]) {
///       for target in targets {
///           if matches!(noise_table.sample_noise(&mut self.rng), Fault::Loss) {
///               ...
///           }
///       }
///   }
///   ```
///   and then doing,
///   ```ignore
///   self.apply_noise(&self.noise_config.rxx, targets)
///   ```
///   is not valid rust.
///
///   There are two alternatives. The first one is cloning the Arc
///   containing the noise config before each call to `apply_loss`. In,
///   that way rust doesn't see the cloned Arc as attached to self anymore.
///   ```ignore
///   let noise_config = Arc::clone(&self.noise_config);
///   self.apply_noise(&noise_config.rxx, targets);
///   ```
///   However, this is not ideal. We don't want to be increasing and decreasing
///   the reference count of an Arc in the hot-loop of the simulation.
///
///   The other alternative is creating a function that takes all the necessary
///   members of self as inputs independently,
///   ```ignore
///   fn apply_noise(
///     state: &mut StateType,
///     noise_table: &CumulativeNoiseTable,
///     targets: &[QubitID],
///     rng: &mut Rng,
///     loss: &mut Vec<bool>
///   ) {
///       for target in targets {
///           if matches!(noise_table.sample_loss(rng), Fault::Loss) {
///               // Since we don't have access to `self`
///               // we would need a re-implemplementation of
///               // self.mresetz(...) impl here.
///               loss[*target] = true;
///           }
///       }
///   }
///   ```
///   However, this is not very elegant. We would even need to re-implement mresetz.
///
///   The remaining alternative is using a macro.
macro_rules! apply_noise {
    ($slf:expr, $noise_table:ident, $targets:expr) => {{
        let fault = $slf.noise_config.$noise_table.sample_noise(&mut $slf.rng);
        if let Some(fault) = fault {
            $slf.apply_fault(&fault, $targets);
        }
    }};
}

impl StabilizerSimulator {
    /// Sets the random seed of the simulator.
    pub fn set_seed(&mut self, seed: u64) {
        let (noise_rng, measurement_rng) = seeded_randomness(seed);
        self.rng = noise_rng;
        self.measurement_rng = measurement_rng;
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

    /// Forces the state of a qubit to collapse to a specific value.
    pub fn post_select_z(&mut self, result: bool, target: QubitID) -> Result<(), String> {
        let mut projected = self.state.clone();
        let probability = projected.project(&[paulimer::core::z(target)].into(), result);
        if probability.is_nearly_zero() {
            Err("post-selection condition has zero probability".to_string())
        } else {
            self.state = projected;
            Ok(())
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
        let idle_fault = self.noise_config.gen_idle_fault(&mut self.rng, idle_time);
        if idle_fault && !self.loss[target] {
            self.state.unitary_op(UnitaryOp::SqrtZ, &[target]);
        }
    }

    fn apply_fault(&mut self, fault: &Fault, targets: &[QubitID]) {
        let observable: Vec<_> = fault
            .0
            .iter()
            .zip(targets)
            .filter(|(term, q)| {
                if self.loss[**q] {
                    return false;
                }
                match term {
                    FaultTerm::I => false,
                    FaultTerm::X | FaultTerm::Y | FaultTerm::Z => true,
                    FaultTerm::Loss => {
                        self.mresetz_impl(**q);
                        self.loss[**q] = true;
                        false
                    }
                }
            })
            .map(|(term, q)| match term {
                FaultTerm::X => (PauliObservable::PlusX, *q).into(),
                FaultTerm::Y => (PauliObservable::PlusY, *q).into(),
                FaultTerm::Z => (PauliObservable::PlusZ, *q).into(),
                FaultTerm::I | FaultTerm::Loss => unreachable!("these terms were filtered"),
            })
            .collect();
        self.state.pauli(&observable.into());
    }

    /// Applies an `S` adjoint to the given target
    /// Used by the [`LossPolicy::ResidualSDagger`] behavior.
    fn residual_s_dagger(&mut self, target: QubitID) {
        self.apply_idle_noise(target);
        self.state.unitary_op(UnitaryOp::SqrtZInv, &[target]);
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

        let observable = [paulimer::core::z(target)].into();
        let one_probability = self.state.outcome_probability(&observable, true);
        // Snap numerical residue so deterministic measurements do not consume randomness.
        let outcome = if one_probability.is_nearly_zero() {
            false
        } else if (1.0 - one_probability).is_nearly_zero() {
            true
        } else {
            self.measurement_rng
                .random_bool(one_probability.clamp(0.0, 1.0))
        };
        if outcome {
            let probability = self.state.project(&observable, true);
            assert!(
                !probability.is_nearly_zero(),
                "sampled a zero-probability measurement outcome"
            );
            MeasurementResult::One
        } else {
            let zero_probability = self.state.project(&observable, false);
            assert!(
                !zero_probability.is_nearly_zero(),
                "sampled a zero-probability measurement outcome"
            );
            MeasurementResult::Zero
        }
    }

    /// Measures a Z observable on the given `target` and reset the qubit to the zero state.
    fn mresetz_impl(&mut self, target: QubitID) -> MeasurementResult {
        if self.loss[target] {
            self.loss[target] = false;
            return MeasurementResult::Loss;
        }

        let result = self.mz_impl(target);
        if result == MeasurementResult::One {
            self.state.pauli(&[paulimer::core::x(target)].into());
        }
        result
    }

    fn loss_impl(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.mresetz_impl(target);
            self.loss[target] = true;
        }
    }
}

impl Simulator for StabilizerSimulator {
    type Noise = Arc<CumulativeNoiseConfig>;
    type StateDumpData = BranchingState;

    fn new(num_qubits: usize, num_results: usize, seed: u32, noise_config: Self::Noise) -> Self {
        let (rng, measurement_rng) = seeded_randomness(u64::from(seed));
        Self {
            noise_config,
            rng,
            measurement_rng,
            state: BranchingState::new(num_qubits),
            loss: vec![false; num_qubits],
            measurements: vec![MeasurementResult::Zero; num_results],
            last_operation_time: vec![0; num_qubits],
            time: 0,
        }
    }

    fn x(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::X, &[target]);
            apply_noise!(self, x, &[target]);
        }
    }

    fn y(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::Y, &[target]);
            apply_noise!(self, y, &[target]);
        }
    }

    fn z(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::Z, &[target]);
            apply_noise!(self, z, &[target]);
        }
    }

    fn h(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::Hadamard, &[target]);
            apply_noise!(self, h, &[target]);
        }
    }

    fn s(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::SqrtZ, &[target]);
            apply_noise!(self, s, &[target]);
        }
    }

    fn s_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::SqrtZInv, &[target]);
            apply_noise!(self, s_adj, &[target]);
        }
    }

    fn sx(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::SqrtX, &[target]);
            apply_noise!(self, sx, &[target]);
        }
    }

    fn sx_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(UnitaryOp::SqrtXInv, &[target]);
            apply_noise!(self, sx_adj, &[target]);
        }
    }

    fn cx(&mut self, control: QubitID, target: QubitID) {
        match (self.loss[control], self.loss[target]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[control] { target } else { control };
                self.apply_idle_noise(remaining_qubit);
                match self.noise_config.cx.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::Degrade | LossPolicy::ApplyAnyway => unreachable!(
                        "the `cx` gate does not support the Degrade or ApplyAnyway loss policies"
                    ),
                }
            }
            (false, false) => {
                self.apply_idle_noise(control);
                self.apply_idle_noise(target);
                self.state
                    .unitary_op(UnitaryOp::ControlledX, &[control, target]);
            }
        }
        // We still apply operation faults to non-lost qubits.
        apply_noise!(self, cx, &[control, target]);
    }

    fn cy(&mut self, control: QubitID, target: QubitID) {
        match (self.loss[control], self.loss[target]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[control] { target } else { control };
                self.apply_idle_noise(remaining_qubit);
                match self.noise_config.cy.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::Degrade | LossPolicy::ApplyAnyway => unreachable!(
                        "the `cy` gate does not support the Degrade or ApplyAnyway loss policies"
                    ),
                }
            }
            (false, false) => {
                self.apply_idle_noise(control);
                self.apply_idle_noise(target);
                self.state.unitary_op(UnitaryOp::SqrtZInv, &[target]);
                self.state
                    .unitary_op(UnitaryOp::ControlledX, &[control, target]);
                self.state.unitary_op(UnitaryOp::SqrtZ, &[target]);
            }
        }
        // We still apply operation faults to non-lost qubits.
        apply_noise!(self, cy, &[control, target]);
    }

    fn cz(&mut self, control: QubitID, target: QubitID) {
        match (self.loss[control], self.loss[target]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[control] { target } else { control };
                self.apply_idle_noise(remaining_qubit);
                match self.noise_config.cz.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::Degrade | LossPolicy::ApplyAnyway => unreachable!(
                        "the `cz` gate does not support the Degrade or ApplyAnyway loss policies"
                    ),
                }
            }
            (false, false) => {
                self.apply_idle_noise(control);
                self.apply_idle_noise(target);
                self.state
                    .unitary_op(UnitaryOp::ControlledZ, &[control, target]);
            }
        }
        // We still apply operation faults to non-lost qubits.
        apply_noise!(self, cz, &[control, target]);
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
            if let Some(unitary) = unitary {
                self.state.unitary_op(unitary, &[target]);
            } else {
                self.state
                    .rotate(angle, &[paulimer::core::x(target)].into());
            }

            apply_noise!(self, rx, &[target]);
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
            if let Some(unitary) = unitary {
                self.state.unitary_op(unitary, &[target]);
            } else {
                self.state
                    .rotate(angle, &[paulimer::core::y(target)].into());
            }

            apply_noise!(self, ry, &[target]);
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
            if let Some(unitary) = unitary {
                self.state.unitary_op(unitary, &[target]);
            } else {
                self.state
                    .rotate(angle, &[paulimer::core::z(target)].into());
            }

            apply_noise!(self, rz, &[target]);
        }
    }

    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[q1] { q2 } else { q1 };
                self.apply_idle_noise(remaining_qubit);
                match self.noise_config.rxx.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Degrade => return self.rx(angle, remaining_qubit),
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::ApplyAnyway => {
                        unreachable!("the `rxx` gate does not support the ApplyAnyway loss policy")
                    }
                }
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);

                // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
                // and check to see if it is supported.
                let unitary = unitary_from_normalized_angle(
                    angle,
                    UnitaryOp::Z,
                    UnitaryOp::SqrtZ,
                    UnitaryOp::SqrtZInv,
                );
                if let Some(unitary) = unitary {
                    // Perform Rxx by changing basis to Z and using the Rzz decomposition.
                    self.state.unitary_op(UnitaryOp::Hadamard, &[q1]);
                    self.state.unitary_op(UnitaryOp::Hadamard, &[q2]);
                    self.state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                    self.state.unitary_op(unitary, &[q1]);
                    self.state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                    self.state.unitary_op(UnitaryOp::Hadamard, &[q1]);
                    self.state.unitary_op(UnitaryOp::Hadamard, &[q2]);
                } else {
                    self.state.rotate(
                        angle,
                        &[paulimer::core::x(q1), paulimer::core::x(q2)].into(),
                    );
                }
            }
        }
        apply_noise!(self, rxx, &[q1, q2]);
    }

    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[q1] { q2 } else { q1 };
                self.apply_idle_noise(remaining_qubit);
                match self.noise_config.ryy.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Degrade => return self.ry(angle, remaining_qubit),
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::ApplyAnyway => {
                        unreachable!("the `ryy` gate does not support the ApplyAnyway loss policy")
                    }
                }
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);

                // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
                // and check to see if it is supported.
                let unitary = unitary_from_normalized_angle(
                    angle,
                    UnitaryOp::Z,
                    UnitaryOp::SqrtZ,
                    UnitaryOp::SqrtZInv,
                );
                if let Some(unitary) = unitary {
                    // Perform Ryy by changing basis to Z and using the Rzz decomposition.
                    self.state.unitary_op(UnitaryOp::SqrtX, &[q1]);
                    self.state.unitary_op(UnitaryOp::SqrtX, &[q2]);
                    self.state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                    self.state.unitary_op(unitary, &[q1]);
                    self.state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                    self.state.unitary_op(UnitaryOp::SqrtXInv, &[q1]);
                    self.state.unitary_op(UnitaryOp::SqrtXInv, &[q2]);
                } else {
                    self.state.rotate(
                        angle,
                        &[paulimer::core::y(q1), paulimer::core::y(q2)].into(),
                    );
                }
            }
        }
        apply_noise!(self, ryy, &[q1, q2]);
    }

    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[q1] { q2 } else { q1 };
                self.apply_idle_noise(remaining_qubit);
                match self.noise_config.rzz.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Degrade => return self.rz(angle, remaining_qubit),
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::ApplyAnyway => {
                        unreachable!("the `rzz` gate does not support the ApplyAnyway loss policy")
                    }
                }
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);

                // We can only perform rotations by multiples of PI / 2 in the stabilizer, so normalize the angle
                // and check to see if it is supported.
                let unitary = unitary_from_normalized_angle(
                    angle,
                    UnitaryOp::Z,
                    UnitaryOp::SqrtZ,
                    UnitaryOp::SqrtZInv,
                );
                if let Some(unitary) = unitary {
                    self.state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                    self.state.unitary_op(unitary, &[q1]);
                    self.state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                } else {
                    self.state.rotate(
                        angle,
                        &[paulimer::core::z(q1), paulimer::core::z(q2)].into(),
                    );
                }
            }
        }
        apply_noise!(self, rzz, &[q1, q2]);
    }

    fn swap(&mut self, q1: QubitID, q2: QubitID) {
        // There are three kinds of swaps:
        //   1. A logical swap, also called a relabel.
        //   2. A swap by physically exchanging the location of the qubits.
        //   3. An exchange of information by doing three CX.
        //
        // This method is concerned with the kinds (1) and (2), since (3)
        // gets decomposed into other instructions before making it to the simulator.
        // In both (1) and (2), the loss state of the qubits gets exchanged.

        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let lost_qubit = if self.loss[q1] { q1 } else { q2 };
                let remaining_qubit = if self.loss[q1] { q2 } else { q1 };
                self.apply_idle_noise(remaining_qubit);
                match self.noise_config.swap.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Degrade => {
                        unreachable!("the `swap` gate does not support the Degrade loss policy")
                    }
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => {
                        self.state.permute(&[1, 0], &[q1, q2]);
                        self.residual_s_dagger(lost_qubit);
                        self.loss.swap(q1, q2);
                    }
                    LossPolicy::ApplyAnyway => {
                        self.state.permute(&[1, 0], &[q1, q2]);
                        self.loss.swap(q1, q2);
                    }
                }
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                self.state.permute(&[1, 0], &[q1, q2]);
                self.loss.swap(q1, q2);
            }
        }

        // Is up to the user if swap is a virtual operation or not.
        // If they don't specify noise/loss probability for swap, then it is virtual.
        apply_noise!(self, swap, &[q1, q2]);
    }

    fn mz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_idle_noise(target);
        self.record_mz(target, result_id);
        apply_noise!(self, mz, &[target]);
    }

    fn mresetz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_idle_noise(target);
        self.record_mresetz(target, result_id);
        apply_noise!(self, mresetz, &[target]);
    }

    fn resetz(&mut self, target: QubitID) {
        self.apply_idle_noise(target);
        self.mresetz_impl(target);
        apply_noise!(self, mresetz, &[target]);
    }

    fn mov(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            apply_noise!(self, mov, &[target]);
        }
    }

    fn correlated_noise_intrinsic(&mut self, intrinsic_id: IntrinsicID, targets: &[usize]) {
        let fault = match self.noise_config.intrinsics.get(&intrinsic_id) {
            Some(correlated_noise) => correlated_noise.sample(&mut self.rng).cloned(),
            None => return,
        };
        if let Some(fault) = fault {
            self.apply_fault(&fault, targets);
        }
    }

    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }

    fn t(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.rotate(
                std::f64::consts::FRAC_PI_4,
                &[paulimer::core::z(target)].into(),
            );
            apply_noise!(self, t, &[target]);
        }
    }

    fn t_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.rotate(
                -std::f64::consts::FRAC_PI_4,
                &[paulimer::core::z(target)].into(),
            );
            apply_noise!(self, t_adj, &[target]);
        }
    }

    fn state_dump(&self) -> &Self::StateDumpData {
        &self.state
    }

    fn apply_readout_noise(&mut self, p_zero_as_one: f64, p_one_as_zero: f64, result_id: QubitID) {
        let measurement = self.measurements[result_id];
        let sample = self.rng.random_range(0.0..1.0);
        let new_measurement = match measurement {
            MeasurementResult::Zero if sample < p_zero_as_one => MeasurementResult::One,
            MeasurementResult::One if sample < p_one_as_zero => MeasurementResult::Zero,
            measurement_result => measurement_result,
        };
        self.measurements[result_id] = new_measurement;
    }
}

fn unitary_from_normalized_angle(
    angle: f64,
    pauli: UnitaryOp,
    sqrt_pauli: UnitaryOp,
    sqrt_pauli_inv: UnitaryOp,
) -> Option<UnitaryOp> {
    let mut normalized_angle = angle % TAU;
    if normalized_angle < 0.0 {
        normalized_angle += TAU;
    }
    if normalized_angle.is_nearly_zero() || (normalized_angle - TAU).is_nearly_zero() {
        // The angle is a multiple of 2 * PI, so the operation is effectively an identity.
        Some(UnitaryOp::I)
    } else if (normalized_angle - PI).is_nearly_zero() {
        // The angle is an odd multiple of PI, so the operation is effectively a Pauli gate.
        Some(pauli)
    } else if (normalized_angle - FRAC_PI_2).is_nearly_zero() {
        // The angle is an odd multiple of PI / 2, so the operation is effectively a sqrt(Pauli) gate.
        Some(sqrt_pauli)
    } else if (normalized_angle - 3.0 * FRAC_PI_2).is_nearly_zero() {
        // The angle is an odd multiple of 3 * PI / 2, so the operation is effectively a sqrt(Pauli) adjoint gate.
        Some(sqrt_pauli_inv)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::StabilizerSimulator;
    use crate::{
        MeasurementResult, Simulator, cpu_full_state_simulator::FullStateSimulator,
        noise_config::CumulativeNoiseConfig,
    };
    use std::sync::Arc;

    fn sample_t_interference<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>(
        seed: u32,
    ) -> MeasurementResult {
        let mut simulator = S::new(1, 1, seed, Arc::new(CumulativeNoiseConfig::default()));
        simulator.h(0);
        simulator.t(0);
        simulator.h(0);
        simulator.mz(0, 0);
        simulator.measurements()[0]
    }

    #[test]
    fn t_interference_matches_full_state_simulator() {
        let shots = 4_096;
        let stabilizer_ones = (0..shots).fold(0_u32, |count, seed| {
            count
                + u32::from(
                    sample_t_interference::<StabilizerSimulator>(seed) == MeasurementResult::One,
                )
        });
        let full_state_ones = (0..shots).fold(0_u32, |count, seed| {
            count
                + u32::from(
                    sample_t_interference::<FullStateSimulator>(seed) == MeasurementResult::One,
                )
        });

        let stabilizer_probability = f64::from(stabilizer_ones) / f64::from(shots);
        let full_state_probability = f64::from(full_state_ones) / f64::from(shots);
        let expected = (2.0 - 2.0_f64.sqrt()) / 4.0;
        assert!((stabilizer_probability - expected).abs() < 0.025);
        assert!((full_state_probability - expected).abs() < 0.025);
        assert!((stabilizer_probability - full_state_probability).abs() < 0.025);
    }

    fn sample_entangled_t_circuit<S: Simulator<Noise = Arc<CumulativeNoiseConfig>>>(
        seed: u32,
    ) -> usize {
        let mut simulator = S::new(2, 2, seed, Arc::new(CumulativeNoiseConfig::default()));
        simulator.h(0);
        simulator.cx(0, 1);
        simulator.t(0);
        simulator.t(1);
        simulator.h(0);
        simulator.h(1);
        simulator.mz(0, 0);
        simulator.mz(1, 1);
        usize::from(simulator.measurements()[0] == MeasurementResult::One) * 2
            + usize::from(simulator.measurements()[1] == MeasurementResult::One)
    }

    #[test]
    fn entangled_t_circuit_matches_full_state_simulator() {
        let shots = 8_192;
        let mut stabilizer_counts = [0_u32; 4];
        let mut full_state_counts = [0_u32; 4];
        for seed in 0..shots {
            stabilizer_counts[sample_entangled_t_circuit::<StabilizerSimulator>(seed)] += 1;
            full_state_counts[sample_entangled_t_circuit::<FullStateSimulator>(seed)] += 1;
        }

        for (stabilizer, full_state) in stabilizer_counts.into_iter().zip(full_state_counts) {
            let difference = f64::from(stabilizer.abs_diff(full_state)) / f64::from(shots);
            assert!(
                difference < 0.025,
                "distribution differs by {difference}: stabilizer={stabilizer_counts:?}, full-state={full_state_counts:?}"
            );
        }
    }
}
