// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(unused)]

use std::f32::consts::FRAC_1_SQRT_2;

use bytemuck::{Pod, Zeroable};

pub const MAX_QUBITS_PER_THREAD: u32 = 10;
pub const MAX_QUBITS_PER_WORKGROUP: u32 = 12;

#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum OpID {
    Id = 0,
    Reset = 1,
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
    PauliNoise1Q = 128,
    PauliNoise2Q = 129,
    LossNoise = 130,
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
            1 => Ok(Self::Reset),
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
            128 => Ok(Self::PauliNoise1Q),
            129 => Ok(Self::PauliNoise2Q),
            130 => Ok(Self::LossNoise),
            invalid => Err(invalid),
        }
    }
}

// Operation identifiers used by the GPU shader.
pub mod ops {
    pub const ID: u32 = super::OpID::Id.as_u32();
    pub const RESET: u32 = super::OpID::Reset.as_u32();
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

    #[must_use]
    pub fn is_1q_op(op_id: u32) -> bool {
        matches!(
            op_id,
            ID | RESET
                | X
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
        )
    }

    #[must_use]
    pub fn is_2q_op(op_id: u32) -> bool {
        matches!(op_id, CX | CZ | RXX | RYY | RZZ | SWAP | MATRIX_2Q)
    }

    #[must_use]
    pub fn is_noise_op(op_id: u32) -> bool {
        matches!(op_id, PAULI_NOISE_1Q | PAULI_NOISE_2Q | LOSS_NOISE)
    }
}

pub(super) const OP_PADDING: usize = 100;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Op {
    pub id: u32,
    pub q1: u32,
    pub q2: u32,
    pub q3: u32, // For ccx
    pub rzr: f32,
    pub rzi: f32,
    pub _00r: f32,
    pub _00i: f32,
    pub _01r: f32,
    pub _01i: f32,
    pub _02r: f32,
    pub _02i: f32,
    pub _03r: f32,
    pub _03i: f32,
    pub _10r: f32,
    pub _10i: f32,
    pub _11r: f32,
    pub _11i: f32,
    pub _12r: f32,
    pub _12i: f32,
    pub _13r: f32,
    pub _13i: f32,
    pub _20r: f32,
    pub _20i: f32,
    pub _21r: f32,
    pub _21i: f32,
    pub _22r: f32,
    pub _22i: f32,
    pub _23r: f32,
    pub _23i: f32,
    pub _30r: f32,
    pub _30i: f32,
    pub _31r: f32,
    pub _31i: f32,
    pub _32r: f32,
    pub _32i: f32,
    pub _33r: f32,
    pub _33i: f32,
    pub angle: f32,
    pub padding: [u8; OP_PADDING],
}

// safety check to make sure Op is the correct size with padding at compile time
const _: () = assert!(std::mem::size_of::<Op>() == 256);

