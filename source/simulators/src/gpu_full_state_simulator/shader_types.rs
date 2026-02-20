// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::f32::consts::FRAC_1_SQRT_2;

use bytemuck::{Pod, Zeroable};

// ********** Constants used by the GPU shader code and structures *********

// Some of these values are to align with WebGPU default limits
// See https://gpuweb.github.io/gpuweb/#limits
pub const MAX_BUFFER_SIZE: usize = 1 << 30; // 1 GB limit due to some wgpu restrictions
pub const MAX_QUBIT_COUNT: i32 = 27; // 2^27 * 8 bytes per complex32 = 1 GB buffer limit
pub const MAX_QUBITS_PER_WORKGROUP: i32 = 18; // Max qubits to be processed by a single workgroup
pub const THREADS_PER_WORKGROUP: i32 = 32; // 32 gives good occupancy across various GPUs

// Once a shot is big enough to need multiple workgroups, what's the max number of workgroups possible
pub const MAX_PARTITIONED_WORKGROUPS: i32 = 1 << (MAX_QUBIT_COUNT - MAX_QUBITS_PER_WORKGROUP);
pub const MAX_SHOTS_PER_BATCH: i32 = 65535; // To align with max workgroups per dimension WebGPU default

// Round up circuit qubits if smaller to enable to optimizations re unrolling, etc.
// With min qubit count of 8, this means min 256 entries per shot. Spread across 32 threads = 8 entries per thread.
// With each iteration in each thread processing 2 or 4 entries, that means 2 or 4 iterations per thread minimum.
pub const MIN_QUBIT_COUNT: i32 = 8;
pub const SIZEOF_SHOTDATA: usize = std::mem::size_of::<ShotData>(); // Size of ShotData struct on the GPU in bytes

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
pub const MAX_CIRCUIT_OPS: i32 = (MAX_BUFFER_SIZE / std::mem::size_of::<Op>()) as i32;

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
pub const MAX_SHOT_ENTRIES: i32 = (MAX_BUFFER_SIZE / SIZEOF_SHOTDATA) as i32;

// ********* The below structure should be kept in sync with the WGSL shader code *********

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub batch_start_shot_id: i32,
    pub rng_seed: u32,
}

// The follow data is copied back from the GPU for diagnostics
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QubitProbabilities {
    zero: f32,
    one: f32,
}

// Each workgroup sums the probabilities for the entries it processed for each qubit
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct WorkgroupSums {
    qubits: [QubitProbabilities; MAX_QUBIT_COUNT as usize],
}

