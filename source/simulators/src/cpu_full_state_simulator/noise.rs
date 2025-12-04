use crate::noise_config::{IdleNoiseParams, NoiseConfig, NoiseTable};
use rand::Rng;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PauliFault {
    I,
    X,
    Y,
    Z,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// No fault occurred.
    None,
    /// A Pauli fault.
    Pauli(Vec<PauliFault>),
    /// A gradual dephasing fault. Qubits are always slowly
    /// rotating along the Z-axis with an unknown rate,
    /// eventually resulting in an `S` gate.
    S,
    /// The qubit was lost.
    Loss,
}

/// A cumulative representation of the `NoiseTable` to make
/// computation more efficient.
///
/// This is the internal format used by the simulator.
#[derive(Clone, Debug)]
pub(crate) struct CumulativeNoiseTable {
    pub pauli_strings: Vec<Vec<PauliFault>>,
    pub probabilities: Vec<f32>,
    pub loss: f32,
}

impl From<NoiseTable> for CumulativeNoiseTable {
    fn from(value: NoiseTable) -> Self {
        let mut probabilities = Vec::new();
        for p in value.probabilities {
            if let Some(last_p) = probabilities.last() {
                probabilities.push(last_p + p);
            } else {
                probabilities.push(p);
            }
        }
        if let Some(last_p) = probabilities.last() {
            assert!(
                *last_p <= 1.0,
                "`NoiseTable` probabilities should add up to a number less or equal than 1.0"
            );
        }
        assert!(
            value.loss <= 1.0,
            "loss probability should be less or equal than 1.0"
        );

        let mut pauli_strings = Vec::new();
        for pauli_string in value.pauli_strings {
            pauli_strings.push(
                pauli_string
                    .chars()
                    .map(|c| match c {
                        'I' => PauliFault::I,
                        'X' => PauliFault::X,
                        'Y' => PauliFault::Y,
                        'Z' => PauliFault::Z,
                        _ => panic!("invalid character in pauli string {c}"),
                    })
                    .collect(),
            );
        }

        Self {
            pauli_strings,
            probabilities,
            loss: value.loss,
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

        // We don't reuse the sample we used for loss, because then we would never
        // sample anything below that for the Clifford faults.
        let sample: f32 = rand::rngs::ThreadRng::default().gen_range(0.0..1.0);
        for (p, pauli_string) in self.probabilities.iter().zip(self.pauli_strings.iter()) {
            if sample < *p {
                return Fault::Pauli(pauli_string.clone());
            }
        }
        Fault::None
    }
}

/// Describes the noise configuration for each operation.
///
/// This is the internal format used by the simulator.
pub(crate) struct CumulativeNoiseConfig {
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
    pub mov: CumulativeNoiseTable,
    pub mresetz: CumulativeNoiseTable,
    pub idle: IdleNoiseParams,
}

impl From<NoiseConfig> for CumulativeNoiseConfig {
    fn from(value: NoiseConfig) -> Self {
        Self {
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
}
