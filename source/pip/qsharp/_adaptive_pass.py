# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""AdaptiveProfilePass: walks Adaptive Profile QIR and emits the intermediate
format consumed by the Rust GPU receiver.

Unlike ``AggregateGatesPass`` (which subclasses ``pyqir.QirModuleVisitor`` and
only dispatches CALL instructions), this pass iterates basic blocks and
instructions directly so it can handle *all* LLVM IR opcodes required by the
Adaptive Profile specification.
"""

from __future__ import annotations

import struct
from typing import Any, Dict, List, Optional, Tuple

import pyqir

from ._adaptive_opcodes import (
    DYN_QUBIT_SENTINEL,
    FLAG_FLOAT,
    FLAG_SRC0_IMM,
    FLAG_SRC1_IMM,
    ICMP_EQ,
    ICMP_NE,
    ICMP_SGE,
    ICMP_SGT,
    ICMP_SLE,
    ICMP_SLT,
    ICMP_UGE,
    ICMP_UGT,
    ICMP_ULE,
    ICMP_ULT,
    FCMP_FALSE,
    FCMP_OEQ,
    FCMP_OGE,
    FCMP_OGT,
    FCMP_OLE,
    FCMP_OLT,
    FCMP_ONE,
    FCMP_ORD,
    FCMP_TRUE,
    FCMP_UEQ,
    FCMP_UGE as FCMP_UGE_CODE,
    FCMP_UGT as FCMP_UGT_CODE,
    FCMP_ULE as FCMP_ULE_CODE,
    FCMP_ULT as FCMP_ULT_CODE,
    FCMP_UNE,
    FCMP_UNO,
    OP_ADD,
    OP_AND,
    OP_ASHR,
    OP_BRANCH,
    OP_CALL,
    OP_CALL_RETURN,
    OP_CONST,
    OP_FADD,
    OP_FCMP,
    OP_FDIV,
    OP_FMUL,
    OP_FSUB,
    OP_ICMP,
    OP_INTTOPTR,
    OP_JUMP,
    OP_LSHR,
    OP_MEASURE,
    OP_MOV,
    OP_MUL,
    OP_OR,
    OP_PHI,
    OP_QUANTUM_GATE,
    OP_READ_RESULT,
    OP_RECORD_OUTPUT,
    OP_RESET,
    OP_RET,
    OP_SDIV,
    OP_SELECT,
    OP_SEXT,
    OP_SHL,
    OP_SREM,
    OP_SUB,
    OP_SWITCH,
    OP_TRUNC,
    OP_UDIV,
    OP_UREM,
    OP_XOR,
    OP_ZEXT,
    OP_FPTOSI,
    OP_SITOFP,
    OP_FPEXT,
    OP_FPTRUNC,
    REG_TYPE_BOOL,
    REG_TYPE_F32,
    REG_TYPE_F64,
    REG_TYPE_I32,
    REG_TYPE_I64,
    REG_TYPE_PTR,
)

# ---------------------------------------------------------------------------
# Gate name → OpID mapping (must match shader_types.rs OpID enum)
# ---------------------------------------------------------------------------

GATE_MAP: Dict[str, int] = {
    "x": 2,
    "y": 3,
    "z": 4,
    "h": 5,
    "s": 6,
    "sadj": 7,
    "t": 8,
    "tadj": 9,
    "sx": 10,
    "sxadj": 11,
    "rx": 12,
    "ry": 13,
    "rz": 14,
    "cnot": 15,
    "cx": 15,
    "cz": 16,
    "cy": 29,
    "rxx": 17,
    "ryy": 18,
    "rzz": 19,
    "ccx": 20,
    "swap": 24,
}

# Gates that perform measurement (OpID → OP_MEASURE)
MEASURE_GATES = {"mz": 21, "mresetz": 22, "m": 21}

# Gates that perform reset
RESET_GATES = {"reset": 1}

# Rotation gates that take an angle parameter as first argument
ROTATION_GATES = {"rx", "ry", "rz", "rxx", "ryy", "rzz"}

# Two-qubit gates
TWO_QUBIT_GATES = {"cx", "cnot", "cz", "cy", "swap", "rxx", "ryy", "rzz"}

# Three-qubit gates
THREE_QUBIT_GATES = {"ccx"}

# ---------------------------------------------------------------------------
# ICmp / FCmp predicate mappings
# ---------------------------------------------------------------------------

ICMP_MAP = {
    pyqir.IntPredicate.EQ: ICMP_EQ,
    pyqir.IntPredicate.NE: ICMP_NE,
    pyqir.IntPredicate.SLT: ICMP_SLT,
    pyqir.IntPredicate.SLE: ICMP_SLE,
    pyqir.IntPredicate.SGT: ICMP_SGT,
    pyqir.IntPredicate.SGE: ICMP_SGE,
    pyqir.IntPredicate.ULT: ICMP_ULT,
    pyqir.IntPredicate.ULE: ICMP_ULE,
    pyqir.IntPredicate.UGT: ICMP_UGT,
    pyqir.IntPredicate.UGE: ICMP_UGE,
}

FCMP_MAP = {
    pyqir.FloatPredicate.FALSE: FCMP_FALSE,
    pyqir.FloatPredicate.OEQ: FCMP_OEQ,
    pyqir.FloatPredicate.OGT: FCMP_OGT,
    pyqir.FloatPredicate.OGE: FCMP_OGE,
    pyqir.FloatPredicate.OLT: FCMP_OLT,
    pyqir.FloatPredicate.OLE: FCMP_OLE,
    pyqir.FloatPredicate.ONE: FCMP_ONE,
    pyqir.FloatPredicate.ORD: FCMP_ORD,
    pyqir.FloatPredicate.UNO: FCMP_UNO,
    pyqir.FloatPredicate.UEQ: FCMP_UEQ,
    pyqir.FloatPredicate.UGT: FCMP_UGT_CODE,
    pyqir.FloatPredicate.UGE: FCMP_UGE_CODE,
    pyqir.FloatPredicate.ULT: FCMP_ULT_CODE,
    pyqir.FloatPredicate.ULE: FCMP_ULE_CODE,
    pyqir.FloatPredicate.UNE: FCMP_UNE,
    pyqir.FloatPredicate.TRUE: FCMP_TRUE,
}


class AdaptiveProfilePass:
    """Walks Adaptive Profile QIR and emits the intermediate format for Rust/GPU."""

    def __init__(self) -> None:
        # Output tables
        self.blocks: List[Tuple[int, int, int, int]] = []
        self.instructions: List[Tuple[int, int, int, int, int, int, int, int]] = []
        self.quantum_ops: List[Tuple[int, int, int, int, float]] = []
        self.functions: List[Tuple[int, int, int, int]] = []
        self.phi_entries: List[Tuple[int, int]] = []
        self.switch_cases: List[Tuple[int, int]] = []
        self.call_args: List[int] = []
        self.labels: List[str] = []
        self.register_types: List[int] = []

        # Internal tracking
        self._next_reg: int = 0
        self._next_block: int = 0
        self._next_qop: int = 0
        self._value_to_reg: Dict[Any, int] = {}  # pyqir.Value id → register ID
        self._block_to_id: Dict[Any, int] = {}  # pyqir.BasicBlock id → block ID
        self._func_to_id: Dict[str, int] = {}  # function name → function ID
        self._current_func_is_entry: bool = True

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def run(self, mod: pyqir.Module) -> dict:
        """Process module and return the AdaptiveProgram dict."""
        errors = mod.verify()
        if errors is not None:
            raise ValueError(f"Module verification failed: {errors}")

        # Phase 1: Assign block IDs and function IDs for all defined functions
        for func in mod.functions:
            if len(func.basic_blocks) > 0:
                self._assign_function(func)

        # Phase 2: Walk instructions and emit encoding
        for func in mod.functions:
            if len(func.basic_blocks) > 0:
                self._walk_function(func)

        entry_func = next(filter(pyqir.is_entry_point, mod.functions))
        return {
            "version": 1,
            "num_qubits": pyqir.required_num_qubits(entry_func),
            "num_results": pyqir.required_num_results(entry_func),
            "num_registers": self._next_reg,
            "entry_block": self._block_to_id[entry_func.basic_blocks[0]],
            "blocks": self.blocks,
            "instructions": self.instructions,
            "quantum_ops": self.quantum_ops,
            "functions": self.functions,
            "phi_entries": self.phi_entries,
            "switch_cases": self.switch_cases,
            "call_args": self.call_args,
            "labels": self.labels,
            "register_types": self.register_types,
        }

    # ------------------------------------------------------------------
    # Register allocation
    # ------------------------------------------------------------------

    def _alloc_reg(self, value: Any, type_tag: int) -> int:
        """Allocate a new register for *value* and record its type.

        If *value* was already pre-allocated (e.g. as a forward reference from
        a phi node), return the existing register instead of allocating a new
        one.
        """
        if value is not None and value in self._value_to_reg:
            return self._value_to_reg[value]
        reg = self._next_reg
        self._next_reg += 1
        if value is not None:
            self._value_to_reg[value] = reg
        self.register_types.append(type_tag)
        return reg

    # ------------------------------------------------------------------
    # Instruction emission
    # ------------------------------------------------------------------

    def _emit(
        self,
        opcode: int,
        dst: int = 0,
        src0: int = 0,
        src1: int = 0,
        aux0: int = 0,
        aux1: int = 0,
        aux2: int = 0,
        aux3: int = 0,
    ) -> None:
        self.instructions.append((opcode, dst, src0, src1, aux0, aux1, aux2, aux3))

    def _emit_quantum_op(
        self,
        op_id: int,
        q1: int = 0,
        q2: int = 0,
        q3: int = 0,
        angle: float = 0.0,
    ) -> int:
        idx = self._next_qop
        self._next_qop += 1
        self.quantum_ops.append((op_id, q1, q2, q3, angle))
        return idx

    # ------------------------------------------------------------------
    # Operand resolution
    # ------------------------------------------------------------------

    def _resolve_operand(self, value: Any) -> int:
        """Resolve a pyqir Value to a register index.

        If *value* is an already-assigned SSA register, return its index.
        If *value* is an integer constant, allocate a register and emit
        ``OP_CONST`` to materialise it.
        """
        if value in self._value_to_reg:
            return self._value_to_reg[value]

        if isinstance(value, pyqir.IntConstant):
            # Determine type tag from the constant's bit width
            type_tag = self._int_type_tag(value)
            reg = self._alloc_reg(value, type_tag)
            # For i32-range values, use immediate
            self._emit(OP_CONST | FLAG_SRC0_IMM, dst=reg, src0=value.value & 0xFFFFFFFF)
            return reg

        if isinstance(value, pyqir.FloatConstant):
            reg = self._alloc_reg(value, REG_TYPE_F32)
            bits = struct.unpack("<I", struct.pack("<f", value.value))[0]
            self._emit(OP_CONST | FLAG_SRC0_IMM | FLAG_FLOAT, dst=reg, src0=bits)
            return reg

        # Forward reference (e.g. phi incoming from a later block).
        # Pre-allocate a register; the defining instruction will reuse it
        # via _alloc_reg's dedup check.
        if isinstance(value, pyqir.Instruction):
            return self._alloc_reg(value, self._type_tag(value.type))

        # Constant expressions (e.g. inttoptr (i64 N to %Qubit*)).
        if isinstance(value, pyqir.Constant):
            # Try extracting as a qubit/result pointer constant.
            qid = pyqir.qubit_id(value)
            if qid is not None:
                reg = self._alloc_reg(value, REG_TYPE_PTR)
                self._emit(OP_CONST | FLAG_SRC0_IMM, dst=reg, src0=qid)
                return reg
            rid = pyqir.result_id(value)
            if rid is not None:
                reg = self._alloc_reg(value, REG_TYPE_PTR)
                self._emit(OP_CONST | FLAG_SRC0_IMM, dst=reg, src0=rid)
                return reg
            # Null pointer
            if value.is_null:
                reg = self._alloc_reg(value, REG_TYPE_PTR)
                self._emit(OP_CONST | FLAG_SRC0_IMM, dst=reg, src0=0)
                return reg

        raise ValueError(f"Cannot resolve operand: {type(value).__name__}")

    def _int_type_tag(self, value: pyqir.IntConstant) -> int:
        """Determine register type tag for an integer constant."""
        if isinstance(value.type, pyqir.IntType):
            w = value.type.width
            if w == 1:
                return REG_TYPE_BOOL
            if w <= 32:
                return REG_TYPE_I32
            return REG_TYPE_I64
        return REG_TYPE_I32

    def _type_tag(self, ty: Any) -> int:
        """Map a pyqir Type to a register type tag."""
        if isinstance(ty, pyqir.IntType):
            w = ty.width
            if w == 1:
                return REG_TYPE_BOOL
            if w <= 32:
                return REG_TYPE_I32
            return REG_TYPE_I64
        if isinstance(ty, pyqir.PointerType):
            return REG_TYPE_PTR
        type_str = str(ty)
        if "float" in type_str:
            return REG_TYPE_F32
        if "double" in type_str:
            return REG_TYPE_F64
        if "ptr" in type_str or "*" in type_str:
            return REG_TYPE_PTR
        return REG_TYPE_I32  # default

    # ------------------------------------------------------------------
    # Binary / unary helpers
    # ------------------------------------------------------------------

    def _emit_binary(self, opcode: int, instr: Any) -> None:
        """Emit a binary arithmetic/bitwise instruction."""
        dst = self._alloc_reg(instr, self._type_tag(instr.type))
        src0 = self._resolve_operand(instr.operands[0])
        src1 = self._resolve_operand(instr.operands[1])
        self._emit(opcode, dst=dst, src0=src0, src1=src1)

    def _emit_unary(self, opcode: int, instr: Any) -> None:
        """Emit a unary conversion instruction."""
        dst = self._alloc_reg(instr, self._type_tag(instr.type))
        src0 = self._resolve_operand(instr.operands[0])
        self._emit(opcode, dst=dst, src0=src0)

    def _emit_sext(self, instr: Any) -> None:
        """Emit OP_SEXT with source bit width in aux0."""
        dst = self._alloc_reg(instr, self._type_tag(instr.type))
        src0 = self._resolve_operand(instr.operands[0])
        src_type = instr.operands[0].type
        src_bits = src_type.width if isinstance(src_type, pyqir.IntType) else 32
        self._emit(OP_SEXT, dst=dst, src0=src0, aux0=src_bits)

    # ------------------------------------------------------------------
    # Function assignment (Phase 1)
    # ------------------------------------------------------------------

    def _assign_function(self, func: Any) -> None:
        """Assign block IDs and function IDs for a function."""
        if not pyqir.is_entry_point(func) and func.name not in self._func_to_id:
            func_id = len(self.functions)
            self._func_to_id[func.name] = func_id
        for block in func.basic_blocks:
            self._block_to_id[block] = self._next_block
            self._next_block += 1

    # ------------------------------------------------------------------
    # Function walking (Phase 2)
    # ------------------------------------------------------------------

    def _walk_function(self, func: Any) -> None:
        """Walk all blocks and instructions in a function, emitting bytecode."""
        self._current_func_is_entry = pyqir.is_entry_point(func)

        # For non-entry functions, register parameters as registers
        if not self._current_func_is_entry:
            param_base = self._next_reg
            for param in func.params:
                self._alloc_reg(
                    param, REG_TYPE_PTR
                )  # params are pointers (%Qubit*, %Result*)
            # Record function entry in the function table
            func_name = func.name
            if func_name in self._func_to_id:
                func_entry_block = self._block_to_id[func.basic_blocks[0]]
                self.functions.append(
                    (func_entry_block, len(func.params), param_base, 0)
                )

        for block in func.basic_blocks:
            block_id = self._block_to_id[block]
            instr_offset = len(self.instructions)
            for instr in block.instructions:
                self._on_instruction(instr)
            # NOTE: block.terminator is already included in block.instructions
            # in pyqir, so we do NOT separately process it.
            instr_count = len(self.instructions) - instr_offset
            self.blocks.append((block_id, instr_offset, instr_count, 0))

    # ------------------------------------------------------------------
    # Instruction dispatch
    # ------------------------------------------------------------------

    def _on_instruction(self, instr: Any) -> None:
        """Dispatch a single instruction by opcode."""
        match instr.opcode:
            case pyqir.Opcode.CALL:
                self._emit_call(instr)
            case pyqir.Opcode.PHI:
                self._emit_phi(instr)
            case pyqir.Opcode.ICMP:
                self._emit_icmp(instr)
            case pyqir.Opcode.FCMP:
                self._emit_fcmp(instr)
            case pyqir.Opcode.SWITCH:
                self._emit_switch(instr)
            case pyqir.Opcode.BR:
                self._emit_branch(instr)
            case pyqir.Opcode.RET:
                self._emit_ret(instr)
            case pyqir.Opcode.SELECT:
                self._emit_select(instr)
            case pyqir.Opcode.ADD:
                self._emit_binary(OP_ADD, instr)
            case pyqir.Opcode.SUB:
                self._emit_binary(OP_SUB, instr)
            case pyqir.Opcode.MUL:
                self._emit_binary(OP_MUL, instr)
            case pyqir.Opcode.UDIV:
                self._emit_binary(OP_UDIV, instr)
            case pyqir.Opcode.SDIV:
                self._emit_binary(OP_SDIV, instr)
            case pyqir.Opcode.UREM:
                self._emit_binary(OP_UREM, instr)
            case pyqir.Opcode.SREM:
                self._emit_binary(OP_SREM, instr)
            case pyqir.Opcode.AND:
                self._emit_binary(OP_AND, instr)
            case pyqir.Opcode.OR:
                self._emit_binary(OP_OR, instr)
            case pyqir.Opcode.XOR:
                self._emit_binary(OP_XOR, instr)
            case pyqir.Opcode.SHL:
                self._emit_binary(OP_SHL, instr)
            case pyqir.Opcode.LSHR:
                self._emit_binary(OP_LSHR, instr)
            case pyqir.Opcode.ASHR:
                self._emit_binary(OP_ASHR, instr)
            case pyqir.Opcode.ZEXT:
                self._emit_unary(OP_ZEXT, instr)
            case pyqir.Opcode.SEXT:
                self._emit_sext(instr)
            case pyqir.Opcode.TRUNC:
                self._emit_unary(OP_TRUNC, instr)
            case pyqir.Opcode.FADD:
                self._emit_binary(OP_FADD | FLAG_FLOAT, instr)
            case pyqir.Opcode.FSUB:
                self._emit_binary(OP_FSUB | FLAG_FLOAT, instr)
            case pyqir.Opcode.FMUL:
                self._emit_binary(OP_FMUL | FLAG_FLOAT, instr)
            case pyqir.Opcode.FDIV:
                self._emit_binary(OP_FDIV | FLAG_FLOAT, instr)
            case pyqir.Opcode.FP_EXT:
                self._emit_unary(OP_FPEXT | FLAG_FLOAT, instr)
            case pyqir.Opcode.FP_TRUNC:
                self._emit_unary(OP_FPTRUNC | FLAG_FLOAT, instr)
            case pyqir.Opcode.FP_TO_SI:
                self._emit_unary(OP_FPTOSI, instr)
            case pyqir.Opcode.SI_TO_FP:
                self._emit_unary(OP_SITOFP | FLAG_FLOAT, instr)
            case pyqir.Opcode.INT_TO_PTR:
                self._emit_inttoptr(instr)
            case _:
                # Ignore unrecognised opcodes (e.g. alloca, load, store used
                # in some QIR patterns).
                pass

    # ------------------------------------------------------------------
    # Call dispatch
    # ------------------------------------------------------------------

    def _emit_call(self, call: Any) -> None:
        """Dispatch a CALL instruction based on callee name."""
        callee = call.callee.name

        match callee:
            case "__quantum__qis__read_result__body" | "__quantum__rt__read_result":
                dst = self._alloc_reg(call, REG_TYPE_BOOL)
                result_reg = self._resolve_result_operand(call.args[0])
                self._emit(OP_READ_RESULT, dst=dst, src0=result_reg)
            case _ if callee.startswith("__quantum__qis__"):
                self._emit_quantum_call(call)
            case "__quantum__rt__result_record_output":
                result_reg = self._resolve_result_operand(call.args[0])
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(OP_RECORD_OUTPUT, src0=result_reg, aux0=label_idx)
            case "__quantum__rt__array_record_output":
                # Record structure output — pass through as-is for output formatting
                count = (
                    call.args[0].value
                    if isinstance(call.args[0], pyqir.IntConstant)
                    else 0
                )
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(
                    OP_RECORD_OUTPUT, src0=count, aux0=label_idx, aux1=1
                )  # aux1=1 -> array
            case "__quantum__rt__tuple_record_output":
                count = (
                    call.args[0].value
                    if isinstance(call.args[0], pyqir.IntConstant)
                    else 0
                )
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(
                    OP_RECORD_OUTPUT, src0=count, aux0=label_idx, aux1=2
                )  # aux1=2 -> tuple
            case "__quantum__rt__bool_record_output":
                # Bool record output - pass through
                src = self._resolve_operand(call.args[0])
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(
                    OP_RECORD_OUTPUT, src0=src, aux0=label_idx, aux1=3
                )  # aux1=3 -> bool
            case "__quantum__rt__int_record_output":
                src = self._resolve_operand(call.args[0])
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(
                    OP_RECORD_OUTPUT, src0=src, aux0=label_idx, aux1=4
                )  # aux1=4 -> int
            case (
                "__quantum__rt__initialize"
                | "__quantum__rt__begin_parallel"
                | "__quantum__rt__end_parallel"
                | "__quantum__qis__barrier__body"
                | "__quantum__rt__read_loss"
            ):
                pass  # No-op
            case _ if callee in self._func_to_id:
                self._emit_ir_function_call(call)
            case _:
                raise ValueError(f"Unsupported call: {callee}")

    # ------------------------------------------------------------------
    # Quantum call routing
    # ------------------------------------------------------------------

    def _emit_quantum_call(self, call: Any) -> None:
        """Emit a quantum gate, measure, or reset from a ``__quantum__qis__*`` call."""
        callee = call.callee.name
        gate_name = (
            callee.replace("__quantum__qis__", "")
            .replace("__body", "")
            .replace("__adj", "")
        )

        # --- Measure gates ---
        if gate_name in MEASURE_GATES:
            op_id = MEASURE_GATES[gate_name]
            qubit_arg = call.args[0]
            result_arg = call.args[1]

            q_static = pyqir.qubit_id(qubit_arg)
            r_static = pyqir.result_id(result_arg)

            q_val = q_static if q_static is not None else 0
            r_val = r_static if r_static is not None else 0

            qop_idx = self._emit_quantum_op(op_id, q_val, r_val)

            # Dynamic qubit register
            dyn_q_reg = DYN_QUBIT_SENTINEL
            if q_static is None:
                dyn_q_reg = self._resolve_operand(qubit_arg)

            # Dynamic result register
            dyn_r_reg = DYN_QUBIT_SENTINEL
            if r_static is None:
                dyn_r_reg = self._resolve_operand(result_arg)

            self._emit(
                OP_MEASURE,
                dst=0,
                src0=0,
                src1=0,
                aux0=qop_idx,
                aux1=dyn_q_reg,
                aux2=dyn_r_reg,
            )
            return

        # --- Reset gates ---
        if gate_name in RESET_GATES:
            op_id = RESET_GATES[gate_name]
            qubit_arg = call.args[0]
            q_static = pyqir.qubit_id(qubit_arg)
            q_val = q_static if q_static is not None else 0
            qop_idx = self._emit_quantum_op(op_id, q_val)

            dyn_q_reg = DYN_QUBIT_SENTINEL
            if q_static is None:
                dyn_q_reg = self._resolve_operand(qubit_arg)

            self._emit(OP_RESET, dst=0, src0=0, src1=0, aux0=qop_idx, aux1=dyn_q_reg)
            return

        # --- Standard gates ---
        if gate_name not in GATE_MAP:
            raise ValueError(f"Unknown quantum gate: {gate_name} (callee: {callee})")

        op_id = GATE_MAP[gate_name]
        angle = 0.0
        arg_offset = 0

        # Rotation gates have angle as first argument
        if gate_name in ROTATION_GATES:
            angle_arg = call.args[0]
            if isinstance(angle_arg, pyqir.FloatConstant):
                angle = angle_arg.value
            else:
                angle = 0.0  # dynamic angle — store in register (future)
            arg_offset = 1

        # Resolve qubit arguments
        qubit_args = call.args[arg_offset:]

        q1_static = pyqir.qubit_id(qubit_args[0]) if len(qubit_args) > 0 else None
        q2_static = pyqir.qubit_id(qubit_args[1]) if len(qubit_args) > 1 else None
        q3_static = pyqir.qubit_id(qubit_args[2]) if len(qubit_args) > 2 else None

        q1_val = q1_static if q1_static is not None else 0
        q2_val = q2_static if q2_static is not None else 0
        q3_val = q3_static if q3_static is not None else 0

        qop_idx = self._emit_quantum_op(op_id, q1_val, q2_val, q3_val, angle)

        # Dynamic qubit registers
        dyn_q1 = DYN_QUBIT_SENTINEL
        dyn_q2 = DYN_QUBIT_SENTINEL
        if q1_static is None and len(qubit_args) > 0:
            dyn_q1 = self._resolve_operand(qubit_args[0])
        if q2_static is None and len(qubit_args) > 1:
            dyn_q2 = self._resolve_operand(qubit_args[1])

        self._emit(
            OP_QUANTUM_GATE,
            dst=0,
            src0=0,
            src1=0,
            aux0=qop_idx,
            aux1=dyn_q1,
            aux2=dyn_q2,
        )

    # ------------------------------------------------------------------
    # Control flow emitters
    # ------------------------------------------------------------------

    def _emit_branch(self, instr: Any) -> None:
        """Emit jump or conditional branch."""
        operands = instr.operands
        if len(operands) == 1:
            # Unconditional: br label %target
            target = self._block_to_id[operands[0]]
            self._emit(OP_JUMP, dst=target)
        else:
            # Conditional: br i1 %cond, label %true, label %false
            # pyqir operands: [condition, FALSE_block, TRUE_block]
            cond_reg = self._resolve_operand(operands[0])
            false_block = self._block_to_id[operands[1]]
            true_block = self._block_to_id[operands[2]]
            self._emit(OP_BRANCH, src0=cond_reg, aux0=true_block, aux1=false_block)

    def _emit_phi(self, phi_instr: Any) -> None:
        """Emit a PHI node with side table entries."""
        dst_reg = self._alloc_reg(phi_instr, self._type_tag(phi_instr.type))
        phi_offset = len(self.phi_entries)
        for value, block in phi_instr.incoming:
            val_reg = self._resolve_operand(value)
            block_id = self._block_to_id[block]
            self.phi_entries.append((block_id, val_reg))
        count = len(phi_instr.incoming)
        self._emit(OP_PHI, dst=dst_reg, aux0=phi_offset, aux1=count)

    def _emit_select(self, instr: Any) -> None:
        """Emit a SELECT instruction."""
        dst = self._alloc_reg(instr, self._type_tag(instr.type))
        cond = self._resolve_operand(instr.operands[0])
        true_val = self._resolve_operand(instr.operands[1])
        false_val = self._resolve_operand(instr.operands[2])
        self._emit(OP_SELECT, dst=dst, src0=cond, aux0=true_val, aux1=false_val)

    def _emit_switch(self, switch_instr: Any) -> None:
        """Emit a SWITCH instruction with case table entries.

        NOTE: We use ``operands`` instead of the ``.cond`` / ``.cases``
        helpers because pyqir's ``Switch.cond`` returns a stale ``Function``
        reference when ``mod.functions`` has already been iterated (two-pass
        compilation).  ``operands`` is not affected by this behavior.
        """
        # operands layout: [cond, default_block, case_val0, case_block0, ...]
        ops = switch_instr.operands
        cond_reg = self._resolve_operand(ops[0])
        default_block = self._block_to_id[ops[1]]
        case_offset = len(self.switch_cases)
        num_case_pairs = (len(ops) - 2) // 2
        for i in range(num_case_pairs):
            case_val = ops[2 + 2 * i]
            case_block = ops[2 + 2 * i + 1]
            target_block = self._block_to_id[case_block]
            self.switch_cases.append((case_val.value, target_block))
        case_count = num_case_pairs
        self._emit(
            OP_SWITCH,
            src0=cond_reg,
            aux0=default_block,
            aux1=case_offset,
            aux2=case_count,
        )

    def _emit_ret(self, instr: Any) -> None:
        """Emit RET or CALL_RETURN."""
        if not self._current_func_is_entry:
            # Return from IR-defined function
            if len(instr.operands) > 0:
                ret_reg = self._resolve_operand(instr.operands[0])
                self._emit(OP_CALL_RETURN, src0=ret_reg)
            else:
                self._emit(OP_CALL_RETURN)
        else:
            # Return from entry point
            if len(instr.operands) > 0:
                ret_reg = self._resolve_operand(instr.operands[0])
                self._emit(OP_RET, dst=ret_reg)
            else:
                # Void return — use immediate 0 as exit code.
                self._emit(OP_RET | FLAG_SRC0_IMM, dst=0)

    # ------------------------------------------------------------------
    # Comparison emitters
    # ------------------------------------------------------------------

    def _emit_icmp(self, instr: Any) -> None:
        """Emit an integer comparison."""
        cond_code = ICMP_MAP.get(instr.predicate, 0)
        dst = self._alloc_reg(instr, REG_TYPE_BOOL)
        src0 = self._resolve_operand(instr.operands[0])
        src1 = self._resolve_operand(instr.operands[1])
        self._emit(OP_ICMP | (cond_code << 8), dst=dst, src0=src0, src1=src1)

    def _emit_fcmp(self, instr: Any) -> None:
        """Emit a float comparison."""
        cond_code = FCMP_MAP.get(instr.predicate, 0)
        dst = self._alloc_reg(instr, REG_TYPE_BOOL)
        src0 = self._resolve_operand(instr.operands[0])
        src1 = self._resolve_operand(instr.operands[1])
        self._emit(
            OP_FCMP | (cond_code << 8) | FLAG_FLOAT,
            dst=dst,
            src0=src0,
            src1=src1,
        )

    # ------------------------------------------------------------------
    # inttoptr handling
    # ------------------------------------------------------------------

    def _emit_inttoptr(self, instr: Any) -> None:
        """Handle ``inttoptr`` — just propagate the source register.

        ``inttoptr i64 %v to %Qubit*`` is a no-op cast; the integer value
        is the qubit/result ID.  We use OP_MOV to alias the value.
        """
        src_operand = instr.operands[0]
        src_reg = self._resolve_operand(src_operand)
        # Register the inttoptr result as pointing to the same register
        dst = self._alloc_reg(instr, REG_TYPE_PTR)
        self._emit(OP_MOV, dst=dst, src0=src_reg)

    # ------------------------------------------------------------------
    # IR-defined function call/return
    # ------------------------------------------------------------------

    def _emit_ir_function_call(self, call: Any) -> None:
        """Emit OP_CALL for an IR-defined function."""
        func_name = call.callee.name
        func_id = self._func_to_id[func_name]
        arg_offset = len(self.call_args)
        for arg in call.args:
            self.call_args.append(self._resolve_operand(arg))
        # Allocate return register if function has non-void return type
        type_str = str(call.type)
        if "void" in type_str:
            return_reg = DYN_QUBIT_SENTINEL  # no return
        else:
            return_reg = self._alloc_reg(call, REG_TYPE_I32)
        self._emit(
            OP_CALL,
            dst=return_reg,
            aux0=func_id,
            aux1=len(call.args),
            aux2=arg_offset,
        )

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    def _resolve_result_operand(self, value: Any) -> int:
        """Resolve a result argument — returns static result ID or register index."""
        static_id = pyqir.result_id(value)
        if static_id is not None:
            return static_id
        # Dynamic result — resolve through register
        return self._resolve_operand(value)

    def _extract_label(self, value: Any) -> str:
        """Extract a label string from a call argument."""
        bs = pyqir.extract_byte_string(value)
        if bs is not None:
            return bs.decode("utf-8")
        return ""
