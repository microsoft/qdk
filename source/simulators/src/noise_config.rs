// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Noise description for each operation.
///
/// This is the format in which the user config files are
/// written.
#[derive(Clone, Copy, Debug)]
pub struct NoiseConfig {
    pub i: NoiseTable,
    pub x: NoiseTable,
    pub y: NoiseTable,
    pub z: NoiseTable,
    pub h: NoiseTable,
    pub s: NoiseTable,
    pub s_adj: NoiseTable,
    pub t: NoiseTable,
    pub t_adj: NoiseTable,
    pub sx: NoiseTable,
    pub sx_adj: NoiseTable,
    pub rx: NoiseTable,
    pub ry: NoiseTable,
    pub rz: NoiseTable,
    pub cx: NoiseTable,
    pub cz: NoiseTable,
    pub rxx: NoiseTable,
    pub ryy: NoiseTable,
    pub rzz: NoiseTable,
    pub swap: NoiseTable,
    pub mov: NoiseTable,
    pub mresetz: NoiseTable,
    pub idle: IdleNoiseParams,
}

impl NoiseConfig {
    pub const NOISELESS: Self = Self {
        i: NoiseTable::NOISELESS,
        x: NoiseTable::NOISELESS,
        y: NoiseTable::NOISELESS,
        z: NoiseTable::NOISELESS,
        h: NoiseTable::NOISELESS,
        s: NoiseTable::NOISELESS,
        s_adj: NoiseTable::NOISELESS,
        t: NoiseTable::NOISELESS,
        t_adj: NoiseTable::NOISELESS,
        sx: NoiseTable::NOISELESS,
        sx_adj: NoiseTable::NOISELESS,
        rx: NoiseTable::NOISELESS,
        ry: NoiseTable::NOISELESS,
        rz: NoiseTable::NOISELESS,
        cx: NoiseTable::NOISELESS,
        cz: NoiseTable::NOISELESS,
        rxx: NoiseTable::NOISELESS,
        ryy: NoiseTable::NOISELESS,
        rzz: NoiseTable::NOISELESS,
        swap: NoiseTable::NOISELESS,
        mov: NoiseTable::NOISELESS,
        mresetz: NoiseTable::NOISELESS,
        idle: IdleNoiseParams::NOISELESS,
    };
}

/// The probability of idle noise is computed using the equation:
///   `idle_noise_prob(steps) = (s_probability + 1.0).pow(step) - 1.0`
///
/// Where:
///  - `s_probability`: is the probability of an `S` happening during
///    an idle a step, and is in the range `[0, 1]`.
///
/// This structure allows the user to paremetrize the equation.
#[derive(Clone, Copy, Debug)]
pub struct IdleNoiseParams {
    pub s_probability: f32,
}

impl IdleNoiseParams {
    pub const NOISELESS: Self = Self { s_probability: 0.0 };

    fn s_probability(self, steps: u32) -> f32 {
        (self.s_probability + 1.0).powi(i32::try_from(steps).expect("steps should fit in 31 bits"))
            - 1.0
    }
}

/// Describes the noise configuration for each operation.
///
/// This is the internal format used by the simulator.
pub(crate) struct CumulativeNoiseConfig {
    pub i: CumulativeNoiseTable,
    pub x: CumulativeNoiseTable,
    pub y: CumulativeNoiseTable,
    pub z: CumulativeNoiseTable,
    pub h: CumulativeNoiseTable,
    pub s: CumulativeNoiseTable,
    pub s_adj: CumulativeNoiseTable,
    pub t: CumulativeNoiseTable,
    pub t_adj: CumulativeNoiseTable,
    pub sx: CumulativeNoiseTable,
    pub sx_adj: CumulativeNoiseTable,
    pub rx: CumulativeNoiseTable,
    pub ry: CumulativeNoiseTable,
    pub rz: CumulativeNoiseTable,
    pub cx: CumulativeNoiseTable,
    pub cz: CumulativeNoiseTable,
    pub rxx: CumulativeNoiseTable,
    pub ryy: CumulativeNoiseTable,
    pub rzz: CumulativeNoiseTable,
    pub swap: CumulativeNoiseTable,
    pub mov: CumulativeNoiseTable,
    pub mresetz: CumulativeNoiseTable,
    pub idle: IdleNoiseParams,
}

