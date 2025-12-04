mod noise;

use core::f64;
use std::sync::LazyLock;

use crate::{
    noise_config::NoiseConfig,
    stabilizer_simulator::{MeasurementResult, QubitID},
};
use nalgebra::Complex;
use noise::{CumulativeNoiseConfig, Fault};
use noisy_simulator::{Instrument, NoisySimulator, Operation, StateVectorSimulator, operation};

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

static CZ: LazyLock<Operation> = LazyLock::new(|| {
    operation!([1., 0., 0., 0.;
                0., 1., 0., 0.;
                0., 0., 1., 0.;
                0., 0., 0., -1.;])
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

pub struct Simulator {
    /// The noise configuration for the simulation.
    noise_config: CumulativeNoiseConfig,
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

impl Simulator {
    /// Creates a new Simulator with `num_qubits` qubits.
    #[must_use]
    pub fn new(
        num_qubits: usize,
        num_results: usize,
        noise_config: NoiseConfig,
        seed: u32,
    ) -> Self {
        Self {
            noise_config: noise_config.into(),
            state: StateVectorSimulator::new_with_seed(num_qubits, seed.into()),
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
        for q in targets {
            self.reload_qubit(*q);
        }
    }

    /// Single qubit X gate.
    pub fn x(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&X, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.x.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit X gate.
    pub fn y(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&Y, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.y.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit Z gate.
    pub fn z(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&Z, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.z.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit H gate.
    pub fn h(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&H, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.h.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit S gate.
    pub fn s(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&S, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.s.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit S adjoint gate.
    pub fn s_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&S_ADJ, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.s_adj.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit SX gate.
    pub fn sx(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&SX, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.sx.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit  gate.
    pub fn sx_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&SX_ADJ, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.sx_adj.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit T gate.
    pub fn t(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&T, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.t.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit T adjoint gate.
    pub fn t_adj(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&T_ADJ, &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.t_adj.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit RX gate.
    pub fn rx(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&rx(angle), &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.rx.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit RY gate.
    pub fn ry(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&ry(angle), &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.ry.gen_operation_fault(), &[target]);
        }
    }

    /// Single qubit RZ gate.
    pub fn rz(&mut self, angle: f64, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&rz(angle), &[target])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.rz.gen_operation_fault(), &[target]);
        }
    }

    /// Controlled-X gate.
    pub fn cx(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&CX, &[control, target])
                .expect("apply_operation should succeed");
            self.apply_fault(
                self.noise_config.cx.gen_operation_fault(),
                &[control, target],
            );
        }
    }

    /// Controlled-Z gate.
    pub fn cz(&mut self, control: QubitID, target: QubitID) {
        if !self.loss[control] && !self.loss[target] {
            self.apply_idle_noise(control);
            self.apply_idle_noise(target);
            self.state
                .apply_operation(&CZ, &[control, target])
                .expect("apply_operation should succeed");
            self.apply_fault(
                self.noise_config.cz.gen_operation_fault(),
                &[control, target],
            );
        }
    }

    /// Two qubits RXX gate.
    pub fn rxx(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        if !self.loss[q1] && !self.loss[q2] {
            self.apply_idle_noise(q1);
            self.apply_idle_noise(q2);
            self.state
                .apply_operation(&rxx(angle), &[q1, q2])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.rxx.gen_operation_fault(), &[q1, q2]);
        }
    }

    /// Two qubits RYY gate.
    pub fn ryy(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        if !self.loss[q1] && !self.loss[q2] {
            self.apply_idle_noise(q1);
            self.apply_idle_noise(q2);
            self.state
                .apply_operation(&ryy(angle), &[q1, q2])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.ryy.gen_operation_fault(), &[q1, q2]);
        }
    }

    /// Two qubits RZZ gate.
    pub fn rzz(&mut self, angle: f64, q1: QubitID, q2: QubitID) {
        if !self.loss[q1] && !self.loss[q2] {
            self.apply_idle_noise(q1);
            self.apply_idle_noise(q2);
            self.state
                .apply_operation(&rzz(angle), &[q1, q2])
                .expect("apply_operation should succeed");
            self.apply_fault(self.noise_config.rzz.gen_operation_fault(), &[q1, q2]);
        }
    }

    /// `MResetZ` operation.
    pub fn mresetz(&mut self, target: QubitID, result_id: QubitID) {
        self.apply_idle_noise(target);
        self.record_z_measurement(target, result_id);
        self.apply_fault(self.noise_config.mresetz.gen_operation_fault(), &[target]);
    }

    /// Move operation. The purpose of this operation is modeling
    /// the noise coming from qubit movement in neutral atom machines.
    pub fn mov(&mut self, target: QubitID) {
        if !self.loss[target] {
            self.apply_idle_noise(target);
            self.apply_fault(self.noise_config.mov.gen_operation_fault(), &[target]);
        }
    }

    fn apply_idle_noise(&mut self, target: QubitID) {
        let idle_time = self.time - self.last_operation_time[target];
        self.last_operation_time[target] = self.time;
        let fault = self.noise_config.gen_idle_fault(idle_time);
        self.apply_fault(fault, &[target]);
    }

    fn apply_fault(&mut self, fault: Fault, targets: &[QubitID]) {
        match fault {
            Fault::None => (),
            Fault::Pauli(pauli_string) => {
                for (pauli, target) in pauli_string.iter().zip(targets) {
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
                self.state
                    .apply_operation(&S, targets)
                    .expect("apply_operation should succeed");
            }
            Fault::Loss => {
                for target in targets {
                    self.measure_z(*target);
                    self.loss[*target] = true;
                }
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

    /// Returns a list of the measurements recorded during the simulation.
    #[must_use]
    pub fn measurements(&self) -> &[MeasurementResult] {
        &self.measurements
    }

    pub fn take_measurements(&mut self) -> Vec<MeasurementResult> {
        std::mem::take(&mut self.measurements)
    }
}
