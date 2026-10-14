// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This crate implements a stabilizer simulator for the QDK.

pub mod branching_state;
pub mod operation;

use crate::{
    MeasurementResult, NearlyZero, QubitID, ResultID, Simulator,
    noise_config::{
        CumulativeNoiseConfig, CumulativeNoiseTable, Fault, FaultTerm, IntrinsicID, LossPolicy,
    },
};
use branching_state::BranchingState;
use operation::Operation;
use paulimer::{PauliObservable, UnitaryOp, core::PositionedPauliObservable, pauli::SparsePauli};
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

fn unsupported_apply_anyway(gate: &str) -> ! {
    unreachable!("the `{gate}` gate does not support the ApplyAnyway loss policy")
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

    fn apply_noise(
        &mut self,
        select_table: impl for<'a> FnOnce(&'a CumulativeNoiseConfig) -> &'a CumulativeNoiseTable,
        targets: &[QubitID],
    ) {
        let fault = select_table(self.noise_config.as_ref()).sample_noise(&mut self.rng);
        if let Some(fault) = fault {
            self.apply_fault(&fault, targets);
        }
    }

    fn apply_single_qubit_operation(
        &mut self,
        operation: UnitaryOp,
        select_table: impl for<'a> FnOnce(&'a CumulativeNoiseConfig) -> &'a CumulativeNoiseTable,
        target: QubitID,
    ) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.unitary_op(operation, &[target]);
            self.apply_noise(select_table, &[target]);
        }
    }

    fn apply_single_qubit_rotation(
        &mut self,
        angle: f64,
        clifford_rotations: (UnitaryOp, UnitaryOp, UnitaryOp),
        observable: impl FnOnce(QubitID) -> PositionedPauliObservable,
        select_table: impl for<'a> FnOnce(&'a CumulativeNoiseConfig) -> &'a CumulativeNoiseTable,
        target: QubitID,
    ) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            apply_clifford_or_general_rotation(
                &mut self.state,
                angle,
                clifford_rotations,
                |state, unitary| state.unitary_op(unitary, &[target]),
                || [observable(target)].into(),
            );
            self.apply_noise(select_table, &[target]);
        }
    }

    /// Applies a controlled operation under its configured loss policy.
    ///
    /// Operation faults are still sampled for non-lost qubits when loss prevents the gate itself.
    fn apply_controlled_operation(
        &mut self,
        apply_operation: impl FnOnce(&mut BranchingState, QubitID, QubitID),
        select_table: impl for<'a> Fn(&'a CumulativeNoiseConfig) -> &'a CumulativeNoiseTable,
        control: QubitID,
        target: QubitID,
    ) {
        let targets = [control, target];
        match (self.loss[control], self.loss[target]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[control] { target } else { control };
                self.apply_idle_noise(remaining_qubit);
                match select_table(self.noise_config.as_ref()).on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::Degrade | LossPolicy::ApplyAnyway => unreachable!(
                        "controlled gates do not support the Degrade or ApplyAnyway loss policies"
                    ),
                }
            }
            (false, false) => {
                self.apply_idle_noise(control);
                self.apply_idle_noise(target);
                apply_operation(&mut self.state, control, target);
            }
        }
        self.apply_noise(select_table, &targets);
    }

    /// Applies a two-qubit rotation under its configured loss policy.
    ///
    /// `select_gate` keeps the noise table, diagnostic name, and one-qubit degradation
    /// operation together. Degradation delegates to the one-qubit path and returns before
    /// sampling two-qubit operation faults.
    fn apply_two_qubit_rotation<Observable, ApplyClifford, Degrade>(
        &mut self,
        angle: f64,
        observable: Observable,
        apply_clifford_rotation: ApplyClifford,
        select_gate: impl for<'a> Fn(
            &'a CumulativeNoiseConfig,
        ) -> (&'a CumulativeNoiseTable, &'static str, Degrade),
        q1: QubitID,
        q2: QubitID,
    ) where
        Observable: Fn(QubitID) -> PositionedPauliObservable,
        ApplyClifford: FnOnce(&mut BranchingState, UnitaryOp, QubitID, QubitID),
        Degrade: FnOnce(&mut Self, f64, QubitID),
    {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) | (false, true) => {
                let remaining_qubit = if self.loss[q1] { q2 } else { q1 };
                self.apply_idle_noise(remaining_qubit);
                let (noise_table, gate, degrade_operation) =
                    select_gate(self.noise_config.as_ref());
                match noise_table.on_loss {
                    LossPolicy::Skip => (),
                    LossPolicy::Degrade => {
                        degrade_operation(self, angle, remaining_qubit);
                        return;
                    }
                    LossPolicy::Propagate => self.loss_impl(remaining_qubit),
                    LossPolicy::ResidualSDagger => self.residual_s_dagger(remaining_qubit),
                    LossPolicy::ApplyAnyway => unsupported_apply_anyway(gate),
                }
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                apply_clifford_or_general_rotation(
                    &mut self.state,
                    angle,
                    (UnitaryOp::Z, UnitaryOp::SqrtZ, UnitaryOp::SqrtZInv),
                    |state, unitary| apply_clifford_rotation(state, unitary, q1, q2),
                    || [observable(q1), observable(q2)].into(),
                );
            }
        }
        self.apply_noise(|config| select_gate(config).0, &[q1, q2]);
    }

    /// Applies an `S` adjoint to the given target
    /// Used by the [`LossPolicy::ResidualSDagger`] behavior.
    fn residual_s_dagger(&mut self, target: QubitID) {
        self.apply_idle_noise(target);
        self.state.unitary_op(UnitaryOp::SqrtZInv, &[target]);
    }

    /// Records a z-measurement on the given `target`.
    fn record_mz(&mut self, target: QubitID, result_id: ResultID) {
        let measurement = self.mz_impl(target);
        self.measurements[result_id] = measurement;
    }

    /// Records a z-measurement on the given `target` and resets the qubit to the zero state.
    fn record_mresetz(&mut self, target: QubitID, result_id: ResultID) {
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
        self.apply_single_qubit_operation(UnitaryOp::X, |config| &config.x, target);
    }

    fn y(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(UnitaryOp::Y, |config| &config.y, target);
    }

    fn z(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(UnitaryOp::Z, |config| &config.z, target);
    }

    fn h(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(UnitaryOp::Hadamard, |config| &config.h, target);
    }

    fn s(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(UnitaryOp::SqrtZ, |config| &config.s, target);
    }

    fn s_adj(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(UnitaryOp::SqrtZInv, |config| &config.s_adj, target);
    }

    fn sx(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(UnitaryOp::SqrtX, |config| &config.sx, target);
    }

    fn sx_adj(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(UnitaryOp::SqrtXInv, |config| &config.sx_adj, target);
    }

    fn cx(&mut self, control: QubitID, target: QubitID) {
        self.apply_controlled_operation(
            |state, control, target| {
                state.unitary_op(UnitaryOp::ControlledX, &[control, target]);
            },
            |config| &config.cx,
            control,
            target,
        );
    }

    fn cy(&mut self, control: QubitID, target: QubitID) {
        self.apply_controlled_operation(
            |state, control, target| {
                state.unitary_op(UnitaryOp::SqrtZInv, &[target]);
                state.unitary_op(UnitaryOp::ControlledX, &[control, target]);
                state.unitary_op(UnitaryOp::SqrtZ, &[target]);
            },
            |config| &config.cy,
            control,
            target,
        );
    }

    fn cz(&mut self, control: QubitID, target: QubitID) {
        self.apply_controlled_operation(
            |state, control, target| {
                state.unitary_op(UnitaryOp::ControlledZ, &[control, target]);
            },
            |config| &config.cz,
            control,
            target,
        );
    }

    fn rx(&mut self, angle: f64, target: QubitID) {
        self.apply_single_qubit_rotation(
            angle,
            (UnitaryOp::X, UnitaryOp::SqrtX, UnitaryOp::SqrtXInv),
            paulimer::core::x,
            |config| &config.rx,
            target,
        );
    }

    fn ry(&mut self, angle: f64, target: QubitID) {
        self.apply_single_qubit_rotation(
            angle,
            (UnitaryOp::Y, UnitaryOp::SqrtY, UnitaryOp::SqrtYInv),
            paulimer::core::y,
            |config| &config.ry,
            target,
        );
    }

    fn rz(&mut self, angle: f64, target: QubitID) {
        self.apply_single_qubit_rotation(
            angle,
            (UnitaryOp::Z, UnitaryOp::SqrtZ, UnitaryOp::SqrtZInv),
            paulimer::core::z,
            |config| &config.rz,
            target,
        );
    }

    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.apply_two_qubit_rotation(
            angle,
            paulimer::core::x,
            |state: &mut BranchingState, unitary, q1, q2| {
                state.unitary_op(UnitaryOp::Hadamard, &[q1]);
                state.unitary_op(UnitaryOp::Hadamard, &[q2]);
                state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                state.unitary_op(unitary, &[q1]);
                state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                state.unitary_op(UnitaryOp::Hadamard, &[q1]);
                state.unitary_op(UnitaryOp::Hadamard, &[q2]);
            },
            |config| (&config.rxx, "rxx", Self::rx),
            q1,
            q2,
        );
    }

    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.apply_two_qubit_rotation(
            angle,
            paulimer::core::y,
            |state: &mut BranchingState, unitary, q1, q2| {
                state.unitary_op(UnitaryOp::SqrtX, &[q1]);
                state.unitary_op(UnitaryOp::SqrtX, &[q2]);
                state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                state.unitary_op(unitary, &[q1]);
                state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                state.unitary_op(UnitaryOp::SqrtXInv, &[q1]);
                state.unitary_op(UnitaryOp::SqrtXInv, &[q2]);
            },
            |config| (&config.ryy, "ryy", Self::ry),
            q1,
            q2,
        );
    }

    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.apply_two_qubit_rotation(
            angle,
            paulimer::core::z,
            |state: &mut BranchingState, unitary, q1, q2| {
                state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
                state.unitary_op(unitary, &[q1]);
                state.unitary_op(UnitaryOp::ControlledX, &[q2, q1]);
            },
            |config| (&config.rzz, "rzz", Self::rz),
            q1,
            q2,
        );
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
                        self.last_operation_time.swap(q1, q2);
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
        self.apply_noise(|config| &config.swap, &[q1, q2]);
    }

    fn mz(&mut self, target: QubitID, result_id: ResultID) {
        self.apply_idle_noise(target);
        self.record_mz(target, result_id);
        self.apply_noise(|config| &config.mz, &[target]);
    }

    fn mresetz(&mut self, target: QubitID, result_id: ResultID) {
        self.apply_idle_noise(target);
        self.record_mresetz(target, result_id);
        self.apply_noise(|config| &config.mresetz, &[target]);
    }

    fn resetz(&mut self, target: QubitID) {
        self.apply_idle_noise(target);
        self.mresetz_impl(target);
        self.apply_noise(|config| &config.mresetz, &[target]);
    }

    fn mov(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.apply_noise(|config| &config.mov, &[target]);
        }
    }

    fn correlated_noise_intrinsic(&mut self, intrinsic_id: IntrinsicID, targets: &[QubitID]) {
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
            self.apply_noise(|config| &config.t, &[target]);
        }
    }

    fn t_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state.rotate(
                -std::f64::consts::FRAC_PI_4,
                &[paulimer::core::z(target)].into(),
            );
            self.apply_noise(|config| &config.t_adj, &[target]);
        }
    }

    fn state_dump(&self) -> &Self::StateDumpData {
        &self.state
    }

    fn peek_loss(&mut self, qubit: QubitID, result_id: ResultID) {
        let is_lost = self.loss[qubit];
        self.measurements[result_id] = if is_lost {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        };
    }

    fn apply_loss_noise(&mut self, p_loss: f64, target: QubitID) {
        if self.rng.random_bool(p_loss) {
            self.loss_impl(target);
        }
    }

    fn apply_readout_noise(&mut self, p_zero_as_one: f64, p_one_as_zero: f64, result_id: ResultID) {
        let measurement = self.measurements[result_id];
        let sample = self.rng.random_range(0.0..1.0);
        let new_measurement = match measurement {
            MeasurementResult::Zero if sample < p_zero_as_one => MeasurementResult::One,
            MeasurementResult::One if sample < p_one_as_zero => MeasurementResult::Zero,
            measurement_result => measurement_result,
        };
        self.measurements[result_id] = new_measurement;
    }

    fn write_result(&mut self, value: bool, result_id: ResultID) {
        self.measurements[result_id] = if value {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        };
    }
}

/// Uses an exact Clifford realization when available, or applies the general Pauli rotation.
///
/// `clifford_rotations` contains the operations for `PI`, `PI / 2`, and `-PI / 2`, respectively.
/// `make_pauli` is evaluated only when the angle requires the general rotation path.
fn apply_clifford_or_general_rotation(
    state: &mut BranchingState,
    angle: f64,
    clifford_rotations: (UnitaryOp, UnitaryOp, UnitaryOp),
    apply_clifford: impl FnOnce(&mut BranchingState, UnitaryOp),
    make_pauli: impl FnOnce() -> SparsePauli,
) {
    let (full_rotation, half_rotation, half_rotation_adjoint) = clifford_rotations;
    if let Some(unitary) =
        unitary_from_normalized_angle(angle, full_rotation, half_rotation, half_rotation_adjoint)
    {
        apply_clifford(state, unitary);
    } else {
        state.rotate(angle, &make_pauli());
    }
}

/// Returns the exact Clifford unitary for a rotation by an integer multiple of `PI / 2`.
///
/// Returns `None` for angles that require the general Pauli rotation path.
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
