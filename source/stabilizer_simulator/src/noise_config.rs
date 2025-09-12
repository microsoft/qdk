// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::Fault;
use rand::Rng;

/// Noise description for each operation.
///
/// This is the format in which the user config files are
/// written.
#[derive(Default)]
pub struct NoiseConfig {
    x: NoiseTable,
    y: NoiseTable,
    z: NoiseTable,
    h: NoiseTable,
    s: NoiseTable,
    cz: NoiseTable,
    mov: NoiseTable,
    mz: NoiseTable,
}

/// Describes the noise configuration for each operation.
///
/// This is the internal format used by the simulator.
#[derive(Default)]
pub(crate) struct CumulativeNoiseConfig {
    pub x: CumulativeNoiseTable,
    pub y: CumulativeNoiseTable,
    pub z: CumulativeNoiseTable,
    pub h: CumulativeNoiseTable,
    pub s: CumulativeNoiseTable,
    pub cz: CumulativeNoiseTable,
    pub mov: CumulativeNoiseTable,
    pub mresetz: CumulativeNoiseTable,
}

impl From<NoiseConfig> for CumulativeNoiseConfig {
    fn from(value: NoiseConfig) -> Self {
        Self {
            x: value.x.into(),
            y: value.y.into(),
            z: value.z.into(),
            h: value.h.into(),
            s: value.s.into(),
            cz: value.cz.into(),
            mov: value.mov.into(),
            mresetz: value.mz.into(),
        }
    }
}

impl CumulativeNoiseConfig {
    /// Samples a float in the range [0, 1] and picks one of the faults
    /// `X`, `Y`, `Z`, `S` based on the provided noise table.
    pub fn gen_idle_fault(&self, _idle_steps: u32) -> Fault {
        // TODO: 1. How the idle noise accumulates.
        //          Is it just `(p(idle_noise) + 1.0).pow(idle_steps) - 1.0`
        //          or is it some other function?
        //       2. How to pick among X, Y, Z, S?
        Fault::None
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
#[derive(Default)]
pub struct NoiseTable {
    x: f32,
    y: f32,
    z: f32,
    loss: f32,
}

/// A cumulative representation of the NoiseTable to make
/// computation more efficient.
///
/// This is the internal format used by the simulator.
#[derive(Default)]
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
}