impl Default for Op {
    fn default() -> Self {
        Self {
            id: 0,
            q1: 0,
            q2: 0,
            q3: 0,
            rzr: 0.0,
            rzi: 0.0,
            _00r: 0.0,
            _00i: 0.0,
            _01r: 0.0,
            _01i: 0.0,
            _02r: 0.0,
            _02i: 0.0,
            _03r: 0.0,
            _03i: 0.0,
            _10r: 0.0,
            _10i: 0.0,
            _11r: 0.0,
            _11i: 0.0,
            _12r: 0.0,
            _12i: 0.0,
            _13r: 0.0,
            _13i: 0.0,
            _20r: 0.0,
            _20i: 0.0,
            _21r: 0.0,
            _21i: 0.0,
            _22r: 0.0,
            _22i: 0.0,
            _23r: 0.0,
            _23i: 0.0,
            _30r: 0.0,
            _30i: 0.0,
            _31r: 0.0,
            _31i: 0.0,
            _32r: 0.0,
            _32i: 0.0,
            _33r: 0.0,
            _33i: 0.0,
            angle: 0.0,
            padding: [0; OP_PADDING],
        }
    }
}
/// Utility functions for creating 1-qubit gate operations
#[allow(clippy::pub_underscore_fields, clippy::used_underscore_binding)]
impl Op {
    /// Create a new Op with default values
    fn new_1q_gate(op_id: u32, qubit: u32) -> Self {
        Self {
            id: op_id,
            q1: qubit,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn new_m_every_z_gate() -> Self {
        let mut op = Self::new_1q_gate(ops::MEVERYZ, 0);
        op
    }

    #[must_use]
    pub fn new_sample_gate(rand_value: f32) -> Self {
        let mut op = Self::new_1q_gate(ops::SAMPLE, 0);
        // Store the random value in the angle field
        op.angle = rand_value;
        op
    }

    /// Identity gate: [[1, 0], [0, 1]]
    #[must_use]
    pub fn new_id_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::ID, qubit);
        op._00r = 1.0; // |0⟩⟨0| coefficient (real)
        op._00i = 0.0; // |0⟩⟨0| coefficient (imaginary)
        op._01r = 0.0; // |0⟩⟨1| coefficient (real)
        op._01i = 0.0; // |0⟩⟨1| coefficient (imaginary)
        op._10r = 0.0; // |1⟩⟨0| coefficient (real)
        op._10i = 0.0; // |1⟩⟨0| coefficient (imaginary)
        op._11r = 1.0; // |1⟩⟨1| coefficient (real)
        op._11i = 0.0; // |1⟩⟨1| coefficient (imaginary)
        op
    }

    #[must_use]
    pub fn new_move_gate(qubit: u32) -> Self {
        // Treat is like an identity for now
        let mut op = Self::new_id_gate(qubit);
        op.id = ops::MOVE;
        op
    }

