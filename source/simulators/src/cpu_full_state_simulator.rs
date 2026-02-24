// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod noise;

use crate::{
    MeasurementResult, QubitID, Simulator,
    noise_config::{CumulativeNoiseConfig, IntrinsicID},
};
use core::f64;
use nalgebra::Complex;
use noise::Fault;
use noisy_simulator::{
    Instrument, NoisySimulator as _, Operation, StateVectorSimulator, operation,
};
use rand::{SeedableRng as _, rngs::StdRng};
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

/// A noiseless state-vector simulator.
pub struct NoiselessSimulator {
    /// The current state of the simulation.
    state: StateVectorSimulator,
    /// Measurement results.
    measurements: Vec<MeasurementResult>,
}

impl NoiselessSimulator {
    /// Records a z-measurement on the given `target`.
    fn record_mz(&mut self, target: QubitID, result_id: QubitID) {
        let measurement = self.mz_impl(target);
        self.measurements[result_id] = measurement;
    }

    /// Records a z-measurement on the given `target` and reset the qubit to the zero state.
    fn record_mresetz(&mut self, target: QubitID, result_id: QubitID) {
        let measurement = self.mresetz_impl(target);
        self.measurements[result_id] = measurement;
    }

    /// Measures a Z observable on the given `target`.
    fn mz_impl(&mut self, target: QubitID) -> MeasurementResult {
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

    /// Measures a Z observable on the given `target` and reset the qubit to the zero state.
    fn mresetz_impl(&mut self, target: QubitID) -> MeasurementResult {
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
}

impl Simulator for NoiselessSimulator {
    type Noise = ();
    type StateDumpData = noisy_simulator::StateVector;

    fn new(num_qubits: usize, num_results: usize, seed: u32, _noise: Self::Noise) -> Self {
        Self {
            state: StateVectorSimulator::new_with_seed(num_qubits, seed.into()),
            measurements: vec![MeasurementResult::Zero; num_results],
        }
    }

    fn x(&mut self, target: QubitID) {
        self.state
            .apply_operation(&X, &[target])
            .expect("apply_operation should succeed");
    }

    fn y(&mut self, target: QubitID) {
        self.state
            .apply_operation(&Y, &[target])
            .expect("apply_operation should succeed");
    }

    fn z(&mut self, target: QubitID) {
        self.state
            .apply_operation(&Z, &[target])
            .expect("apply_operation should succeed");
    }

    fn h(&mut self, target: QubitID) {
        self.state
            .apply_operation(&H, &[target])
            .expect("apply_operation should succeed");
    }

    fn s(&mut self, target: QubitID) {
        self.state
            .apply_operation(&S, &[target])
            .expect("apply_operation should succeed");
    }

    fn s_adj(&mut self, target: QubitID) {
        self.state
            .apply_operation(&S_ADJ, &[target])
            .expect("apply_operation should succeed");
    }

    fn sx(&mut self, target: QubitID) {
        self.state
            .apply_operation(&SX, &[target])
            .expect("apply_operation should succeed");
    }

    fn sx_adj(&mut self, target: QubitID) {
        self.state
            .apply_operation(&SX_ADJ, &[target])
            .expect("apply_operation should succeed");
    }

    fn t(&mut self, target: QubitID) {
        self.state
            .apply_operation(&T, &[target])
            .expect("apply_operation should succeed");
    }

    fn t_adj(&mut self, target: QubitID) {
        self.state
            .apply_operation(&T_ADJ, &[target])
            .expect("apply_operation should succeed");
    }

    fn rx(&mut self, angle: f64, target: QubitID) {
        self.state
            .apply_operation(&rx(angle), &[target])
            .expect("apply_operation should succeed");
    }

    fn ry(&mut self, angle: f64, target: QubitID) {
        self.state
            .apply_operation(&ry(angle), &[target])
            .expect("apply_operation should succeed");
    }

    fn rz(&mut self, angle: f64, target: QubitID) {
        self.state
            .apply_operation(&rz(angle), &[target])
            .expect("apply_operation should succeed");
    }

    fn cx(&mut self, control: QubitID, target: QubitID) {
        self.state
            .apply_operation(&CX, &[control, target])
            .expect("apply_operation should succeed");
    }

    fn cy(&mut self, control: QubitID, target: QubitID) {
        self.state
            .apply_operation(&CY, &[control, target])
            .expect("apply_operation should succeed");
    }

    fn cz(&mut self, control: QubitID, target: QubitID) {
        self.state
            .apply_operation(&CZ, &[control, target])
            .expect("apply_operation should succeed");
    }

    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.state
            .apply_operation(&rxx(angle), &[q1, q2])
            .expect("apply_operation should succeed");
    }

    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.state
            .apply_operation(&ryy(angle), &[q1, q2])
            .expect("apply_operation should succeed");
    }

    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        self.state
            .apply_operation(&rzz(angle), &[q1, q2])
            .expect("apply_operation should succeed");
    }

    fn swap(&mut self, q1: QubitID, q2: QubitID) {
        self.state
            .apply_operation(&SWAP, &[q1, q2])
            .expect("apply_operation should succeed");
    }

    fn mz(&mut self, target: QubitID, result_id: QubitID) {
        self.record_mz(target, result_id);
    }

    fn mresetz(&mut self, target: QubitID, result_id: QubitID) {
        self.record_mresetz(target, result_id);
    }

    fn resetz(&mut self, target: QubitID) {
        self.mresetz_impl(target);
    }

    fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }

    fn mov(&mut self, _target: QubitID) {
        // MOV instruction is a no-op for the noiseless simulator.
    }

    fn correlated_noise_intrinsic(&mut self, _intrinsic_id: IntrinsicID, _targets: &[usize]) {
        // Noise is a no-op for the noiseless simulator.
    }

    fn state_dump(&self) -> &Self::StateDumpData {
        self.state.state().expect("state should be valid")
    }
}

