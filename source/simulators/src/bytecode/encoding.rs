// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The encoding of the bytecode's instructions: the opcodes, the flags
//! that mark an operand as an immediate, the comparison predicates, the
//! quantum operations' ids, and the sentinel of a call whose result is
//! dropped. They must stay in sync with the Python `_adaptive_bytecode.py`
//! and the WGSL `gpu_statevector_shaders.wgsl` shader.

// Flags (pre-shifted to bit 16+)
pub(crate) const FLAG_SRC0_IMM: u64 = 1 << 16;
pub(crate) const FLAG_SRC1_IMM: u64 = 1 << 17;
pub(crate) const FLAG_DST_IMM: u64 = 1 << 18;
pub(crate) const FLAG_AUX0_IMM: u64 = 1 << 19;
pub(crate) const FLAG_AUX1_IMM: u64 = 1 << 20;
pub(crate) const FLAG_AUX2_IMM: u64 = 1 << 21;
pub(crate) const FLAG_AUX3_IMM: u64 = 1 << 22;

// Control flow
pub(crate) const OP_NOP: u8 = 0x00;
pub(crate) const OP_RET: u8 = 0x02;
pub(crate) const OP_JUMP: u8 = 0x04;
pub(crate) const OP_BRANCH: u8 = 0x05;
pub(crate) const OP_SWITCH: u8 = 0x06;
pub(crate) const OP_CALL: u8 = 0x07;
pub(crate) const OP_CALL_RETURN: u8 = 0x08;

// Quantum
pub(crate) const OP_QUANTUM_GATE: u8 = 0x10;
pub(crate) const OP_MEASURE: u8 = 0x11;
pub(crate) const OP_RESET: u8 = 0x12;
pub(crate) const OP_READ_RESULT: u8 = 0x13;
pub(crate) const OP_RECORD_OUTPUT: u8 = 0x14;
pub(crate) const OP_READ_LOSS: u8 = 0x15;
pub(crate) const OP_PEEK_LOSS: u8 = 0x16;
pub(crate) const OP_READOUT_NOISE: u8 = 0x17;
pub(crate) const OP_WRITE_RESULT: u8 = 0x18;

// Integer arithmetic
pub(crate) const OP_ADD: u8 = 0x20;
pub(crate) const OP_SUB: u8 = 0x21;
pub(crate) const OP_MUL: u8 = 0x22;
pub(crate) const OP_UDIV: u8 = 0x23;
pub(crate) const OP_SDIV: u8 = 0x24;
pub(crate) const OP_UREM: u8 = 0x25;
pub(crate) const OP_SREM: u8 = 0x26;

// Bitwise / shift
pub(crate) const OP_AND: u8 = 0x28;
pub(crate) const OP_OR: u8 = 0x29;
pub(crate) const OP_XOR: u8 = 0x2A;
pub(crate) const OP_SHL: u8 = 0x2B;
pub(crate) const OP_LSHR: u8 = 0x2C;
pub(crate) const OP_ASHR: u8 = 0x2D;

// Comparison
pub(crate) const OP_ICMP: u8 = 0x30;
pub(crate) const OP_FCMP: u8 = 0x31;

// Float arithmetic
pub(crate) const OP_FADD: u8 = 0x38;
pub(crate) const OP_FSUB: u8 = 0x39;
pub(crate) const OP_FMUL: u8 = 0x3A;
pub(crate) const OP_FDIV: u8 = 0x3B;
pub(crate) const OP_FREM: u8 = 0x3C;

// Type conversion
pub(crate) const OP_ZEXT: u8 = 0x40;
pub(crate) const OP_SEXT: u8 = 0x41;
pub(crate) const OP_TRUNC: u8 = 0x42;
pub(crate) const OP_FPEXT: u8 = 0x43;
pub(crate) const OP_FPTRUNC: u8 = 0x44;
pub(crate) const OP_INTTOPTR: u8 = 0x45;
pub(crate) const OP_FPTOSI: u8 = 0x46;
pub(crate) const OP_SITOFP: u8 = 0x47;
pub(crate) const OP_FPTOUI: u8 = 0x48;
pub(crate) const OP_UITOFP: u8 = 0x49;

// SSA / data movement
pub(crate) const OP_PHI: u8 = 0x50;
pub(crate) const OP_SELECT: u8 = 0x51;
pub(crate) const OP_MOV: u8 = 0x52;
pub(crate) const OP_CONST: u8 = 0x53;