    /// Reset gate: maps |0⟩ to |0⟩ and |1⟩ to |0⟩
    /// Note: This is used with a qubit id of u32::MAX to indicate a reset of the entire system
    #[must_use]
    pub fn new_reset_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::RESET, qubit);
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 1.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = 0.0; // |1⟩⟨1| coefficient
        op._11i = 0.0;
        op
    }

    /// Reset gate: maps |0⟩ to |0⟩ and |1⟩ to |0⟩
    /// Note: This is used with a qubit id of u32::MAX to indicate a reset of the entire system
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
        op._00r = 0.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 1.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 1.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = 0.0; // |1⟩⟨1| coefficient
        op._11i = 0.0;
        op
    }

    /// Y gate (Pauli-Y): [[0, -i], [i, 0]]
    #[must_use]
    pub fn new_y_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::Y, qubit);
        op._00r = 0.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient (real part of -i)
        op._01i = -1.0; // |0⟩⟨1| coefficient (imaginary part of -i)
        op._10r = 0.0; // |1⟩⟨0| coefficient (real part of i)
        op._10i = 1.0; // |1⟩⟨0| coefficient (imaginary part of i)
        op._11r = 0.0; // |1⟩⟨1| coefficient
        op._11i = 0.0;
        op
    }

    /// Z gate (Pauli-Z): [[1, 0], [0, -1]]
    #[must_use]
    pub fn new_z_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::Z, qubit);
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = -1.0; // |1⟩⟨1| coefficient
        op._11i = 0.0;
        op
    }

    /// H gate (Hadamard): [[1/√2, 1/√2], [1/√2, -1/√2]]
    #[must_use]
    pub fn new_h_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::H, qubit);
        op._00r = FRAC_1_SQRT_2; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = FRAC_1_SQRT_2; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = FRAC_1_SQRT_2; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = -FRAC_1_SQRT_2; // |1⟩⟨1| coefficient
        op._11i = 0.0;
        op
    }

    /// S gate (Phase): [[1, 0], [0, i]]
    #[must_use]
    pub fn new_s_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::S, qubit);
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = 0.0; // |1⟩⟨1| coefficient (real part of i)
        op._11i = 1.0; // |1⟩⟨1| coefficient (imaginary part of i)
        op
    }

    /// S† gate (Phase adjoint): [[1, 0], [0, -i]]
    #[must_use]
    pub fn new_s_adj_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::S_ADJ, qubit);
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = 0.0; // |1⟩⟨1| coefficient (real part of -i)
        op._11i = -1.0; // |1⟩⟨1| coefficient (imaginary part of -i)
        op
    }

    /// T gate (π/8): [[1, 0], [0, e^(iπ/4)]]
    #[must_use]
    pub fn new_t_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::T, qubit);
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (real part of e^(iπ/4))
        op._11i = FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (imaginary part of e^(iπ/4))
        op
    }

    /// T† gate (π/8 adjoint): [[1, 0], [0, e^(-iπ/4)]]
    #[must_use]
    pub fn new_t_adj_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::T_ADJ, qubit);
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (real part of e^(-iπ/4))
        op._11i = -FRAC_1_SQRT_2; // |1⟩⟨1| coefficient (imaginary part of e^(-iπ/4))
        op
    }

    /// SX gate (√X): [[1+i, 1-i], [1-i, 1+i]]/2
    #[must_use]
    pub fn new_sx_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::SX, qubit);
        // SX = (1/2) * [[1+i, 1-i], [1-i, 1+i]]
        op._00r = 0.5; // |0⟩⟨0| coefficient (real part of (1+i)/2)
        op._00i = 0.5; // |0⟩⟨0| coefficient (imaginary part of (1+i)/2)
        op._01r = 0.5; // |0⟩⟨1| coefficient (real part of (1-i)/2)
        op._01i = -0.5; // |0⟩⟨1| coefficient (imaginary part of (1-i)/2)
        op._10r = 0.5; // |1⟩⟨0| coefficient (real part of (1-i)/2)
        op._10i = -0.5; // |1⟩⟨0| coefficient (imaginary part of (1-i)/2)
        op._11r = 0.5; // |1⟩⟨1| coefficient (real part of (1+i)/2)
        op._11i = 0.5; // |1⟩⟨1| coefficient (imaginary part of (1+i)/2)
        op
    }

    /// SX† gate (√X adjoint): [[1-i, 1+i], [1+i, 1-i]]/2
    #[must_use]
    pub fn new_sx_adj_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::SX_ADJ, qubit);
        // SX† = (1/2) * [[1-i, 1+i], [1+i, 1-i]]
        op._00r = 0.5; // |0⟩⟨0| coefficient (real part of (1-i)/2)
        op._00i = -0.5; // |0⟩⟨0| coefficient (imaginary part of (1-i)/2)
        op._01r = 0.5; // |0⟩⟨1| coefficient (real part of (1+i)/2)
        op._01i = 0.5; // |0⟩⟨1| coefficient (imaginary part of (1+i)/2)
        op._10r = 0.5; // |1⟩⟨0| coefficient (real part of (1+i)/2)
        op._10i = 0.5; // |1⟩⟨0| coefficient (imaginary part of (1+i)/2)
        op._11r = 0.5; // |1⟩⟨1| coefficient (real part of (1-i)/2)
        op._11i = -0.5; // |1⟩⟨1| coefficient (imaginary part of (1-i)/2)
        op
    }

    /// RX gate (rotation around X): [[cos(θ/2), -i*sin(θ/2)], [-i*sin(θ/2), cos(θ/2)]]
    #[must_use]
    pub fn new_rx_gate(angle: f32, qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::RX, qubit);
        let half_angle = angle / 2.0;
        let cos_half = half_angle.cos();
        let sin_half = half_angle.sin();

        op._00r = cos_half; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient (real part of -i*sin(θ/2))
        op._01i = -sin_half; // |0⟩⟨1| coefficient (imaginary part of -i*sin(θ/2))
        op._10r = 0.0; // |1⟩⟨0| coefficient (real part of -i*sin(θ/2))
        op._10i = -sin_half; // |1⟩⟨0| coefficient (imaginary part of -i*sin(θ/2))
        op._11r = cos_half; // |1⟩⟨1| coefficient
        op._11i = 0.0;
        op
    }

    /// RY gate (rotation around Y): [[cos(θ/2), -sin(θ/2)], [sin(θ/2), cos(θ/2)]]
    #[must_use]
    pub fn new_ry_gate(angle: f32, qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::RY, qubit);
        let half_angle = angle / 2.0;
        let cos_half = half_angle.cos();
        let sin_half = half_angle.sin();

        op._00r = cos_half; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = -sin_half; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = sin_half; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = cos_half; // |1⟩⟨1| coefficient
        op._11i = 0.0;
        op
    }

    /// RZ gate (rotation around Z): [[e^(-iθ/2), 0], [0, e^(iθ/2)]]
    #[must_use]
    pub fn new_rz_gate(angle: f32, qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::RZ, qubit);

        // The shader uses an optimization relying on these two values
        op.rzr = angle.cos();
        op.rzi = angle.sin();

        // In case we need to return to a uniform processing based on 2x2 matrix
        // let half_angle = angle / 2.0;
        // op._00r = (-half_angle).cos(); // |0⟩⟨0| coefficient (real part of e^(-iθ/2))
        // op._00i = (-half_angle).sin(); // |0⟩⟨0| coefficient (imaginary part of e^(-iθ/2))
        op._00r = 1.0;
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = op.rzr; // |1⟩⟨1| coefficient (real part of e^(iθ/2))
        op._11i = op.rzi; // |1⟩⟨1| coefficient (imaginary part of e^(iθ/2))
        // op._11r = half_angle.cos(); // |1⟩⟨1| coefficient (real part of e^(iθ/2))
        // op._11i = half_angle.sin(); // |1⟩⟨1| coefficient (imaginary part of e^(iθ/2))
        op
    }

    #[must_use]
    pub fn new_pauli_noise_1q(qubit: u32, p_x: f32, p_y: f32, p_z: f32) -> Self {
        let mut op = Self::new_1q_gate(ops::PAULI_NOISE_1Q, qubit);
        op._00r = p_x;
        op._01r = p_y;
        op._02r = p_z;
        op
    }

    #[must_use]
    pub fn new_pauli_noise_2q(q1: u32, q2: u32, p_x: f32, p_y: f32, p_z: f32) -> Self {
        let mut op = Self::new_2q_gate(ops::PAULI_NOISE_2Q, q1, q2);
        op._00r = p_x;
        op._01r = p_y;
        op._02r = p_z;
        op
    }

    #[must_use]
    pub fn new_loss_noise(qubit: u32, p_loss: f32) -> Self {
        let mut op = Self::new_1q_gate(ops::LOSS_NOISE, qubit);
        op._00r = p_loss;
        op
    }

    /// Create a new 2-qubit gate Op with default values
    fn new_2q_gate(op_id: u32, control: u32, target: u32) -> Self {
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
        op._00r = 1.0;
        op._11r = 1.0;
        op._23r = 1.0;
        op._32r = 1.0;
        op
    }

    /// CZ gate (Controlled-Z): Controlled-Z gate
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_cz_gate(control: u32, target: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::CZ, control, target);
        op._00r = 1.0;
        op._11r = 1.0;
        op._22r = 1.0;
        op._33r = -1.0;
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
        op._00r = cos_half;
        op._00i = 0.0;
        op._33r = cos_half;
        op._33i = 0.0;

        // |01⟩⟨01| and |10⟩⟨10| coefficients
        op._11r = cos_half;
        op._11i = 0.0;
        op._22r = cos_half;
        op._22i = 0.0;

        // |00⟩⟨11| and |11⟩⟨00| coefficients (-i*sin(θ/2))
        op._03r = 0.0;
        op._03i = -sin_half;
        op._30r = 0.0;
        op._30i = -sin_half;

        // |01⟩⟨10| and |10⟩⟨01| coefficients (-i*sin(θ/2))
        op._12r = 0.0;
        op._12i = -sin_half;
        op._21r = 0.0;
        op._21i = -sin_half;

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
        op._00r = cos_half;
        op._00i = 0.0;
        op._33r = cos_half;
        op._33i = 0.0;

        // |01⟩⟨01| and |10⟩⟨10| coefficients
        op._11r = cos_half;
        op._11i = 0.0;
        op._22r = cos_half;
        op._22i = 0.0;

        // |00⟩⟨11| and |11⟩⟨00| coefficients (i*sin(θ/2))
        op._03r = 0.0;
        op._03i = sin_half;
        op._30r = 0.0;
        op._30i = sin_half;

        // |01⟩⟨10| and |10⟩⟨01| coefficients (-i*sin(θ/2))
        op._12r = 0.0;
        op._12i = -sin_half;
        op._21r = 0.0;
        op._21i = -sin_half;

        // All other coefficients are 0 (already set by new_2q_gate)
        op
    }

    /// RZZ gate: Two-qubit rotation around ZZ
    /// Matrix: exp(-i*θ/2 * (Z ⊗ Z))
    /// [[e^(-i*θ/2), 0, 0, 0],
    ///  [0, e^(i*θ/2), 0, 0],
    ///  [0, 0, e^(i*θ/2), 0],
    ///  [0, 0, 0, e^(-i*θ/2)]]
    #[must_use]
    pub fn new_rzz_gate(angle: f32, qubit1: u32, qubit2: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::RZZ, qubit1, qubit2);
        let half_angle = angle / 2.0;

        // |00⟩⟨00| coefficient (e^(-i*θ/2))
        op._00r = (-half_angle).cos();
        op._00i = (-half_angle).sin();

        // |01⟩⟨01| coefficient (e^(i*θ/2))
        op._11r = half_angle.cos();
        op._11i = half_angle.sin();

        // |10⟩⟨10| coefficient (e^(i*θ/2))
        op._22r = half_angle.cos();
        op._22i = half_angle.sin();

        // |11⟩⟨11| coefficient (e^(-i*θ/2))
        op._33r = (-half_angle).cos();
        op._33i = (-half_angle).sin();

        // All off-diagonal elements are 0 (already set by new_2q_gate)
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
        op._00r = m00.0;
        op._00i = m00.1;
        op._01r = m01.0;
        op._01i = m01.1;
        op._10r = m10.0;
        op._10i = m10.1;
        op._11r = m11.0;
        op._11i = m11.1;
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
        op._00r = row0[0].0;
        op._00i = row0[0].1;
        op._01r = row0[1].0;
        op._01i = row0[1].1;
        op._02r = row0[2].0;
        op._02i = row0[2].1;
        op._03r = row0[3].0;
        op._03i = row0[3].1;

        // Standard matrix layout: Row 1 -> _10, _11, _12, _13
        op._10r = row1[0].0;
        op._10i = row1[0].1;
        op._11r = row1[1].0;
        op._11i = row1[1].1;
        op._12r = row1[2].0;
        op._12i = row1[2].1;
        op._13r = row1[3].0;
        op._13i = row1[3].1;

        // Standard matrix layout: Row 2 -> _20, _21, _22, _23
        op._20r = row2[0].0;
        op._20i = row2[0].1;
        op._21r = row2[1].0;
        op._21i = row2[1].1;
        op._22r = row2[2].0;
        op._22i = row2[2].1;
        op._23r = row2[3].0;
        op._23i = row2[3].1;

        // Standard matrix layout: Row 3 -> _30, _31, _32, _33
        op._30r = row3[0].0;
        op._30i = row3[0].1;
        op._31r = row3[1].0;
        op._31i = row3[1].1;
        op._32r = row3[2].0;
        op._32i = row3[2].1;
        op._33r = row3[3].0;
        op._33i = row3[3].1;

        op
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Result {
    pub entry_idx: u32,
    pub probability: f32,
}
