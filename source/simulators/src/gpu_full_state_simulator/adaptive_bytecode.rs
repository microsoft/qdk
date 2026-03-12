// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! GPU bytecode types for the Adaptive Profile QIR interpreter.
//!
//! These types define the bytecode instruction format that is uploaded to GPU
//! buffers and interpreted per-shot by the WGSL shader. Values must stay in
//! sync with the Python `_adaptive_opcodes.py` module.

use bytemuck::{Pod, Zeroable};

// ---------------------------------------------------------------------------
// Instruction struct — 32 bytes (8 × u32), aligned for GPU access
// ---------------------------------------------------------------------------

/// GPU bytecode instruction.
///
/// Layout:
/// - `opcode`: packed word — bits\[7:0\]=primary, bits\[15:8\]=sub/condition, bits\[23:16\]=flags
/// - `dst`: destination register or branch target
/// - `src0`, `src1`: source registers or immediates
/// - `aux0`–`aux3`: auxiliary fields (gate index, block ids, side-table offsets, etc.)
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct Instruction {
    pub opcode: u32,
    pub dst: u32,
    pub src0: u32,
    pub src1: u32,
    pub aux0: u32,
    pub aux1: u32,
    pub aux2: u32,
    pub aux3: u32,
}

const _: () = assert!(std::mem::size_of::<Instruction>() == 32);

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

impl Instruction {
    /// Create an instruction from an 8-tuple (matching Python emission format).
    #[must_use]
    pub const fn from_tuple(t: (u32, u32, u32, u32, u32, u32, u32, u32)) -> Self {
        Self {
            opcode: t.0,
            dst: t.1,
            src0: t.2,
            src1: t.3,
            aux0: t.4,
            aux1: t.5,
            aux2: t.6,
            aux3: t.7,
        }
    }

    /// Extract the primary opcode (bits [7:0]).
    #[must_use]
    pub const fn primary_opcode(&self) -> u8 {
        (self.opcode & 0xFF) as u8
    }

    /// Extract the sub-opcode / condition code (bits [15:8]).
    #[must_use]
    pub const fn sub_opcode(&self) -> u8 {
        ((self.opcode >> 8) & 0xFF) as u8
    }

    /// Extract the flags word (bits [23:16]).
    #[must_use]
    pub const fn flags(&self) -> u8 {
        ((self.opcode >> 16) & 0xFF) as u8
    }

    /// Check whether a specific flag bit is set.
    #[must_use]
    pub const fn has_flag(&self, flag: u32) -> bool {
        self.opcode & flag != 0
    }
}

// ---------------------------------------------------------------------------
// Block table entry (matches Python `blocks` tuple)
// ---------------------------------------------------------------------------

/// A basic block descriptor.
///
/// `(block_id, instruction_offset, instruction_count, predecessor_count)`
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct BlockEntry {
    pub block_id: u32,
    pub instr_offset: u32,
    pub instr_count: u32,
    pub pred_count: u32,
}

// ---------------------------------------------------------------------------
// Function table entry
// ---------------------------------------------------------------------------

/// An IR-defined function descriptor.
///
/// `(entry_block_id, param_count, param_base_reg, reserved)`
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
pub struct FunctionEntry {
    pub entry_block_id: u32,
    pub param_count: u32,
    pub param_base_reg: u32,
    pub reserved: u32,
}

// ---------------------------------------------------------------------------
// Adaptive program aggregate
// ---------------------------------------------------------------------------

use crate::shader_types::{Op, ops};

/// Stores a parsed adaptive program ready for GPU upload.
#[derive(Debug)]
pub struct AdaptiveProgram {
    /// Bytecode instructions to upload to the GPU.
    pub bytecode: Vec<Instruction>,
    /// GPU block table: `[instr_offset, instr_count]` per block, indexed by block ID.
    pub block_table: Vec<[u32; 2]>,
    /// Function table: `[entry_block, param_count, param_base_reg, reserved]` per function.
    pub function_table: Vec<[u32; 4]>,
    /// Quantum op pool (full `Op` structs with expanded unitaries).
    pub quantum_ops: Vec<Op>,
    /// Phi side table: `[predecessor_block_id, value_register]` entries.
    pub phi_entries: Vec<[u32; 2]>,
    /// Switch side table: `[match_value, target_block]` entries.
    pub switch_cases: Vec<[u32; 2]>,
    /// Call argument register indices.
    pub call_args: Vec<u32>,
    /// Number of qubits used by the program.
    pub num_qubits: u32,
    /// Number of result registers used by the program.
    pub num_results: u32,
    /// Number of virtual registers used by the program.
    pub num_registers: u32,
    /// Entry block ID for the program.
    pub entry_block: u32,
}

/// Build a pool of [`Op`] structs from compact `(op_id, q1, q2, q3, angle)` tuples.
///
/// Maps each `OpID` integer to the corresponding `Op::new_*` constructor, expanding
/// the unitary matrix for use on the GPU.
#[must_use]
pub fn build_op_pool(compact_ops: &[(u32, u32, u32, u32, f64)]) -> Vec<Op> {
    compact_ops
        .iter()
        .map(|&(op_id, q1, q2, _q3, angle)| {
            #[allow(clippy::cast_possible_truncation)]
            let angle_f32 = angle as f32;
            match op_id {
                ops::ID => Op::new_id_gate(q1),
                ops::RESETZ => Op::new_resetz_gate(q1),
                ops::X => Op::new_x_gate(q1),
                ops::Y => Op::new_y_gate(q1),
                ops::Z => Op::new_z_gate(q1),
                ops::H => Op::new_h_gate(q1),
                ops::S => Op::new_s_gate(q1),
                ops::S_ADJ => Op::new_s_adj_gate(q1),
                ops::T => Op::new_t_gate(q1),
                ops::T_ADJ => Op::new_t_adj_gate(q1),
                ops::SX => Op::new_sx_gate(q1),
                ops::SX_ADJ => Op::new_sx_adj_gate(q1),
                ops::RX => Op::new_rx_gate(angle_f32, q1),
                ops::RY => Op::new_ry_gate(angle_f32, q1),
                ops::RZ => Op::new_rz_gate(angle_f32, q1),
                ops::CX => Op::new_cx_gate(q1, q2),
                ops::CY => Op::new_cy_gate(q1, q2),
                ops::CZ => Op::new_cz_gate(q1, q2),
                ops::RXX => Op::new_rxx_gate(angle_f32, q1, q2),
                ops::RYY => Op::new_ryy_gate(angle_f32, q1, q2),
                ops::RZZ => Op::new_rzz_gate(angle_f32, q1, q2),
                ops::SWAP => Op::new_swap_gate(q1, q2),
                ops::MZ => Op::new_mz_gate(q1, q2),
                ops::MRESETZ => Op::new_mresetz_gate(q1, q2),
                ops::MOVE => Op::new_move_gate(q1),
                _ => panic!("Unknown op_id in adaptive quantum op pool: {op_id}"),
            }
        })
        .collect()
}
