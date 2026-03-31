// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Bytecode types for the Adaptive Profile QIR interpreter.
//!
//! Values must stay in sync with the Python `_adaptive_opcodes.py` module.

use bytemuck::{Pod, Zeroable};
use num_traits::Unsigned;

// We need these for uploading data to the GPU.
unsafe impl Pod for Instruction<u32> {}
unsafe impl Pod for Block<u32> {}
unsafe impl Pod for Function<u32> {}
unsafe impl Pod for PhiNodeEntry<u32> {}
unsafe impl Pod for SwitchCase<u32> {}

/// Stores a parsed adaptive program.
#[derive(Debug)]
pub struct AdaptiveProgram<Word: Unsigned> {
    /// Number of qubits used by the program.
    pub num_qubits: u32,
    /// Number of result registers used by the program.
    pub num_results: u32,
    /// Number of virtual registers used by the program.
    pub num_registers: u32,
    /// Entry block ID for the program.
    pub entry_block: Word,
    /// Bytecode instructions.
    pub instructions: Vec<Instruction<Word>>,
    /// Block table: indexed by block ID.
    pub block_table: Vec<Block<Word>>,
    /// Function table.
    pub function_table: Vec<Function<Word>>,
    /// Phi side table: `[predecessor_block_id, value_register]` entries.
    pub phi_entries: Vec<PhiNodeEntry<Word>>,
    /// Switch side table: `[match_value, target_block]` entries.
    pub switch_cases: Vec<SwitchCase<Word>>,
    /// Call argument register indices.
    pub call_args: Vec<Word>,
    /// Quantum op pool (full `Op` structs with expanded unitaries).
    pub quantum_ops: Vec<Op<Word>>,
}

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
#[derive(Copy, Clone, Debug, Default, Zeroable)]
pub struct Instruction<Word> {
    pub opcode: Word,
    pub dst: Word,
    pub src0: Word,
    pub src1: Word,
    pub aux0: Word,
    pub aux1: Word,
    pub aux2: Word,
    pub aux3: Word,
}

const _: () = assert!(std::mem::size_of::<Instruction<u32>>() == 32);
const _: () = assert!(std::mem::size_of::<Instruction<u64>>() == 64);

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

impl<Word> Instruction<Word> {
    /// Create an [`Instruction`] from an 8-tuple (matching Python emission format).
    #[must_use]
    pub fn from_tuple(t: (Word, Word, Word, Word, Word, Word, Word, Word)) -> Self {
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
}

impl Instruction<u64> {
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
    pub const fn has_flag(&self, flag: u64) -> bool {
        self.opcode & flag != 0
    }
}

/// A basic block descriptor.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Zeroable)]
pub struct Block<Word> {
    pub instr_offset: Word,
    pub instr_count: Word,
}

impl<Word> Block<Word> {
    /// Create a [`Block`] from an 2-tuple (matching Python emission format).
    #[must_use]
    pub fn from_tuple(t: (Word, Word)) -> Self {
        Self {
            instr_offset: t.0,
            instr_count: t.1,
        }
    }
}

/// An IR-defined function descriptor.
///
/// `(entry_block_id, param_count, param_base_reg, reserved)`
///
/// The `reserved` field pads the struct to 16 bytes so it matches
/// the GPU shader layout (`vec4<u32>`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Zeroable)]
pub struct Function<Word> {
    pub entry_block_id: Word,
    pub param_count: Word,
    pub param_base_reg: Word,
    pub reserved: Word,
}

impl<Word: Default> Function<Word> {
    /// Create a [`Function`] from a 3-tuple (matching Python emission format).
    #[must_use]
    pub fn from_tuple(t: (Word, Word, Word)) -> Self {
        Self {
            entry_block_id: t.0,
            param_count: t.1,
            param_base_reg: t.2,
            reserved: Word::default(),
        }
    }
}

/// A component of a phi node.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Zeroable)]
pub struct PhiNodeEntry<Word> {
    block_id: Word,
    val_reg: Word,
}

impl<Word> PhiNodeEntry<Word> {
    /// Create a [`PhiNodeEntry`] from an 2-tuple (matching Python emission format).
    #[must_use]
    pub fn from_tuple(t: (Word, Word)) -> Self {
        Self {
            block_id: t.0,
            val_reg: t.1,
        }
    }
}

/// A switch case.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Zeroable)]
pub struct SwitchCase<Word> {
    case_val: Word,
    target_block: Word,
}

impl<Word> SwitchCase<Word> {
    /// Create a [`SwitchCase`] from an 2-tuple (matching Python emission format).
    #[must_use]
    pub fn from_tuple(t: (Word, Word)) -> Self {
        Self {
            case_val: t.0,
            target_block: t.1,
        }
    }
}

#[derive(Debug)]
pub struct Op<Word> {
    pub op_id: Word,
    pub q1: Word,
    pub q2: Word,
    pub q3: Word,
    pub angle: f64,
}

impl<Word> Op<Word> {
    #[must_use]
    pub fn from_tuple(t: (Word, Word, Word, Word, f64)) -> Self {
        Self {
            op_id: t.0,
            q1: t.1,
            q2: t.2,
            q3: t.3,
            angle: t.4,
        }
    }
}