impl From<NoiseConfig> for CumulativeNoiseConfig {
    fn from(value: NoiseConfig) -> Self {
        Self {
            i: value.i.into(),
            x: value.x.into(),
            y: value.y.into(),
            z: value.z.into(),
            h: value.h.into(),
            s: value.s.into(),
            s_adj: value.s_adj.into(),
            t: value.t.into(),
            t_adj: value.t_adj.into(),
            sx: value.sx.into(),
            sx_adj: value.sx_adj.into(),
            rx: value.rx.into(),
            ry: value.ry.into(),
            rz: value.rz.into(),
            cx: value.cx.into(),
            cz: value.cz.into(),
            rxx: value.rxx.into(),
            ryy: value.ryy.into(),
            rzz: value.rzz.into(),
            swap: value.swap.into(),
            mov: value.mov.into(),
            mresetz: value.mresetz.into(),
            idle: value.idle,
        }
    }
}

impl CumulativeNoiseConfig {
    /// Samples a float in the range [0, 1] and picks one of the faults
    /// `X`, `Y`, `Z`, `S` based on the provided noise table.
    pub fn gen_idle_fault(&self, idle_steps: u32) -> Fault {
        let sample: f32 = rand::rngs::ThreadRng::default().gen_range(0.0..1.0);
        if sample < self.idle.s_probability(idle_steps) {
            Fault::S
        } else {
            Fault::None
        }
    }

    /// Samples a float in the range [0, 1] and picks one of the faults
    /// `X`, `Y`, `Z`, `S` based on the provided noise table.
    pub fn gen_idle_fault_with_sample(&self, idle_steps: u32, sample: f32) -> Fault {
        if sample < self.idle.s_probability(idle_steps) {
            Fault::S
        } else {
            Fault::None
        }
    }
}

/// Noise description for an operation.
/// Each field must be a number in the range[0, 1]
/// representing the probability of that kind of fault
/// happening. The x, y, z probabilities should add to
/// a number equal or less than 1.
///
/// This is the format in which the user config files are
/// written.
#[derive(Clone, Copy, Debug)]
pub struct NoiseTable {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub loss: f32,
}

impl NoiseTable {
    pub const NOISELESS: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        loss: 0.0,
    };

    #[must_use]
    pub fn is_noiseless(&self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.z == 0.0 && self.loss == 0.0
    }
}

/// A cumulative representation of the `NoiseTable` to make
/// computation more efficient.
///
/// This is the internal format used by the simulator.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CumulativeNoiseTable {
    x: f32,
    y: f32,
    z: f32,
    loss: f32,
}

impl From<NoiseTable> for CumulativeNoiseTable {
    fn from(value: NoiseTable) -> Self {
        let NoiseTable { x, y, z, loss } = value;
        assert!(
            x + y + z + loss <= 1.0,
            "`NoiseTable` probabilities should add to 1.0 or less"
        );
        Self {
            x,
            y: x + y,
            z: x + y + z,
            loss,
        }
    }
}

impl CumulativeNoiseTable {
    /// Samples a float in the range [0, 1] and picks one of the faults
    /// `X`, `Y`, `Z`, `Loss` based on the provided noise table.
    pub fn gen_operation_fault(&self) -> Fault {
        let sample: f32 = rand::rngs::ThreadRng::default().gen_range(0.0..1.0);
        if sample < self.loss {
            return Fault::Loss;
        }
        let sample: f32 = rand::rngs::ThreadRng::default().gen_range(0.0..1.0);
        if sample < self.x {
            Fault::X
        } else if sample < self.y {
            Fault::Y
        } else if sample < self.z {
            Fault::Z
        } else {
            Fault::None
        }
    }

    /// Samples a float in the range [0, 1] and picks one of the faults
    /// `X`, `Y`, `Z`, `Loss` based on the provided noise table.
    pub fn gen_operation_fault_with_samples(&self, loss_sample: f32, pauli_sample: f32) -> Fault {
        if loss_sample < self.loss {
            return Fault::Loss;
        }
        if pauli_sample < self.x {
            Fault::X
        } else if pauli_sample < self.y {
            Fault::Y
        } else if pauli_sample < self.z {
            Fault::Z
        } else {
            Fault::None
        }
    }
}
