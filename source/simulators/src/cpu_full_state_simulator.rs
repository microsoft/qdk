// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    MeasurementResult, QubitID, ResultID, Simulator,
    noise_config::{
        CumulativeNoiseConfig, CumulativeNoiseTable, Fault, FaultTerm, IntrinsicID, LossPolicy,
    },
};
use core::f64;
use nalgebra::Complex;
use noisy_simulator::{
    Instrument, NoisySimulator as _, Operation, StateVectorSimulator, operation,
};
use rand::{RngExt, SeedableRng as _, rngs::StdRng};
use std::sync::{Arc, LazyLock};

static X: LazyLock<Operation> = LazyLock::new(|| {
    operation!([0., 1.;
                1., 0.;])
    .expect("operation should be valid")
});

static Y: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([0., -i;
                i,   0.;])
    .expect("operation should be valid")
});

static Z: LazyLock<Operation> = LazyLock::new(|| {
    operation!([1.,  0.;
                0., -1.;])
    .expect("operation should be valid")
});

static H: LazyLock<Operation> = LazyLock::new(|| {
    let f = 0.5_f64.sqrt();
    operation!([f,  f;
                f, -f;])
    .expect("operation should be valid")
});

static S: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([1., 0.;
                0., i;])
    .expect("operation should be valid")
});

static S_ADJ: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([1.,  0.;
                0., -i;])
    .expect("operation should be valid")
});

static SX: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([(1. + i) / 2., (1. - i) / 2.;
                (1. - i) / 2., (1. + i) / 2.;])
    .expect("operation should be valid")
});

static SX_ADJ: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([(1. - i) / 2., (1. + i) / 2.;
                (1. + i) / 2., (1. - i) / 2.;])
    .expect("operation should be valid")
});

static T: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([1., 0.;
                0., (i * f64::consts::FRAC_PI_4).exp();])
    .expect("operation should be valid")
});

static T_ADJ: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([1., 0.;
                0., (-i * f64::consts::FRAC_PI_4).exp();])
    .expect("operation should be valid")
});

static CX: LazyLock<Operation> = LazyLock::new(|| {
    operation!([1., 0., 0., 0.;
                0., 0., 0., 1.;
                0., 0., 1., 0.;
                0., 1., 0., 0.;])
    .expect("operation should be valid")
});

static CY: LazyLock<Operation> = LazyLock::new(|| {
    let i = Complex::I;
    operation!([1., 0., 0.,  0.;
                0., 0., 0., -i;
                0., 0., 1.,  0.;
                0., i,  0.,  0.;])
    .expect("operation should be valid")
});

static CZ: LazyLock<Operation> = LazyLock::new(|| {
    operation!([1., 0., 0., 0.;
                0., 1., 0., 0.;
                0., 0., 1., 0.;
                0., 0., 0., -1.;])
    .expect("operation should be valid")
});

static SWAP: LazyLock<Operation> = LazyLock::new(|| {
    operation!([1., 0., 0., 0.;
                0., 0., 1., 0.;
                0., 1., 0., 0.;
                0., 0., 0., 1.;])
    .expect("operation should be valid")
});

static MZ: LazyLock<Instrument> = LazyLock::new(|| {
    let mz0 = operation!([1., 0.;
                          0., 0.;])
    .expect("operation should be valid");
    let mz1 = operation!([0., 0.;
                          0., 1.;])
    .expect("operation should be valid");
    Instrument::new(vec![mz0, mz1]).expect("instrument should be valid")
});

fn rx(angle: f64) -> Operation {
    let sin = (angle / 2.0).sin();
    let cos = (angle / 2.0).cos();
    let i = Complex::I;
    operation!([     cos, -i * sin;
                -i * sin,      cos])
    .expect("operation should be valid")
}

fn ry(angle: f64) -> Operation {
    let sin = (angle / 2.0).sin();
    let cos = (angle / 2.0).cos();
    operation!([cos, -sin;
                sin,  cos])
    .expect("operation should be valid")
}

fn rz(angle: f64) -> Operation {
    let i = Complex::I;
    let a = (-i * angle / 2.0).exp();
    let b = (i * angle / 2.0).exp();
    operation!([a, 0.;
                0.,  b])
    .expect("operation should be valid")
}

fn rxx(angle: f64) -> Operation {
    let i = Complex::I;
    let sin = (angle / 2.0).sin();
    let cos = (angle / 2.0).cos();
    let a = -i * sin;
    let b = cos;
    operation!([b,  0., 0., a;
                0., b,  a,  0.;
                0., a,  b,  0.;
                a,  0., 0., b;

    ])
    .expect("operation should be valid")
}