// Once the dispatch for the workgroup processing is done, the results from all workgroups
// for all active shots are collated here for final processing in the next prepare_op step.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct WorkgroupCollationBuffer {
    sums: [WorkgroupSums; MAX_PARTITIONED_WORKGROUPS as usize],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QubitProbabilityPerThread {
    zero: [f32; MAX_QUBIT_COUNT as usize],
    one: [f32; MAX_QUBIT_COUNT as usize],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QubitState {
    zero_probability: f32,
    one_probability: f32,
    heat: f32, // -1.0 = lost
    idle_since: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ShotData {
    pub shot_id: u32,
    pub next_op_idx: u32,
    pub rng_state: [u32; 6], // 6 x u32
    pub rand_pauli: f32,
    pub rand_damping: f32,
    pub rand_dephase: f32,
    pub rand_measure: f32,
    pub rand_loss: f32,
    pub op_type: u32,
    pub op_idx: u32,
    pub duration: f32,
    pub renormalize: f32,
    pub qubit_is_0_mask: u32,
    pub qubit_is_1_mask: u32,
    pub qubits_updated_last_op_mask: u32,
    pub qubit_state: [QubitState; MAX_QUBIT_COUNT as usize],
    pub unitary: [f32; 32],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct DiagnosticsData {
    pub error_code: u32,
    pub extra1: u32,
    pub extra2: f32,
    pub extra3: f32,
    pub shot: ShotData,
    pub op: Op,
    pub qubit_probabilities: [QubitProbabilityPerThread; THREADS_PER_WORKGROUP as usize],
    pub collation_buffer: WorkgroupCollationBuffer,
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum OpID {
    Id = 0,
    ResetZ = 1,
    X = 2,
    Y = 3,
    Z = 4,
    H = 5,
    S = 6,
    SAdj = 7,
    T = 8,
    TAdj = 9,
    Sx = 10,
    SxAdj = 11,
    Rx = 12,
    Ry = 13,
    Rz = 14,
    Cx = 15,
    Cz = 16,
    Rxx = 17,
    Ryy = 18,
    Rzz = 19,
    Ccx = 20,
    Mz = 21,
    MResetZ = 22,
    MEveryZ = 23,
    Swap = 24,
    Matrix = 25,
    Matrix2Q = 26,
    SAMPLE = 27, // Take a probabilistic sample of all qubits
    Move = 28,
    Cy = 29,
    PauliNoise1Q = 128,
    PauliNoise2Q = 129,
    LossNoise = 130,
    CorrelatedNoise = 131,
}

impl OpID {
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

impl From<OpID> for u32 {
    fn from(op_id: OpID) -> Self {
        op_id as u32
    }
}

impl TryFrom<u32> for OpID {
    type Error = u32;

    fn try_from(value: u32) -> core::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Id),
            1 => Ok(Self::ResetZ),
            2 => Ok(Self::X),
            3 => Ok(Self::Y),
            4 => Ok(Self::Z),
            5 => Ok(Self::H),
            6 => Ok(Self::S),
            7 => Ok(Self::SAdj),
            8 => Ok(Self::T),
            9 => Ok(Self::TAdj),
            10 => Ok(Self::Sx),
            11 => Ok(Self::SxAdj),
            12 => Ok(Self::Rx),
            13 => Ok(Self::Ry),
            14 => Ok(Self::Rz),
            15 => Ok(Self::Cx),
            16 => Ok(Self::Cz),
            17 => Ok(Self::Rxx),
            18 => Ok(Self::Ryy),
            19 => Ok(Self::Rzz),
            20 => Ok(Self::Ccx),
            21 => Ok(Self::Mz),
            22 => Ok(Self::MResetZ),
            23 => Ok(Self::MEveryZ),
            24 => Ok(Self::Swap),
            25 => Ok(Self::Matrix),
            26 => Ok(Self::Matrix2Q),
            27 => Ok(Self::SAMPLE),
            28 => Ok(Self::Move),
            29 => Ok(Self::Cy),
            128 => Ok(Self::PauliNoise1Q),
            129 => Ok(Self::PauliNoise2Q),
            130 => Ok(Self::LossNoise),
            131 => Ok(Self::CorrelatedNoise),
            invalid => Err(invalid),
        }
    }
}

// Operation identifiers used by the GPU shader.
pub mod ops {
    pub const ID: u32 = super::OpID::Id.as_u32();
    pub const RESETZ: u32 = super::OpID::ResetZ.as_u32();
    pub const X: u32 = super::OpID::X.as_u32();
    pub const Y: u32 = super::OpID::Y.as_u32();
    pub const Z: u32 = super::OpID::Z.as_u32();
    pub const H: u32 = super::OpID::H.as_u32();
    pub const S: u32 = super::OpID::S.as_u32();
    pub const S_ADJ: u32 = super::OpID::SAdj.as_u32();
    pub const T: u32 = super::OpID::T.as_u32();
    pub const T_ADJ: u32 = super::OpID::TAdj.as_u32();
    pub const SX: u32 = super::OpID::Sx.as_u32();
    pub const SX_ADJ: u32 = super::OpID::SxAdj.as_u32();
    pub const RX: u32 = super::OpID::Rx.as_u32();
    pub const RY: u32 = super::OpID::Ry.as_u32();
    pub const RZ: u32 = super::OpID::Rz.as_u32();
    pub const CX: u32 = super::OpID::Cx.as_u32();
    pub const CY: u32 = super::OpID::Cy.as_u32();
    pub const CZ: u32 = super::OpID::Cz.as_u32();
    pub const RXX: u32 = super::OpID::Rxx.as_u32();
    pub const RYY: u32 = super::OpID::Ryy.as_u32();
    pub const RZZ: u32 = super::OpID::Rzz.as_u32();
    pub const CCX: u32 = super::OpID::Ccx.as_u32();
    pub const MZ: u32 = super::OpID::Mz.as_u32();
    pub const MRESETZ: u32 = super::OpID::MResetZ.as_u32();
    pub const MEVERYZ: u32 = super::OpID::MEveryZ.as_u32(); // Implicit at end of circuit (for now)
    pub const SWAP: u32 = super::OpID::Swap.as_u32();
    pub const MATRIX: u32 = super::OpID::Matrix.as_u32();
    pub const MATRIX_2Q: u32 = super::OpID::Matrix2Q.as_u32();
    pub const SAMPLE: u32 = super::OpID::SAMPLE.as_u32(); // Take a probabilistic sample of all qubits
    pub const MOVE: u32 = super::OpID::Move.as_u32();
    pub const PAULI_NOISE_1Q: u32 = super::OpID::PauliNoise1Q.as_u32();
    pub const PAULI_NOISE_2Q: u32 = super::OpID::PauliNoise2Q.as_u32();
    pub const LOSS_NOISE: u32 = super::OpID::LossNoise.as_u32();
    pub const CORRELATED_NOISE: u32 = super::OpID::CorrelatedNoise.as_u32();

    #[must_use]
    pub fn is_1q_op(op_id: u32) -> bool {
        matches!(
            op_id,
            ID | X
                | Y
                | Z
                | H
                | S
                | S_ADJ
                | T
                | T_ADJ
                | SX
                | SX_ADJ
                | RX
                | RY
                | RZ
                | MZ
                | MRESETZ
                | MATRIX
                | MOVE
                | RESETZ
        )
    }

    #[must_use]
    pub fn is_2q_op(op_id: u32) -> bool {
        matches!(op_id, CX | CY | CZ | RXX | RYY | RZZ | SWAP | MATRIX_2Q)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Op {
    pub id: u32,
    pub q1: u32,
    pub q2: u32,
    pub q3: u32, // For ccx
    pub r00: f32,
    pub i00: f32,
    pub r01: f32,
    pub i01: f32,
    pub r02: f32,
    pub i02: f32,
    pub r03: f32,
    pub i03: f32,
    pub r10: f32,
    pub i10: f32,
    pub r11: f32,
    pub i11: f32,
    pub r12: f32,
    pub i12: f32,
    pub r13: f32,
    pub i13: f32,
    pub r20: f32,
    pub i20: f32,
    pub r21: f32,
    pub i21: f32,
    pub r22: f32,
    pub i22: f32,
    pub r23: f32,
    pub i23: f32,
    pub r30: f32,
    pub i30: f32,
    pub r31: f32,
    pub i31: f32,
    pub r32: f32,
    pub i32: f32,
    pub r33: f32,
    pub i33: f32,
}

// safety check to make sure Op is the correct size with padding at compile time
const _: () = assert!(std::mem::size_of::<Op>() == 144);

impl Default for Op {
    fn default() -> Self {
        Self {
            id: 0,
            q1: 0,
            q2: 0,
            q3: 0,
            r00: 0.0,
            i00: 0.0,
            r01: 0.0,
            i01: 0.0,
            r02: 0.0,
            i02: 0.0,
            r03: 0.0,
            i03: 0.0,
            r10: 0.0,
            i10: 0.0,
            r11: 0.0,
            i11: 0.0,
            r12: 0.0,
            i12: 0.0,
            r13: 0.0,
            i13: 0.0,
            r20: 0.0,
            i20: 0.0,
            r21: 0.0,
            i21: 0.0,
            r22: 0.0,
            i22: 0.0,
            r23: 0.0,
            i23: 0.0,
            r30: 0.0,
            i30: 0.0,
            r31: 0.0,
            i31: 0.0,
            r32: 0.0,
            i32: 0.0,
            r33: 0.0,
            i33: 0.0,
        }
    }
}
/// Utility functions for creating 1-qubit gate operations
#[allow(clippy::pub_underscore_fields, clippy::used_underscore_binding)]
impl Op {
    /// Create a new Op with default values
    #[must_use]
    pub fn new_1q_gate(op_id: u32, qubit: u32) -> Self {
        Self {
            id: op_id,
            q1: qubit,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn new_m_every_z_gate() -> Self {
        Self::new_1q_gate(ops::MEVERYZ, 0)
    }

    #[must_use]
    pub fn new_sample_gate(rand_value: f32) -> Self {
        let mut op = Self::new_1q_gate(ops::SAMPLE, 0);
        // Store the random value in the angle field
        op.r00 = rand_value;
        op
    }

    /// Identity gate: [[1, 0], [0, 1]]
    #[must_use]
    pub fn new_id_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::ID, qubit);
        op.r00 = 1.0; // |0⟩⟨0| coefficient (real)
        op.i00 = 0.0; // |0⟩⟨0| coefficient (imaginary)
        op.r01 = 0.0; // |0⟩⟨1| coefficient (real)
        op.i01 = 0.0; // |0⟩⟨1| coefficient (imaginary)
        op.r10 = 0.0; // |1⟩⟨0| coefficient (real)
        op.i10 = 0.0; // |1⟩⟨0| coefficient (imaginary)
        op.r11 = 1.0; // |1⟩⟨1| coefficient (real)
        op.i11 = 0.0; // |1⟩⟨1| coefficient (imaginary)
        op
    }

    #[must_use]
    pub fn new_move_gate(qubit: u32) -> Self {
        // Treat is like an identity for now
        let mut op = Self::new_id_gate(qubit);
        op.id = ops::MOVE;
        op
    }

    /// Measure-only gate: projects qubit onto the measured state without resetting
    /// Matrix will need to be determined in the simulator based on the measurement outcome
    #[must_use]
    pub fn new_mz_gate(qubit: u32, result_id: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::MZ, qubit);
        // Store the result id in q2
        op.q2 = result_id;
        // Matrix will need to be determined in the simulator based on the measurement outcome
        op
    }

    /// `ResetZ` gate (quantum channel): measures qubit internally and resets to |0⟩
    /// No measurement result is produced
    #[must_use]
    pub fn new_resetz_gate(qubit: u32) -> Self {
        Self::new_1q_gate(ops::RESETZ, qubit)
        // Matrix will need to be determined in the simulator based on the measurement outcome
    }

    /// `MResetZ` gate: measures qubit, stores result, and resets to |0⟩
    /// Matrix will need to be determined in the simulator based on the measurement outcome
    #[must_use]
    pub fn new_mresetz_gate(qubit: u32, result_id: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::MRESETZ, qubit);
        // Store the result id in q2
        op.q2 = result_id;
        // Matrix will need to be determined in the simulator based on the measurement outcome
        op
    }

    /// X gate (Pauli-X): [[0, 1], [1, 0]]
    #[must_use]
    pub fn new_x_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::X, qubit);
        op.r00 = 0.0; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 1.0; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = 1.0; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = 0.0; // |1⟩⟨1| coefficient
        op.i11 = 0.0;
        op
    }

    /// Y gate (Pauli-Y): [[0, -i], [i, 0]]
    #[must_use]
    pub fn new_y_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::Y, qubit);
        op.r00 = 0.0; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient (real part of -i)
        op.i01 = -1.0; // |0⟩⟨1| coefficient (imaginary part of -i)
        op.r10 = 0.0; // |1⟩⟨0| coefficient (real part of i)
        op.i10 = 1.0; // |1⟩⟨0| coefficient (imaginary part of i)
        op.r11 = 0.0; // |1⟩⟨1| coefficient
        op.i11 = 0.0;
        op
    }

    /// Z gate (Pauli-Z): [[1, 0], [0, -1]]
    #[must_use]
    pub fn new_z_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::Z, qubit);
        op.r00 = 1.0; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = 0.0; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = -1.0; // |1⟩⟨1| coefficient
        op.i11 = 0.0;
        op
    }

    /// H gate (Hadamard): [[1/√2, 1/√2], [1/√2, -1/√2]]
    #[must_use]
    pub fn new_h_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::H, qubit);
        op.r00 = FRAC_1_SQRT_2; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = FRAC_1_SQRT_2; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = FRAC_1_SQRT_2; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = -FRAC_1_SQRT_2; // |1⟩⟨1| coefficient
        op.i11 = 0.0;
        op
    }

    /// S gate (Phase): [[1, 0], [0, i]]
    #[must_use]
    pub fn new_s_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::S, qubit);
        op.r00 = 1.0; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = 0.0; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = 0.0; // |1⟩⟨1| coefficient (real part of i)
        op.i11 = 1.0; // |1⟩⟨1| coefficient (imaginary part of i)
        op
    }

    /// S† gate (Phase adjoint): [[1, 0], [0, -i]]
    #[must_use]
    pub fn new_s_adj_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::S_ADJ, qubit);
        op.r00 = 1.0; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = 0.0; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = 0.0; // |1⟩⟨1| coefficient (real part of -i)
        op.i11 = -1.0; // |1⟩⟨1| coefficient (imaginary part of -i)
        op
    }

    /// T gate (π/8): [[1, 0], [0, e^(iπ/4)]]
    #[must_use]
    pub fn new_t_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::T, qubit);
        op.r00 = 1.0; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = 0.0; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (real part of e^(iπ/4))
        op.i11 = FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (imaginary part of e^(iπ/4))
        op
    }

    /// T† gate (π/8 adjoint): [[1, 0], [0, e^(-iπ/4)]]
    #[must_use]
    pub fn new_t_adj_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::T_ADJ, qubit);
        op.r00 = 1.0; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = 0.0; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (real part of e^(-iπ/4))
        op.i11 = -FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (imaginary part of e^(-iπ/4))
        op
    }

    /// SX gate (√X): [[1+i, 1-i], [1-i, 1+i]]/2
    #[must_use]
    pub fn new_sx_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::SX, qubit);
        // SX = (1/2) * [[1+i, 1-i], [1-i, 1+i]]
        op.r00 = 0.5; // |0⟩⟨0| coefficient (real part of (1+i)/2)
        op.i00 = 0.5; // |0⟩⟨0| coefficient (imaginary part of (1+i)/2)
        op.r01 = 0.5; // |0⟩⟨1| coefficient (real part of (1-i)/2)
        op.i01 = -0.5; // |0⟩⟨1| coefficient (imaginary part of (1-i)/2)
        op.r10 = 0.5; // |1⟩⟨0| coefficient (real part of (1-i)/2)
        op.i10 = -0.5; // |1⟩⟨0| coefficient (imaginary part of (1-i)/2)
        op.r11 = 0.5; // |1⟩⟨1| coefficient (real part of (1+i)/2)
        op.i11 = 0.5; // |1⟩⟨1| coefficient (imaginary part of (1+i)/2)
        op
    }

    /// SX† gate (√X adjoint): [[1-i, 1+i], [1+i, 1-i]]/2
    #[must_use]
    pub fn new_sx_adj_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::SX_ADJ, qubit);
        // SX† = (1/2) * [[1-i, 1+i], [1+i, 1-i]]
        op.r00 = 0.5; // |0⟩⟨0| coefficient (real part of (1-i)/2)
        op.i00 = -0.5; // |0⟩⟨0| coefficient (imaginary part of (1-i)/2)
        op.r01 = 0.5; // |0⟩⟨1| coefficient (real part of (1+i)/2)
        op.i01 = 0.5; // |0⟩⟨1| coefficient (imaginary part of (1+i)/2)
        op.r10 = 0.5; // |1⟩⟨0| coefficient (real part of (1+i)/2)
        op.i10 = 0.5; // |1⟩⟨0| coefficient (imaginary part of (1+i)/2)
        op.r11 = 0.5; // |1⟩⟨1| coefficient (real part of (1-i)/2)
        op.i11 = -0.5; // |1⟩⟨1| coefficient (imaginary part of (1-i)/2)
        op
    }

    /// RX gate (rotation around X): [[cos(θ/2), -i*sin(θ/2)], [-i*sin(θ/2), cos(θ/2)]]
    #[must_use]
    pub fn new_rx_gate(angle: f32, qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::RX, qubit);
        let half_angle = angle / 2.0;
        let cos_half = half_angle.cos();
        let sin_half = half_angle.sin();

        op.r00 = cos_half; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient (real part of -i*sin(θ/2))
        op.i01 = -sin_half; // |0⟩⟨1| coefficient (imaginary part of -i*sin(θ/2))
        op.r10 = 0.0; // |1⟩⟨0| coefficient (real part of -i*sin(θ/2))
        op.i10 = -sin_half; // |1⟩⟨0| coefficient (imaginary part of -i*sin(θ/2))
        op.r11 = cos_half; // |1⟩⟨1| coefficient
        op.i11 = 0.0;
        op
    }

    /// RY gate (rotation around Y): [[cos(θ/2), -sin(θ/2)], [sin(θ/2), cos(θ/2)]]
    #[must_use]
    pub fn new_ry_gate(angle: f32, qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::RY, qubit);
        let half_angle = angle / 2.0;
        let cos_half = half_angle.cos();
        let sin_half = half_angle.sin();

        op.r00 = cos_half; // |0⟩⟨0| coefficient
        op.i00 = 0.0;
        op.r01 = -sin_half; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = sin_half; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = cos_half; // |1⟩⟨1| coefficient
        op.i11 = 0.0;
        op
    }

    /// RZ gate (rotation around Z): [[e^(-iθ/2), 0], [0, e^(iθ/2)]]
    #[must_use]
    pub fn new_rz_gate(angle: f32, qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::RZ, qubit);

        // In case we need to return to a uniform processing based on 2x2 matrix
        // let half_angle = angle / 2.0;
        op.r00 = 1.0;
        op.i00 = 0.0;
        op.r01 = 0.0; // |0⟩⟨1| coefficient
        op.i01 = 0.0;
        op.r10 = 0.0; // |1⟩⟨0| coefficient
        op.i10 = 0.0;
        op.r11 = angle.cos(); // |1⟩⟨1| coefficient (real part of e^(iθ))
        op.i11 = angle.sin(); // |1⟩⟨1| coefficient (imaginary part of e^(iθ))
        op
    }

    #[must_use]
    pub fn new_pauli_noise_1q(qubit: u32, p_x: f32, p_y: f32, p_z: f32) -> Self {
        let mut op = Self::new_1q_gate(ops::PAULI_NOISE_1Q, qubit);
        op.r00 = 1.0 - (p_x + p_y + p_z);
        op.r01 = p_x;
        op.r02 = p_y;
        op.r03 = p_z;
        op
    }

    #[must_use]
    #[allow(clippy::similar_names)]
    #[allow(clippy::too_many_arguments)]
    pub fn new_pauli_noise_2q(
        q1: u32,
        q2: u32,
        p_ix: f32,
        p_iy: f32,
        p_iz: f32,
        p_xi: f32,
        p_xx: f32,
        p_xy: f32,
        p_xz: f32,
        p_yi: f32,
        p_yx: f32,
        p_yy: f32,
        p_yz: f32,
        p_zi: f32,
        p_zx: f32,
        p_zy: f32,
        p_zz: f32,
    ) -> Self {
        let mut op = Self::new_2q_gate(ops::PAULI_NOISE_2Q, q1, q2);
        op.r00 = 1.0
            - (p_ix
                + p_iy
                + p_iz
                + p_xi
                + p_xx
                + p_xy
                + p_xz
                + p_yi
                + p_yx
                + p_yy
                + p_yz
                + p_zi
                + p_zx
                + p_zy
                + p_zz);
        op.r01 = p_ix;
        op.r02 = p_iy;
        op.r03 = p_iz;
        op.r10 = p_xi;
        op.r11 = p_xx;
        op.r12 = p_xy;
        op.r13 = p_xz;
        op.r20 = p_yi;
        op.r21 = p_yx;
        op.r22 = p_yy;
        op.r23 = p_yz;
        op.r30 = p_zi;
        op.r31 = p_zx;
        op.r32 = p_zy;
        op.r33 = p_zz;
        op
    }

    #[must_use]
    pub fn new_loss_noise(qubit: u32, p_loss: f32) -> Self {
        let mut op = Self::new_1q_gate(ops::LOSS_NOISE, qubit);
        op.r00 = p_loss;
        op
    }

    /// Create a new 2-qubit gate Op with default values
    #[must_use]
    pub fn new_2q_gate(op_id: u32, control: u32, target: u32) -> Self {
        Self {
            id: op_id,
            q1: control,
            q2: target,
            ..Default::default()
        }
    }

    /// CX gate (CNOT): Controlled-X gate
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_cx_gate(control: u32, target: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::CX, control, target);
        op.r00 = 1.0;
        op.r11 = 1.0;
        op.r23 = 1.0;
        op.r32 = 1.0;
        op
    }

    /// CY gate (Controlled-Y): Controlled-Y gate
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_cy_gate(control: u32, target: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::CY, control, target);
        op.r00 = 1.0;
        op.r11 = 1.0;
        op.i23 = -1.0;
        op.i32 = 1.0;
        op
    }

    /// CZ gate (Controlled-Z): Controlled-Z gate
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_cz_gate(control: u32, target: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::CZ, control, target);
        op.r00 = 1.0;
        op.r11 = 1.0;
        op.r22 = 1.0;
        op.r33 = -1.0;
        op
    }

    #[must_use]
    pub fn new_swap_gate(a: u32, b: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::SWAP, a, b);
        op.r00 = 1.0;
        op.r12 = 1.0;
        op.r21 = 1.0;
        op.r33 = 1.0;
        op
    }

    /// RXX gate: Two-qubit rotation around XX
    /// Matrix: exp(-i*θ/2 * (X ⊗ X))
    /// [[cos(θ/2), 0, 0, -i*sin(θ/2)],
    ///  [0, cos(θ/2), -i*sin(θ/2), 0],
    ///  [0, -i*sin(θ/2), cos(θ/2), 0],
    ///  [-i*sin(θ/2), 0, 0, cos(θ/2)]]
    #[must_use]
    pub fn new_rxx_gate(angle: f32, qubit1: u32, qubit2: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::RXX, qubit1, qubit2);
        let half_angle = angle / 2.0;
        let cos_half = half_angle.cos();
        let sin_half = half_angle.sin();

        // |00⟩⟨00| and |11⟩⟨11| coefficients
        op.r00 = cos_half;
        op.i00 = 0.0;
        op.r33 = cos_half;
        op.i33 = 0.0;

        // |01⟩⟨01| and |10⟩⟨10| coefficients
        op.r11 = cos_half;
        op.i11 = 0.0;
        op.r22 = cos_half;
        op.i22 = 0.0;

        // |00⟩⟨11| and |11⟩⟨00| coefficients (-i*sin(θ/2))
        op.r03 = 0.0;
        op.i03 = -sin_half;
        op.r30 = 0.0;
        op.i30 = -sin_half;

        // |01⟩⟨10| and |10⟩⟨01| coefficients (-i*sin(θ/2))
        op.r12 = 0.0;
        op.i12 = -sin_half;
        op.r21 = 0.0;
        op.i21 = -sin_half;

        // All other coefficients are 0 (already set by new_2q_gate)
        op
    }

    /// RYY gate: Two-qubit rotation around YY
    /// Matrix: exp(-i*θ/2 * (Y ⊗ Y))
    /// [[cos(θ/2), 0, 0, i*sin(θ/2)],
    ///  [0, cos(θ/2), -i*sin(θ/2), 0],
    ///  [0, -i*sin(θ/2), cos(θ/2), 0],
    ///  [i*sin(θ/2), 0, 0, cos(θ/2)]]
    #[must_use]
    pub fn new_ryy_gate(angle: f32, qubit1: u32, qubit2: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::RYY, qubit1, qubit2);
        let half_angle = angle / 2.0;
        let cos_half = half_angle.cos();
        let sin_half = half_angle.sin();

        // |00⟩⟨00| and |11⟩⟨11| coefficients
        op.r00 = cos_half;
        op.i00 = 0.0;
        op.r33 = cos_half;
        op.i33 = 0.0;

        // |01⟩⟨01| and |10⟩⟨10| coefficients
        op.r11 = cos_half;
        op.i11 = 0.0;
        op.r22 = cos_half;
        op.i22 = 0.0;

        // |00⟩⟨11| and |11⟩⟨00| coefficients (i*sin(θ/2))
        op.r03 = 0.0;
        op.i03 = sin_half;
        op.r30 = 0.0;
        op.i30 = sin_half;

        // |01⟩⟨10| and |10⟩⟨01| coefficients (-i*sin(θ/2))
        op.r12 = 0.0;
        op.i12 = -sin_half;
        op.r21 = 0.0;
        op.i21 = -sin_half;

        // All other coefficients are 0 (already set by new_2q_gate)
        op
    }

    /// RZZ gate: Two-qubit rotation around ZZ
    /// Matrix: exp(-i*θ/2 * (Z ⊗ Z))
    /// [[1, 0,       0, 0],
    ///  [0, e^(i*θ), 0, 0],
    ///  [0, 0, e^(i*θ), 0],
    ///  [0, 0,       0, 1]]
    #[must_use]
    pub fn new_rzz_gate(angle: f32, qubit1: u32, qubit2: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::RZZ, qubit1, qubit2);

        // |00⟩⟨00| coefficient (e^(-i*θ/2))
        op.r00 = 1.0;

        // |01⟩⟨01| coefficient (e^(i*θ))
        op.r11 = angle.cos();
        op.i11 = angle.sin();

        // |10⟩⟨10| coefficient (e^(i*θ))
        op.r22 = angle.cos();
        op.i22 = angle.sin();

        // |11⟩⟨11| coefficient (e^(-i*θ/2))
        op.r33 = 1.0;

        // All off-diagonal elements are 0 (already set by new_2q_gate)
        op
    }

    #[must_use]
    pub fn new_correlated_noise_gate(noise_table: u32, qubits: &[u32]) -> Self {
        // Qubit count will never exceed 32
        #[allow(clippy::cast_possible_truncation)]
        let mut op = Self::new_2q_gate(ops::CORRELATED_NOISE, noise_table, qubits.len() as u32);

        // Store qubit ids in the matrix elements
        for (i, &q) in qubits.iter().enumerate() {
            // The range of qubit ids is limited to 32 for now, so f32 can represent them exactly
            #[allow(clippy::cast_precision_loss)]
            match i {
                0 => op.r00 = q as f32,
                1 => op.i00 = q as f32,
                2 => op.r01 = q as f32,
                3 => op.i01 = q as f32,
                4 => op.r02 = q as f32,
                5 => op.i02 = q as f32,
                6 => op.r03 = q as f32,
                7 => op.i03 = q as f32,
                8 => op.r10 = q as f32,
                9 => op.i10 = q as f32,
                10 => op.r11 = q as f32,
                11 => op.i11 = q as f32,
                12 => op.r12 = q as f32,
                13 => op.i12 = q as f32,
                14 => op.r13 = q as f32,
                15 => op.i13 = q as f32,
                16 => op.r20 = q as f32,
                17 => op.i20 = q as f32,
                18 => op.r21 = q as f32,
                19 => op.i21 = q as f32,
                20 => op.r22 = q as f32,
                21 => op.i22 = q as f32,
                22 => op.r23 = q as f32,
                23 => op.i23 = q as f32,
                24 => op.r30 = q as f32,
                25 => op.i30 = q as f32,
                26 => op.r31 = q as f32,
                27 => op.i31 = q as f32,
                28 => op.r32 = q as f32,
                29 => op.i32 = q as f32,
                30 => op.r33 = q as f32,
                31 => op.i33 = q as f32,
                _ => panic!("More than 32 qubits passed to the correlated noise operation"), // Limited to 32 qubits
            }
        }
        op
    }

    /// Custom 1-qubit operation with arbitrary matrix elements
    /// K = [[_00r + i*_00i, _01r + i*_01i],
    ///      [_10r + i*_10i, _11r + i*_11i]]
    /// Used for quantum noise models and non-unitary operations
    ///
    /// # Arguments
    /// * `qubit` - Target qubit
    /// * `m00` - Matrix element (0,0) as (real, imaginary) tuple
    /// * `m01` - Matrix element (0,1) as (real, imaginary) tuple
    /// * `m10` - Matrix element (1,0) as (real, imaginary) tuple
    /// * `m11` - Matrix element (1,1) as (real, imaginary) tuple
    #[must_use]
    pub fn new_matrix_gate(
        qubit: u32,
        m00: (f32, f32),
        m01: (f32, f32),
        m10: (f32, f32),
        m11: (f32, f32),
    ) -> Self {
        let mut op = Self::new_1q_gate(ops::MATRIX, qubit);
        op.r00 = m00.0;
        op.i00 = m00.1;
        op.r01 = m01.0;
        op.i01 = m01.1;
        op.r10 = m10.0;
        op.i10 = m10.1;
        op.r11 = m11.0;
        op.i11 = m11.1;
        op
    }

    /// Custom 2-qubit operation with arbitrary 4x4 matrix elements
    /// K = [[_00r+i*_00i, _01r+i*_01i, _02r+i*_02i, _03r+i*_03i],
    ///      [_10r+i*_10i, _11r+i*_11i, _12r+i*_12i, _13r+i*_13i],
    ///      [_20r+i*_20i, _21r+i*_21i, _22r+i*_22i, _23r+i*_23i],
    ///      [_30r+i*_30i, _31r+i*_31i, _32r+i*_32i, _33r+i*_33i]]
    /// Used for quantum noise models and non-unitary 2-qubit operations
    ///
    /// # Arguments
    /// * `qubit1` - First target qubit
    /// * `qubit2` - Second target qubit
    /// * `row0` - First row as array of (real, imaginary) tuples [m00, m01, m02, m03]
    /// * `row1` - Second row as array of (real, imaginary) tuples [m10, m11, m12, m13]
    /// * `row2` - Third row as array of (real, imaginary) tuples [m20, m21, m22, m23]
    /// * `row3` - Fourth row as array of (real, imaginary) tuples [m30, m31, m32, m33]
    #[must_use]
    pub fn new_matrix_2q_gate(
        qubit1: u32,
        qubit2: u32,
        row0: [(f32, f32); 4],
        row1: [(f32, f32); 4],
        row2: [(f32, f32); 4],
        row3: [(f32, f32); 4],
    ) -> Self {
        let mut op = Self::new_2q_gate(ops::MATRIX_2Q, qubit1, qubit2);

        // Standard matrix layout: Row 0 -> _00, _01, _02, _03
        op.r00 = row0[0].0;
        op.i00 = row0[0].1;
        op.r01 = row0[1].0;
        op.i01 = row0[1].1;
        op.r02 = row0[2].0;
        op.i02 = row0[2].1;
        op.r03 = row0[3].0;
        op.i03 = row0[3].1;

        // Standard matrix layout: Row 1 -> _10, _11, _12, _13
        op.r10 = row1[0].0;
        op.i10 = row1[0].1;
        op.r11 = row1[1].0;
        op.i11 = row1[1].1;
        op.r12 = row1[2].0;
        op.i12 = row1[2].1;
        op.r13 = row1[3].0;
        op.i13 = row1[3].1;

        // Standard matrix layout: Row 2 -> _20, _21, _22, _23
        op.r20 = row2[0].0;
        op.i20 = row2[0].1;
        op.r21 = row2[1].0;
        op.i21 = row2[1].1;
        op.r22 = row2[2].0;
        op.i22 = row2[2].1;
        op.r23 = row2[3].0;
        op.i23 = row2[3].1;

        // Standard matrix layout: Row 3 -> _30, _31, _32, _33
        op.r30 = row3[0].0;
        op.i30 = row3[0].1;
        op.r31 = row3[1].0;
        op.i31 = row3[1].1;
        op.r32 = row3[2].0;
        op.i32 = row3[2].1;
        op.r33 = row3[3].0;
        op.i33 = row3[3].1;

        op
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Result {
    pub entry_idx: u32,
    pub probability: f32,
}
