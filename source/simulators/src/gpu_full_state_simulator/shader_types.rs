// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(unused)]

use bytemuck::{Pod, Zeroable};

pub const MAX_QUBITS_PER_THREAD: u32 = 10;
pub const MAX_QUBITS_PER_WORKGROUP: u32 = 12;

// Could use an enum, but this avoids some boilerplate
pub mod ops {
    pub const ID: u32 = 0;
    pub const RESET: u32 = 1;
    pub const X: u32 = 2;
    pub const Y: u32 = 3;
    pub const Z: u32 = 4;
    pub const H: u32 = 5;
    pub const S: u32 = 6;
    pub const S_ADJ: u32 = 7;
    pub const T: u32 = 8;
    pub const T_ADJ: u32 = 9;
    pub const SX: u32 = 10;
    pub const SX_ADJ: u32 = 11;
    pub const RX: u32 = 12;
    pub const RY: u32 = 13;
    pub const RZ: u32 = 14;
    pub const CX: u32 = 15;
    pub const CZ: u32 = 16;
    pub const RXX: u32 = 17;
    pub const RYY: u32 = 18;
    pub const RZZ: u32 = 19;
    pub const CCX: u32 = 20;
    pub const MZ: u32 = 21;
    pub const MRESETZ: u32 = 22;
    pub const MEVERYZ: u32 = 23; // Implicit at end of circuit (for now)
    pub const SWAP: u32 = 24;
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Op {
    pub id: u32,
    pub q1: u32,
    pub q2: u32,
    pub q3: u32,    // For ccx
    pub angle: f32, // For rx, ry, rz, rzz
    pub _00r: f32,
    pub _00i: f32,
    pub _01r: f32,
    pub _01i: f32,
    pub _10r: f32,
    pub _10i: f32,
    pub _11r: f32,
    pub _11i: f32,
    // Pad out to 256 bytes for WebGPU dynamic buffer alignment
    pub padding: [u8; 204],
}

// safety check to make sure Op is the correct size with padding at compile time
const _: () = assert!(std::mem::size_of::<Op>() == 256);

/// Utility functions for creating 1-qubit gate operations
#[allow(clippy::pub_underscore_fields, clippy::used_underscore_binding)]
impl Op {
    /// Create a new Op with default values
    fn new_1q_gate(op_id: u32, qubit: u32) -> Self {
        Self {
            id: op_id,
            q1: qubit,
            q2: 0,
            q3: 0,
            angle: 0.0,
            _00r: 0.0,
            _00i: 0.0,
            _01r: 0.0,
            _01i: 0.0,
            _10r: 0.0,
            _10i: 0.0,
            _11r: 0.0,
            _11i: 0.0,
            padding: [0; 204],
        }
    }

    #[must_use]
    pub fn new_m_every_z_gate() -> Self {
        let mut op = Self::new_1q_gate(ops::MEVERYZ, 0);
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
        let inv_sqrt2 = 1.0 / (2.0_f32).sqrt(); // 1/√2
        op._00r = inv_sqrt2; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = inv_sqrt2; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = inv_sqrt2; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = -inv_sqrt2; // |1⟩⟨1| coefficient
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
        let pi_4 = std::f32::consts::PI / 4.0; // π/4
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = pi_4.cos(); // |1⟩⟨1| coefficient (real part of e^(iπ/4))
        op._11i = pi_4.sin(); // |1⟩⟨1| coefficient (imaginary part of e^(iπ/4))
        op
    }

    /// T† gate (π/8 adjoint): [[1, 0], [0, e^(-iπ/4)]]
    #[must_use]
    pub fn new_t_adj_gate(qubit: u32) -> Self {
        let mut op = Self::new_1q_gate(ops::T_ADJ, qubit);
        let neg_pi_4 = -std::f32::consts::PI / 4.0; // -π/4
        op._00r = 1.0; // |0⟩⟨0| coefficient
        op._00i = 0.0;
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = neg_pi_4.cos(); // |1⟩⟨1| coefficient (real part of e^(-iπ/4))
        op._11i = neg_pi_4.sin(); // |1⟩⟨1| coefficient (imaginary part of e^(-iπ/4))
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
        op.angle = angle;
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
        op.angle = angle;
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
        op.angle = angle;
        let half_angle = angle / 2.0;

        op._00r = (-half_angle).cos(); // |0⟩⟨0| coefficient (real part of e^(-iθ/2))
        op._00i = (-half_angle).sin(); // |0⟩⟨0| coefficient (imaginary part of e^(-iθ/2))
        op._01r = 0.0; // |0⟩⟨1| coefficient
        op._01i = 0.0;
        op._10r = 0.0; // |1⟩⟨0| coefficient
        op._10i = 0.0;
        op._11r = half_angle.cos(); // |1⟩⟨1| coefficient (real part of e^(iθ/2))
        op._11i = half_angle.sin(); // |1⟩⟨1| coefficient (imaginary part of e^(iθ/2))
        op
    }

    /// Create a new 2-qubit gate Op with default values
    fn new_2q_gate(op_id: u32, control: u32, target: u32) -> Self {
        Self {
            id: op_id,
            q1: control,
            q2: target,
            q3: 0,
            angle: 0.0,
            _00r: 0.0,
            _00i: 0.0,
            _01r: 0.0,
            _01i: 0.0,
            _10r: 0.0,
            _10i: 0.0,
            _11r: 0.0,
            _11i: 0.0,
            padding: [0; 204],
        }
    }

    /// CX gate (CNOT): Controlled-X gate
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_cx_gate(control: u32, target: u32) -> Self {
        Self::new_2q_gate(ops::CX, control, target)
    }

    /// CZ gate (Controlled-Z): Controlled-Z gate
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_cz_gate(control: u32, target: u32) -> Self {
        Self::new_2q_gate(ops::CZ, control, target)
    }

    /// RXX gate: Two-qubit rotation around XX
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_rxx_gate(angle: f32, qubit1: u32, qubit2: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::RXX, qubit1, qubit2);
        op.angle = angle;
        op
    }

    /// RYY gate: Two-qubit rotation around YY
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_ryy_gate(angle: f32, qubit1: u32, qubit2: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::RYY, qubit1, qubit2);
        op.angle = angle;
        op
    }

    /// RZZ gate: Two-qubit rotation around ZZ
    /// Matrix representation is handled in the shader for 2-qubit gates
    #[must_use]
    pub fn new_rzz_gate(angle: f32, qubit1: u32, qubit2: u32) -> Self {
        let mut op = Self::new_2q_gate(ops::RZZ, qubit1, qubit2);
        op.angle = angle;
        op
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Result {
    pub entry_idx: u32,
    pub probability: f32,
}