// Memory operations
pub(crate) const OP_ALLOCA: u8 = 0x60;
pub(crate) const OP_LOAD: u8 = 0x61;
pub(crate) const OP_STORE: u8 = 0x62;
pub(crate) const OP_GEP: u8 = 0x63;

// ICmp condition codes (sub-opcode)
pub(crate) const ICMP_EQ: u8 = 0;
pub(crate) const ICMP_NE: u8 = 1;
pub(crate) const ICMP_SLT: u8 = 2;
pub(crate) const ICMP_SLE: u8 = 3;
pub(crate) const ICMP_SGT: u8 = 4;
pub(crate) const ICMP_SGE: u8 = 5;
pub(crate) const ICMP_ULT: u8 = 6;
pub(crate) const ICMP_ULE: u8 = 7;
pub(crate) const ICMP_UGT: u8 = 8;
pub(crate) const ICMP_UGE: u8 = 9;

// FCmp condition codes (sub-opcode): LLVM's 16 predicates, which the Python
// pass maps all of. Q# emits only the six ordered ones (`OEQ` to `ONE`), the
// only ones `run_shot` evaluates; the others come from QIR made outside the
// QDK (another front end, or LLVM's optimizer, which may turn `!(a < b)`
// into `uge`), which the pass accepts too, so the machine evaluates all 16.
pub(crate) const FCMP_FALSE: u8 = 0;
pub(crate) const FCMP_OEQ: u8 = 1;
pub(crate) const FCMP_OGT: u8 = 2;
pub(crate) const FCMP_OGE: u8 = 3;
pub(crate) const FCMP_OLT: u8 = 4;
pub(crate) const FCMP_OLE: u8 = 5;
pub(crate) const FCMP_ONE: u8 = 6;
pub(crate) const FCMP_ORD: u8 = 7;
pub(crate) const FCMP_UNO: u8 = 8;
pub(crate) const FCMP_UEQ: u8 = 9;
pub(crate) const FCMP_UGT: u8 = 10;
pub(crate) const FCMP_UGE: u8 = 11;
pub(crate) const FCMP_ULT: u8 = 12;
pub(crate) const FCMP_ULE: u8 = 13;
pub(crate) const FCMP_UNE: u8 = 14;
pub(crate) const FCMP_TRUE: u8 = 15;

// Quantum op IDs — must match `shader_types.rs` `OpID` and `GATE_MAP` in
// `_adaptive_pass.py`.
//
// Op 0 is the GPU simulator's internal `Id`, a noise carrier. It is
// deliberately not a `QirInstruction`, whose doc says why.
pub(crate) const OPID_RESETZ: u64 = 1;
pub(crate) const OPID_X: u64 = 2;
pub(crate) const OPID_Y: u64 = 3;
pub(crate) const OPID_Z: u64 = 4;
pub(crate) const OPID_H: u64 = 5;
pub(crate) const OPID_S: u64 = 6;
pub(crate) const OPID_S_ADJ: u64 = 7;
pub(crate) const OPID_T: u64 = 8;
pub(crate) const OPID_T_ADJ: u64 = 9;
pub(crate) const OPID_SX: u64 = 10;
pub(crate) const OPID_SX_ADJ: u64 = 11;
pub(crate) const OPID_RX: u64 = 12;
pub(crate) const OPID_RY: u64 = 13;
pub(crate) const OPID_RZ: u64 = 14;
pub(crate) const OPID_CX: u64 = 15;
pub(crate) const OPID_CZ: u64 = 16;
pub(crate) const OPID_RXX: u64 = 17;
pub(crate) const OPID_RYY: u64 = 18;
pub(crate) const OPID_RZZ: u64 = 19;
pub(crate) const OPID_MZ: u64 = 21;
pub(crate) const OPID_MRESETZ: u64 = 22;
pub(crate) const OPID_SWAP: u64 = 24;
pub(crate) const OPID_MOVE: u64 = 28;
pub(crate) const OPID_CY: u64 = 29;
pub(crate) const OPID_CORRELATED_NOISE: u64 = 131;

// Sentinel
pub(crate) const VOID_RETURN: u64 = 0xFFFF_FFFF_FFFF_FFFF;