/// A noisy state-vector simulator.
pub struct NoisySimulator {
    /// The noise configuration for the simulation.
    noise_config: Arc<CumulativeNoiseConfig<Fault>>,
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

impl NoisySimulator {
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
        let fault = self.noise_config.gen_idle_fault(&mut self.rng, idle_time);
        if !self.loss[target] && matches!(fault, Fault::S) {
            self.state
                .apply_operation(&S, &[target])
                .expect("apply_operation should succeed");
        }
    }

    fn apply_fault(&mut self, fault: Fault, targets: &[QubitID]) {
        match fault {
            Fault::None => (),
            Fault::Pauli(pauli_string) => {
                for (pauli, target) in pauli_string.iter().zip(targets) {
                    // We don't apply faults on lost qubits.
                    if self.loss[*target] {
                        continue;
                    }
                    match pauli {
                        noise::PauliFault::I => (),
                        noise::PauliFault::X => self
                            .state
                            .apply_operation(&X, &[*target])
                            .expect("apply_operation should succeed"),
                        noise::PauliFault::Y => self
                            .state
                            .apply_operation(&Y, &[*target])
                            .expect("apply_operation should succeed"),
                        noise::PauliFault::Z => self
                            .state
                            .apply_operation(&Z, &[*target])
                            .expect("apply_operation should succeed"),
                    }
                }
            }
            Fault::S => {
                if !self.loss[targets[0]] {
                    self.state
                        .apply_operation(&S, targets)
                        .expect("apply_operation should succeed");
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
}

/// Design decision: Why is this a macro?
///   Rust doesn't allow taking a mutable reference and an inmutable
///   reference to `self` at the same time. So, the obvious way express
///   this,
///   ```ignore
///   fn apply_loss(&mut self, noise_table: &CumulativeNoiseTable<Fault>, targets: &[QubitID]) {
///       for target in targets {
///           if matches!(noise_table.sample_loss(&mut self.rng), Fault::Loss) {
///               self.mresetz_impl(*target);
///               self.loss[*target] = true;
///           }
///       }
///   }
///   ```
///   and then doing,
///   ```ignore
///   self.apply_loss(&self.noise_config.rxx, targets)
///   ```
///   is not valid rust.
///
///   There are two alternatives. The first one is cloning the Arc
///   containing the noise config before each call to `apply_loss`. In,
///   that way rust doesn't see the cloned Arc as attached to self anymore.
///   ```ignore
///   let noise_config = Arc::clone(&self.noise_config);
///   self.apply_loss(&noise_config.rxx, targets);
///   ```
///   However, this is not ideal. We don't want to be increasing and decreasing
///   the reference count of an Arc in the hot-loop of our simulation.
///
///   The other alternative is creating a function that takes all the necessary
///   members of self as inputs independently,
///   ```ignore
///   fn apply_loss(
///     state: &mut StateType,
///     noise_table: &CumulativeNoiseTable<Fault>,
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
macro_rules! apply_loss {
    ($slf:expr, $noise_table:ident, $targets:expr) => {
        for target in $targets {
            if !$slf.loss[*target] {
                let fault = $slf.noise_config.$noise_table.sample_loss(&mut $slf.rng);
                if matches!(fault, Fault::Loss) {
                    $slf.mresetz_impl(*target);
                    $slf.loss[*target] = true;
                }
            }
        }
    };
}

macro_rules! apply_noise {
    ($slf:expr, $noise_table:ident, $targets:expr) => {{
        let fault = $slf.noise_config.$noise_table.sample_noise(&mut $slf.rng);
        if let Fault::Pauli(pauli_string) = fault {
            for (pauli, target) in pauli_string.iter().zip($targets) {
                // We don't apply faults on lost qubits.
                if $slf.loss[*target] {
                    continue;
                }
                match pauli {
                    noise::PauliFault::I => (),
                    noise::PauliFault::X => $slf
                        .state
                        .apply_operation(&X, &[*target])
                        .expect("apply_operation should succeed"),
                    noise::PauliFault::Y => $slf
                        .state
                        .apply_operation(&Y, &[*target])
                        .expect("apply_operation should succeed"),
                    noise::PauliFault::Z => $slf
                        .state
                        .apply_operation(&Z, &[*target])
                        .expect("apply_operation should succeed"),
                }
            }
        };
    }};
}

impl Simulator for NoisySimulator {
    type Noise = Arc<CumulativeNoiseConfig<Fault>>;
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
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&X, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, x, &[target]);
            apply_noise!(self, x, &[target]);
        }
    }

    fn y(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&Y, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, y, &[target]);
            apply_noise!(self, y, &[target]);
        }
    }

    fn z(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&Z, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, z, &[target]);
            apply_noise!(self, z, &[target]);
        }
    }

    fn h(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&H, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, h, &[target]);
            apply_noise!(self, h, &[target]);
        }
    }

    fn s(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&S, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, s, &[target]);
            apply_noise!(self, s, &[target]);
        }
    }

    fn s_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&S_ADJ, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, s_adj, &[target]);
            apply_noise!(self, s_adj, &[target]);
        }
    }

    fn sx(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&SX, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, sx, &[target]);
            apply_noise!(self, sx, &[target]);
        }
    }

    fn sx_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&SX_ADJ, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, sx_adj, &[target]);
            apply_noise!(self, sx_adj, &[target]);
        }
    }

    fn t(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&T, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, t, &[target]);
            apply_noise!(self, t, &[target]);
        }
    }

    fn t_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&T_ADJ, &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, t_adj, &[target]);
            apply_noise!(self, t_adj, &[target]);
        }
    }

    fn rx(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&rx(angle), &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, rx, &[target]);
            apply_noise!(self, rx, &[target]);
        }
    }

    fn ry(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&ry(angle), &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, ry, &[target]);
            apply_noise!(self, ry, &[target]);
        }
    }

    fn rz(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&rz(angle), &[target])
                .expect("apply_operation should succeed");
            apply_loss!(self, rz, &[target]);
            apply_noise!(self, rz, &[target]);
        }
    }

    fn cx(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&CX, &[control, target])
                .expect("apply_operation should succeed");
        }
        // We still apply operation faults to non-lost qubits.
        apply_loss!(self, cx, &[target]);
        apply_noise!(self, cx, &[target]);
    }

    fn cy(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&CY, &[control, target])
                .expect("apply_operation should succeed");
        }
        // We still apply operation faults to non-lost qubits.
        apply_loss!(self, cy, &[target]);
        apply_noise!(self, cy, &[target]);
    }

    fn cz(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&CZ, &[control, target])
                .expect("apply_operation should succeed");
        }
        // We still apply operation faults to non-lost qubits.
        apply_loss!(self, cz, &[target]);
        apply_noise!(self, cz, &[target]);
    }

    fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) => self.rx(angle, q2),
            (false, true) => self.rx(angle, q1),
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                self.state
                    .apply_operation(&rxx(angle), &[q1, q2])
                    .expect("apply_operation should succeed");
                apply_loss!(self, rxx, &[q1, q2]);
                apply_noise!(self, rxx, &[q1, q2]);
            }
        }
    }

    fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) => self.ry(angle, q2),
            (false, true) => self.ry(angle, q1),
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                self.state
                    .apply_operation(&ryy(angle), &[q1, q2])
                    .expect("apply_operation should succeed");
                apply_loss!(self, ryy, &[q1, q2]);
                apply_noise!(self, ryy, &[q1, q2]);
            }
        }
    }

    fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) => self.rz(angle, q2),
            (false, true) => self.rz(angle, q1),
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                self.state
                    .apply_operation(&rzz(angle), &[q1, q2])
                    .expect("apply_operation should succeed");
                apply_loss!(self, rzz, &[q1, q2]);
                apply_noise!(self, rzz, &[q1, q2]);
            }
        }
    }

    fn swap(&mut self, q1: QubitID, q2: QubitID) {
        match (self.loss[q1], self.loss[q2]) {
            (true, true) => (),
            (true, false) => {
                self.apply_idle_noise(q2);
                self.state
                    .apply_operation(&SWAP, &[q1, q2])
                    .expect("apply_operation should succeed");
            }
            (false, true) => {
                self.apply_idle_noise(q1);
                self.state
                    .apply_operation(&SWAP, &[q1, q2])
                    .expect("apply_operation should succeed");
            }
            (false, false) => {
                self.apply_idle_noise(q1);
                self.apply_idle_noise(q2);
                self.state
                    .apply_operation(&SWAP, &[q1, q2])
                    .expect("apply_operation should succeed");
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
        apply_loss!(self, swap, &[q1, q2]);
        apply_noise!(self, swap, &[q1, q2]);
    }

    fn mz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_idle_noise(target);
        self.record_mz(target, result_id);
        apply_loss!(self, mz, &[target]);
        apply_noise!(self, mz, &[target]);
    }

    fn mresetz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_idle_noise(target);
        self.record_mresetz(target, result_id);
        apply_loss!(self, mresetz, &[target]);
        apply_noise!(self, mresetz, &[target]);
    }

    fn resetz(&mut self, target: QubitID) {
        self.apply_idle_noise(target);
        self.mresetz_impl(target);
        apply_loss!(self, mresetz, &[target]);
        apply_noise!(self, mresetz, &[target]);
    }

    fn mov(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            apply_loss!(self, mov, &[target]);
            apply_noise!(self, mov, &[target]);
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

    fn state_dump(&self) -> &Self::StateDumpData {
        self.state.state().expect("state should be valid")
    }
}
