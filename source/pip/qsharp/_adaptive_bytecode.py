# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Shared opcode constants for the Adaptive Profile QIR bytecode interpreter.

These constants define the bytecode encoding used by the Python AdaptiveProfilePass
(emitter) and the Rust GPU receiver. Values must stay in sync with the Rust
``bytecode.rs`` module and the WGSL interpreter.

Opcode word layout::

    bits [7:0]   = primary opcode
    bits [15:8]  = sub-opcode / condition code
    bits [23:16] = flags

Compose via bitwise OR: ``opcode | (sub << 8) | flag``
Example: ``OP_ICMP | (ICMP_SLE << 8) | FLAG_SRC1_IMM``
"""

# ── Flags (pre-shifted to bit 16+) ──────────────────────────────────────────
FLAG_DST_IMM = 1 << 18  # dst  field is an immediate value, not a register
FLAG_SRC0_IMM = 1 << 16  # src0 field is an immediate value, not a register
FLAG_SRC1_IMM = 1 << 17  # src1 field is an immediate value, not a register
FLAG_AUX0_IMM = 1 << 19  # aux0 field is an immediate value, not a register
FLAG_AUX1_IMM = 1 << 20  # aux1 field is an immediate value, not a register
FLAG_AUX2_IMM = 1 << 21  # aux2 field is an immediate value, not a register
FLAG_AUX3_IMM = 1 << 22  # aux3 field is an immediate value, not a register

FLAG_FLOAT = 1 << 23  # operation uses float semantics


# ── Control Flow ─────────────────────────────────────────────────────────────
OP_NOP = 0x00
OP_RET = 0x02
OP_JUMP = 0x04
OP_BRANCH = 0x05
OP_SWITCH = 0x06
OP_CALL = 0x07
OP_CALL_RETURN = 0x08

# ── Quantum ──────────────────────────────────────────────────────────────────
OP_QUANTUM_GATE = 0x10
OP_MEASURE = 0x11
OP_RESET = 0x12
OP_READ_RESULT = 0x13
OP_RECORD_OUTPUT = 0x14

# ── Integer Arithmetic ───────────────────────────────────────────────────────
OP_ADD = 0x20
OP_SUB = 0x21
OP_MUL = 0x22
OP_UDIV = 0x23
OP_SDIV = 0x24
OP_UREM = 0x25
OP_SREM = 0x26

# ── Bitwise / Shift ─────────────────────────────────────────────────────────
OP_AND = 0x28
OP_OR = 0x29
OP_XOR = 0x2A
OP_SHL = 0x2B
OP_LSHR = 0x2C
OP_ASHR = 0x2D

# ── Comparison ───────────────────────────────────────────────────────────────
OP_ICMP = 0x30
OP_FCMP = 0x31

# ── Float Arithmetic ─────────────────────────────────────────────────────────
OP_FADD = 0x38
OP_FSUB = 0x39
OP_FMUL = 0x3A
OP_FDIV = 0x3B

# ── Type Conversion ──────────────────────────────────────────────────────────
OP_ZEXT = 0x40
OP_SEXT = 0x41
OP_TRUNC = 0x42
OP_FPEXT = 0x43
OP_FPTRUNC = 0x44
OP_INTTOPTR = 0x45
OP_FPTOSI = 0x46
OP_SITOFP = 0x47

# ── SSA / Data Movement ─────────────────────────────────────────────────────
OP_PHI = 0x50
OP_SELECT = 0x51
OP_MOV = 0x52
OP_CONST = 0x53

# ── ICmp condition codes (sub-opcode, placed in bits[15:8] via << 8) ─────────
# Reference: https://llvm.org/docs/LangRef.html#icmp-instruction
ICMP_EQ = 0
ICMP_NE = 1
ICMP_SLT = 2
ICMP_SLE = 3
ICMP_SGT = 4
ICMP_SGE = 5
ICMP_ULT = 6
ICMP_ULE = 7
ICMP_UGT = 8
ICMP_UGE = 9

# ── FCmp condition codes ─────────────────────────────────────────────────────
# Reference: https://llvm.org/docs/LangRef.html#fcmp-instruction
FCMP_FALSE = 0
FCMP_OEQ = 1
FCMP_OGT = 2
FCMP_OGE = 3
FCMP_OLT = 4
FCMP_OLE = 5
FCMP_ONE = 6
FCMP_ORD = 7
FCMP_UNO = 8
FCMP_UEQ = 9
FCMP_UGT = 10
FCMP_UGE = 11
FCMP_ULT = 12
FCMP_ULE = 13
FCMP_UNE = 14
FCMP_TRUE = 15

# ── Register type tags ───────────────────────────────────────────────────────
REG_TYPE_BOOL = 0
REG_TYPE_I32 = 1
REG_TYPE_I64 = 2
REG_TYPE_F32 = 3
REG_TYPE_F64 = 4
REG_TYPE_PTR = 5

# ── Sentinel values ──────────────────────────────────────────────────────────
VOID_RETURN = 0xFFFFFFFF  # Function does not have a return value.