fn ryy(angle: f64) -> Operation {
    let i = Complex::I;
    let sin = (angle / 2.0).sin();
    let cos = (angle / 2.0).cos();
    let a = i * sin;
    let b = cos;
    operation!([b,   0., 0., a;
                0.,  b, -a,  0.;
                0., -a,  b,  0.;
                a,   0., 0., b;

    ])
    .expect("operation should be valid")
}

fn rzz(angle: f64) -> Operation {
    let i = Complex::I;
    let a = (-i * angle / 2.0).exp();
    let b = (i * angle / 2.0).exp();
    operation!([a,  0., 0., 0.;
                0., b,  0., 0.;
                0., 0., b,  0.;
                0., 0., 0., a;

    ])
    .expect("operation should be valid")
}

fn unsupported_apply_anyway(gate: &str) -> ! {
    unreachable!("the `{gate}` gate does not support the ApplyAnyway loss policy")
}

/// A full-state simulator with configurable noise.
pub struct FullStateSimulator {
    /// The noise configuration for the simulation.
    noise_config: Arc<CumulativeNoiseConfig>,
    /// Random number generator used to sample from [`Self::noise_config`].
    rng: StdRng,
    /// The current state of the simulation.
    state: StateVectorSimulator,
    /// A vector storing whether a qubit was lost or not.
    loss: Vec<bool>,
    /// Measurement results.
    measurements: Vec<MeasurementResult>,
    /// The last time each qubit was operated upon.
    last_operation_time: Vec<u32>,
    /// Current simulation time.
    time: u32,
}

impl FullStateSimulator {
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

    fn apply_idle_noise(&mut self, target: QubitID) {
        let idle_time = self.time - self.last_operation_time[target];
        self.last_operation_time[target] = self.time;
        let idle_fault = self.noise_config.gen_idle_fault(&mut self.rng, idle_time);
        if idle_fault && !self.loss[target] {
            self.state
                .apply_operation(&S, &[target])
                .expect("apply_operation should succeed");
        }
    }

    fn apply_fault(&mut self, fault: &Fault, targets: &[QubitID]) {
        for (term, target) in fault.0.iter().zip(targets) {
            // We don't apply faults on lost qubits.
            if self.loss[*target] {
                continue;
            }
            match term {
                FaultTerm::I => (),
                FaultTerm::X => self
                    .state
                    .apply_operation(&X, &[*target])
                    .expect("apply_operation should succeed"),
                FaultTerm::Y => self
                    .state
                    .apply_operation(&Y, &[*target])
                    .expect("apply_operation should succeed"),
                FaultTerm::Z => self
                    .state
                    .apply_operation(&Z, &[*target])
                    .expect("apply_operation should succeed"),
                FaultTerm::Loss => {
                    self.mresetz_impl(*target);
                    self.loss[*target] = true;
                }
            }
        }
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
        operation: &Operation,
        select_table: impl for<'a> FnOnce(&'a CumulativeNoiseConfig) -> &'a CumulativeNoiseTable,
        target: QubitID,
    ) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(operation, &[target])
                .expect("apply_operation should succeed");
            self.apply_noise(select_table, &[target]);
        }
    }

    fn apply_single_qubit_rotation(
        &mut self,
        angle: f64,
        operation: impl FnOnce(f64) -> Operation,
        select_table: impl for<'a> FnOnce(&'a CumulativeNoiseConfig) -> &'a CumulativeNoiseTable,
        target: QubitID,
    ) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&operation(angle), &[target])
                .expect("apply_operation should succeed");
            self.apply_noise(select_table, &[target]);
        }
    }

    /// Applies a controlled operation under its configured loss policy.
    ///
    /// Operation faults are still sampled for non-lost qubits when loss prevents the gate itself.
    fn apply_controlled_operation(
        &mut self,
        operation: &Operation,
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
                self.state
                    .apply_operation(operation, &targets)
                    .expect("apply_operation should succeed");
            }
        }
        self.apply_noise(select_table, &targets);
    }

    /// Applies a two-qubit rotation under its configured loss policy.
    ///
    /// `select_gate` keeps the noise table, diagnostic name, and one-qubit degradation
    /// operation together. Degradation delegates to the one-qubit path and returns before
    /// sampling two-qubit operation faults.
    fn apply_two_qubit_rotation<Degrade>(
        &mut self,
        angle: f64,
        operation: impl FnOnce(f64) -> Operation,
        select_gate: impl for<'a> Fn(
            &'a CumulativeNoiseConfig,
        ) -> (&'a CumulativeNoiseTable, &'static str, Degrade),
        q1: QubitID,
        q2: QubitID,
    ) where
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
                self.state
                    .apply_operation(&operation(angle), &[q1, q2])
                    .expect("apply_operation should succeed");
            }
        }
        self.apply_noise(|config| select_gate(config).0, &[q1, q2]);
    }

    /// Applies an `S` adjoint to the given target
    /// Used by the [`LossPolicy::ResidualSDagger`] behavior.
    fn residual_s_dagger(&mut self, target: QubitID) {
        self.apply_idle_noise(target);
        self.state
            .apply_operation(&S_ADJ, &[target])
            .expect("apply_operation should succeed");
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

        // MZ on `target`.
        let r = self
            .state
            .apply_instrument(&MZ, &[target])
            .expect("apply_instrument should succeed");

        if r == 1 {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        }
    }

    /// Measures a Z observable on the given `target` and reset the target to the zero state.
    fn mresetz_impl(&mut self, target: QubitID) -> MeasurementResult {
        if self.loss[target] {
            self.loss[target] = false;
            return MeasurementResult::Loss;
        }

        // MZ on `target`.
        let r = self
            .state
            .apply_instrument(&MZ, &[target])
            .expect("apply_instrument should succeed");

        if r == 1 {
            // Reset `target` to zero state.
            self.state
                .apply_operation(&X, &[target])
                .expect("apply_operation should succeed");
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        }
    }

    fn loss_impl(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.mresetz_impl(target);
            self.loss[target] = true;
        }
    }
}

impl Simulator for FullStateSimulator {
    type Noise = Arc<CumulativeNoiseConfig>;
    type StateDumpData = noisy_simulator::StateVector;

    fn new(num_qubits: usize, num_results: usize, seed: u32, noise_config: Self::Noise) -> Self {
        Self {
            noise_config,
            rng: StdRng::seed_from_u64(u64::from(seed)),
            state: StateVectorSimulator::new_with_seed(num_qubits, seed.into()),
            loss: vec![false; num_qubits],
            measurements: vec![MeasurementResult::Zero; num_results],
            last_operation_time: vec![0; num_qubits],
            time: 0,
        }
    }

    fn x(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&X, |config| &config.x, target);
    }

    fn y(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&Y, |config| &config.y, target);
    }

    fn z(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&Z, |config| &config.z, target);
    }

    fn h(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&H, |config| &config.h, target);
    }

    fn s(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&S, |config| &config.s, target);
    }

    fn s_adj(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&S_ADJ, |config| &config.s_adj, target);
    }

    fn sx(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&SX, |config| &config.sx, target);
    }

    fn sx_adj(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&SX_ADJ, |config| &config.sx_adj, target);
    }

    fn t(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&T, |config| &config.t, target);
    }

    fn t_adj(&mut self, target: QubitID) {
        self.apply_single_qubit_operation(&T_ADJ, |config| &config.t_adj, target);
    }

    fn rx(&mut self, angle: f64, target: QubitID) {
        self.apply_single_qubit_rotation(angle, rx, |config| &config.rx, target);
    }

    fn ry(&mut self, angle: f64, target: QubitID) {
        self.apply_single_qubit_rotation(angle, ry, |config| &config.ry, target);
    }

    fn rz(&mut self, angle: f64, target: QubitID) {
        self.apply_single_qubit_rotation(angle, rz, |config| &config.rz, target);
    }

    fn cx(&mut self, control: QubitID, target: QubitID) {
        self.apply_controlled_operation(&CX, |config| &config.cx, control, target);
    }

    fn cy(&mut self, control: QubitID, target: QubitID) {
        self.apply_controlled_operation(&CY, |config| &config.cy, control, target);
    }

    fn cz(&mut self, control: QubitID, target: QubitID) {
        self.apply_controlled_operation(&CZ, |config| &config.cz, control, target);
    }

    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.apply_two_qubit_rotation(angle, rxx, |config| (&config.rxx, "rxx", Self::rx), q1, q2);
    }

    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.apply_two_qubit_rotation(angle, ryy, |config| (&config.ryy, "ryy", Self::ry), q1, q2);
    }

    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.apply_two_qubit_rotation(angle, rzz, |config| (&config.rzz, "rzz", Self::rz), q1, q2);
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
                        self.state
                            .apply_operation(&SWAP, &[q1, q2])
                            .expect("apply_operation should succeed");
                        self.residual_s_dagger(lost_qubit);
                        self.loss.swap(q1, q2);
                    }
                    LossPolicy::ApplyAnyway => {
                        self.state
                            .apply_operation(&SWAP, &[q1, q2])
                            .expect("apply_operation should succeed");
                        self.loss.swap(q1, q2);
                    }
                }
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                self.state
                    .apply_operation(&SWAP, &[q1, q2])
                    .expect("apply_operation should succeed");
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

    fn state_dump(&self) -> &Self::StateDumpData {
        self.state.state().expect("state should be valid")
    }

    fn peek_loss(&mut self, target: QubitID, result_id: ResultID) {
        let is_lost = self.loss[target];
        self.measurements[result_id] = if is_lost {
            MeasurementResult::One
        } else {
            MeasurementResult::Zero
        };
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
}
