# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""AdaptiveProfilePass: walks Adaptive Profile QIR and emits the intermediate
format consumed by Rust.

Unlike ``AggregateGatesPass`` (which subclasses ``pyqir.QirModuleVisitor`` and
only dispatches CALL instructions), this pass iterates basic blocks and
instructions directly so it can handle *all* LLVM IR opcodes required by the
Adaptive Profile specification.
"""

from __future__ import annotations
from dataclasses import dataclass, astuple
import pyqir
import struct
from typing import Any, Dict, List, Optional, Tuple, TypeAlias, cast
from ._adaptive_bytecode import *

# ---------------------------------------------------------------------------
# Gate name → OpID mapping (must match shader_types.rs OpID enum)
# ---------------------------------------------------------------------------

GATE_MAP: Dict[str, int] = {
    "reset": 1,
    "x": 2,
    "y": 3,
    "z": 4,
    "h": 5,
    "s": 6,
    "s__adj": 7,
    "t": 8,
    "t__adj": 9,
    "sx": 10,
    "sx__adj": 11,
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
    "m": 21,
    "mz": 21,
    "mresetz": 22,
    "swap": 24,
}

# Gates that take a result ID as a second argument
MEASURE_GATES = {"m", "mz", "mresetz"}

# Gates that reset a qubit (single qubit argument, no result)
RESET_GATES = {"reset"}

# Rotation gates that take an angle parameter as first argument
ROTATION_GATES = {"rx", "ry", "rz", "rxx", "ryy", "rzz"}

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
    pyqir.FloatPredicate.UGT: FCMP_UGT,
    pyqir.FloatPredicate.UGE: FCMP_UGE,
    pyqir.FloatPredicate.ULT: FCMP_ULT,
    pyqir.FloatPredicate.ULE: FCMP_ULE,
    pyqir.FloatPredicate.UNE: FCMP_UNE,
    pyqir.FloatPredicate.TRUE: FCMP_TRUE,
}


@dataclass
class AdaptiveProgram:
    num_qubits: int
    num_results: int
    num_registers: int
    entry_block: int
    blocks: List[Block]
    instructions: List[Instruction]
    quantum_ops: List[QuantumOp]
    functions: List[Function]
    phi_entries: List[PhiNodeEntry]
    switch_cases: List[SwitchCase]
    call_args: List[CallArg]
    labels: List[Label]
    register_types: List[RegisterType]

    def as_dict(self):
        """
        Transforms the program to a dictionary, and each of
        the helper dataclasses to a tuple. This format is intended
        to be used in the FFI between Python and Rust.
        """
        return {
            "num_qubits": self.num_qubits,
            "num_results": self.num_results,
            "num_registers": self.num_registers,
            "entry_block": self.entry_block,
            "blocks": [astuple(x) for x in self.blocks],
            "instructions": [astuple(x) for x in self.instructions],
            "quantum_ops": [astuple(x) for x in self.quantum_ops],
            "functions": [astuple(x) for x in self.functions],
            "phi_entries": [astuple(x) for x in self.phi_entries],
            "switch_cases": [astuple(x) for x in self.switch_cases],
            "call_args": self.call_args,
            "labels": self.labels,
            "register_types": self.register_types,
        }


@dataclass
class Block:
    block_id: int
    instr_offset: int
    instr_count: int


@dataclass
class Instruction:
    opcode: int
    dst: int
    src0: int
    src1: int
    aux0: int
    aux1: int
    aux2: int
    aux3: int


@dataclass
class QuantumOp:
    op_id: int
    q1: int
    q2: int
    q3: int
    angle: float


@dataclass
class Function:
    func_entry_block: int
    num_params: int
    param_base: int


@dataclass
class PhiNodeEntry:
    block_id: int
    val_reg: int


@dataclass
class SwitchCase:
    case_val: int
    target_block: int


# OpID for correlated noise (must match shader_types.rs OpID::CorrelatedNoise)
CORRELATED_NOISE_OP_ID = 131

CallArg: TypeAlias = int
Label: TypeAlias = str
RegisterType: TypeAlias = int


@dataclass
class IntOperand:
    val: int = 0

    def __post_init__(self):
        # Mask to u32 range so negative Python ints become their
        # two's-complement u32 representation (e.g. -7 → 0xFFFFFFF9).
        self.val = self.val & 0xFFFFFFFF


class FloatOperand:
    def __init__(self, val: float = 0.0) -> None:
        self.val: int = encode_float_as_bits(val)


@dataclass
class Reg:
    val: int  # index in the registers table


def is_immediate(arg) -> bool:
    return isinstance(arg, (IntOperand, FloatOperand))


def prepare_immediate_flags(
    *, dst=None, src0=None, src1=None, aux0=None, aux1=None, aux2=None, aux3=None
):
    flags = 0
    if is_immediate(dst):
        flags |= FLAG_DST_IMM
    if is_immediate(src0):
        flags |= FLAG_SRC0_IMM
    if is_immediate(src1):
        flags |= FLAG_SRC1_IMM
    if is_immediate(aux0):
        flags |= FLAG_AUX0_IMM
    if is_immediate(aux1):
        flags |= FLAG_AUX1_IMM
    if is_immediate(aux2):
        flags |= FLAG_AUX2_IMM
    if is_immediate(aux3):
        flags |= FLAG_AUX3_IMM
    return flags


def unwrap_operands(
    dst, src0, src1, aux0, aux1, aux2, aux3
) -> Tuple[int, int, int, int, int, int, int]:
    if not isinstance(dst, int):
        dst = dst.val
    if not isinstance(src0, int):
        src0 = src0.val
    if not isinstance(src1, int):
        src1 = src1.val
    if not isinstance(aux0, int):
        aux0 = aux0.val
    if not isinstance(aux1, int):
        aux1 = aux1.val
    if not isinstance(aux2, int):
        aux2 = aux2.val
    if not isinstance(aux3, int):
        aux3 = aux3.val
    return (dst, src0, src1, aux0, aux1, aux2, aux3)


def encode_float_as_bits(val: float) -> int:
    return struct.unpack("<I", struct.pack("<f", val))[0]


class AdaptiveProfilePass:
    """Walks Adaptive Profile QIR and emits the intermediate format for Rust."""

    def __init__(self):
        # Output tables.
        self.blocks: List[Block] = []
        self.instructions: List[Instruction] = []
        self.quantum_ops: List[QuantumOp] = []
        self.functions: List[Function] = []
        self.phi_entries: List[PhiNodeEntry] = []
        self.switch_cases: List[SwitchCase] = []
        self.call_args: List[CallArg] = []
        self.labels: List[Label] = []
        self.register_types: List[RegisterType] = []

        # Internal tracking.
        self._next_reg: int = 0
        self._next_block: int = 0
        self._next_qop: int = 0
        self._value_to_reg: Dict[Any, Reg] = {}  # pyqir.Value id → register ID
        self._block_to_id: Dict[Any, int] = {}  # pyqir.BasicBlock id → block ID
        self._func_to_id: Dict[str, int] = {}  # function name → function ID
        self._current_func_is_entry: bool = True
        self._noise_intrinsics: Optional[Dict[str, int]] = None

    def run(
        self,
        mod: pyqir.Module,
        noise=None,
        noise_intrinsics: Optional[Dict[str, int]] = None,
    ) -> AdaptiveProgram:
        """Process module and return the AdaptiveProgram.

        :param mod: The QIR module to process.
        :param noise: Optional NoiseConfig. When provided, noise intrinsic calls
            are resolved to correlated noise ops using the intrinsics table.
        :param noise_intrinsics: Optional dict mapping noise intrinsic callee names
            to noise table IDs. Takes precedence over ``noise`` if both are given.
        :return: The processed adaptive program.
        :rtype: AdaptiveProgram
        """
        if mod.get_flag("arrays"):
            raise ValueError("QIR arrays are not currently supported.")

        if noise_intrinsics is not None:
            self._noise_intrinsics = noise_intrinsics
        elif noise is not None:
            # Build {name: table_id} mapping from the NoiseConfig intrinsics
            intrinsics = noise.intrinsics
            self._noise_intrinsics = {}
            for callee_name in mod.functions:
                name = callee_name.name
                if name in intrinsics:
                    self._noise_intrinsics[name] = intrinsics.get_intrinsic_id(name)

        errors = mod.verify()
        if errors is not None:
            raise ValueError(f"Module verification failed: {errors}")

        # Pass 1: Assign block IDs and function IDs for all defined functions
        for func in mod.functions:
            if len(func.basic_blocks) > 0:
                self._assign_function(func)

        # Pass 2: Walk instructions and emit encoding
        for func in mod.functions:
            if len(func.basic_blocks) > 0:
                self._walk_function(func)

        entry_func = next(filter(pyqir.is_entry_point, mod.functions))
        num_qubits = pyqir.required_num_qubits(entry_func)
        num_results = pyqir.required_num_results(entry_func)
        assert isinstance(num_qubits, int)
        assert isinstance(num_results, int)

        return AdaptiveProgram(
            num_qubits=num_qubits,
            num_results=num_results,
            num_registers=self._next_reg,
            entry_block=self._block_to_id[entry_func.basic_blocks[0]],
            blocks=self.blocks,
            instructions=self.instructions,
            quantum_ops=self.quantum_ops,
            functions=self.functions,
            phi_entries=self.phi_entries,
            switch_cases=self.switch_cases,
            call_args=self.call_args,
            labels=self.labels,
            register_types=self.register_types,
        )

    # ------------------------------------------------------------------
    # Register allocation
    # ------------------------------------------------------------------

    def _alloc_reg(self, value: Any, type_tag: int) -> Reg:
        """Allocate a new register for `value` and record its type.

        If `value` was already pre-allocated (e.g. as a forward reference from
        a phi node), return the existing register instead of allocating a new
        one.
        """
        if value is not None and value in self._value_to_reg:
            return self._value_to_reg[value]
        reg = Reg(self._next_reg)
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
        *,
        dst: int | IntOperand | FloatOperand | Reg = 0,
        src0: int | IntOperand | FloatOperand | Reg = 0,
        src1: int | IntOperand | FloatOperand | Reg = 0,
        aux0: int | IntOperand | FloatOperand | Reg = 0,
        aux1: int | IntOperand | FloatOperand | Reg = 0,
        aux2: int | IntOperand | FloatOperand | Reg = 0,
        aux3: int | IntOperand | FloatOperand | Reg = 0,
    ) -> None:
        imm_flags = prepare_immediate_flags(
            dst=dst, src0=src0, src1=src1, aux0=aux0, aux1=aux1, aux2=aux2, aux3=aux3
        )
        (dst, src0, src1, aux0, aux1, aux2, aux3) = unwrap_operands(
            dst, src0, src1, aux0, aux1, aux2, aux3
        )
        ins = Instruction(opcode | imm_flags, dst, src0, src1, aux0, aux1, aux2, aux3)
        self.instructions.append(ins)

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
        qop = QuantumOp(op_id, q1, q2, q3, angle)
        self.quantum_ops.append(qop)
        return idx

    # ------------------------------------------------------------------
    # Operand resolution
    # ------------------------------------------------------------------

    def _resolve_operand(self, value: pyqir.Value) -> IntOperand | FloatOperand | Reg:
        """Resolve a pyqir Value to a register index.

        If `value` is an already-assigned SSA register, return its index.
        If `value` is an integer constant, allocate a register and emit
        ``OP_CONST`` to materialise it.
        """
        if value in self._value_to_reg:
            return self._value_to_reg[value]

        if isinstance(value, pyqir.IntConstant):
            val = value.value
            return IntOperand(val)

        if isinstance(value, pyqir.FloatConstant):
            val = value.value
            return FloatOperand(val)

        # Forward reference (e.g. phi incoming from a later block).
        # Pre-allocate a register; the defining instruction will reuse it
        # via _alloc_reg's dedup check.
        if isinstance(value, pyqir.Instruction):
            return self._alloc_reg(value, self._type_tag(value.type))

        # Constant expressions (e.g. inttoptr (i64 N to ptr)).
        if isinstance(value, pyqir.Constant):
            # Try extracting as a qubit/result pointer constant.
            pid = pyqir.ptr_id(value)
            if pid is not None:
                return IntOperand(pid)
            # Null pointer
            if value.is_null:
                reg = self._alloc_reg(value, REG_TYPE_PTR)
                self._emit(OP_CONST | FLAG_SRC0_IMM, dst=reg.val, src0=0)
                return reg

        raise ValueError(f"Cannot resolve operand: {type(value).__name__}")

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
        if ty.is_double:
            return REG_TYPE_F64
        # Remaining floating-point types (e.g. float/f32)
        return REG_TYPE_F32

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
    # Function assignment (Pass 1)
    # ------------------------------------------------------------------

    def _assign_function(self, func: pyqir.Function) -> None:
        """Assign block IDs and function IDs for a function."""
        if not pyqir.is_entry_point(func) and func.name not in self._func_to_id:
            func_id = len(self._func_to_id)
            self._func_to_id[func.name] = func_id
        for block in func.basic_blocks:
            self._block_to_id[block] = self._next_block
            self._next_block += 1

    # ------------------------------------------------------------------
    # Function walking (Pass 2)
    # ------------------------------------------------------------------

    def _walk_function(self, func: pyqir.Function) -> None:
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
            if func.name in self._func_to_id:
                func_entry_block = self._block_to_id[func.basic_blocks[0]]
                f = Function(func_entry_block, len(func.params), param_base)
                self.functions.append(f)

        for block in func.basic_blocks:
            block_id = self._block_to_id[block]
            instr_offset = len(self.instructions)
            for instr in block.instructions:
                self._on_instruction(instr)
            # NOTE: block.terminator is already included in block.instructions
            # in pyqir, so we do NOT separately process it.
            instr_count = len(self.instructions) - instr_offset
            blk = Block(block_id, instr_offset, instr_count)
            self.blocks.append(blk)

    # ------------------------------------------------------------------
    # Instruction dispatch
    # ------------------------------------------------------------------

    def _on_instruction(self, instr: pyqir.Instruction) -> None:
        """Dispatch a single instruction by opcode."""
        match instr.opcode:
            case pyqir.Opcode.CALL:
                self._emit_call(cast(pyqir.Call, instr))
            case pyqir.Opcode.PHI:
                self._emit_phi(cast(pyqir.Phi, instr))
            case pyqir.Opcode.ICMP:
                self._emit_icmp(cast(pyqir.ICmp, instr))
            case pyqir.Opcode.FCMP:
                self._emit_fcmp(cast(pyqir.FCmp, instr))
            case pyqir.Opcode.SWITCH:
                self._emit_switch(cast(pyqir.Switch, instr))
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
                raise ValueError(f"Unsupported instruction: {instr.opcode}")

    # ------------------------------------------------------------------
    # Call dispatch
    # ------------------------------------------------------------------

    def _emit_call(self, call: pyqir.Call) -> None:
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
            case _ if "qdk_noise" in call.callee.attributes.func:
                # Check if this is a noise intrinsic (custom gate with qdk_noise attribute)
                self._emit_noise_intrinsic_call(call)
            case _:
                raise ValueError(f"Unsupported call: {callee}")

    # ------------------------------------------------------------------
    # Quantum call dispatch
    # ------------------------------------------------------------------

    def _resolve_qubit_operands(
        self, args: List[pyqir.Value]
    ) -> Tuple[IntOperand | Reg, IntOperand | Reg, IntOperand | Reg]:
        qs: List[IntOperand | Reg] = [IntOperand(), IntOperand(), IntOperand()]
        for i, arg in enumerate(args):
            qs[i] = self._resolve_qubit_operand(arg)
        return (qs[0], qs[1], qs[2])

    def _resolve_qubit_operand(self, arg: pyqir.Value) -> IntOperand | Reg:
        a = self._resolve_operand(arg)
        assert isinstance(a, (IntOperand, Reg))
        return a

    def _resolve_result_operand(self, arg: pyqir.Value) -> IntOperand | Reg:
        a = self._resolve_operand(arg)
        assert isinstance(a, (IntOperand, Reg))
        return a

    def _resolve_angle_operand(self, arg: pyqir.Value) -> FloatOperand | Reg:
        a = self._resolve_operand(arg)
        assert isinstance(a, (FloatOperand, Reg))
        return a

    def _emit_quantum_call(self, call: pyqir.Call) -> None:
        """Emit a quantum gate, measure, or reset from a ``__quantum__qis__*`` call."""
        callee_name = call.callee.name
        gate_name = callee_name.replace("__quantum__qis__", "").replace("__body", "")
        op_id = GATE_MAP[gate_name]
        if gate_name in MEASURE_GATES:
            q = self._resolve_qubit_operand(call.args[0])
            r = self._resolve_result_operand(call.args[1])
            qop_idx = self._emit_quantum_op(op_id, q.val, r.val)
            self._emit(
                OP_MEASURE,
                aux0=qop_idx,
                aux1=q,
                aux2=r,
            )
            return
        if gate_name in RESET_GATES:
            q = self._resolve_qubit_operand(call.args[0])
            qop_idx = self._emit_quantum_op(op_id, q.val)
            self._emit(
                OP_RESET,
                aux0=qop_idx,
                aux1=q,
            )
            return
        if gate_name in ROTATION_GATES:
            qubit_arg_offset = 1
            angle = self._resolve_angle_operand(call.args[0])
        else:
            qubit_arg_offset = 0
            angle = FloatOperand()
        qubit_arg_offset = 1 if gate_name in ROTATION_GATES else 0
        q1, q2, q3 = self._resolve_qubit_operands(call.args[qubit_arg_offset:])
        qop_idx = self._emit_quantum_op(op_id, q1.val, q2.val, q3.val, angle.val)
        self._emit(
            OP_QUANTUM_GATE,
            aux0=qop_idx,
            aux1=q1,
            aux2=q2,
            aux3=q3,
        )

    def _emit_noise_intrinsic_call(self, call: pyqir.Call) -> None:
        """Emit a noise intrinsic call.

        When a noise config is provided and the callee is a known intrinsic,
        store qubit register indices in ``call_args`` (following the same
        pattern as ``_emit_ir_function_call``), then emit a single
        ``OP_QUANTUM_GATE`` whose ``aux1`` = qubit count and ``aux2`` =
        offset into ``call_args``.  The shader reads qubit IDs from
        ``call_arg_table`` at runtime, supporting arbitrarily many qubits.

        When no noise config is provided, emit an identity gate (no-op).
        """
        callee_name = call.callee.name
        if self._noise_intrinsics is not None and callee_name in self._noise_intrinsics:
            table_id = self._noise_intrinsics[callee_name]
            qubit_count = len(call.args)
            # Store qubit register indices in call_args, materializing
            # immediates into registers (same pattern as _emit_ir_function_call).
            arg_offset = len(self.call_args)
            for arg in call.args:
                operand = self._resolve_qubit_operand(arg)
                if isinstance(operand, Reg):
                    self.call_args.append(operand.val)
                else:
                    reg = self._alloc_reg(None, REG_TYPE_PTR)
                    self._emit(OP_MOV | FLAG_SRC0_IMM, dst=reg, src0=operand.val)
                    self.call_args.append(reg.val)
            # QuantumOp stores table_id in q1 and qubit_count in q2.
            qop_idx = self._emit_quantum_op(
                CORRELATED_NOISE_OP_ID, table_id, qubit_count
            )
            self._emit(
                OP_QUANTUM_GATE,
                aux0=qop_idx,
                aux1=IntOperand(qubit_count),
                aux2=IntOperand(arg_offset),
            )
        elif self._noise_intrinsics is not None:
            raise ValueError(f"Missing noise intrinsic: {callee_name}")
        else:
            # No noise config — no-op
            pass

    # ------------------------------------------------------------------
    # Control flow emitters
    # ------------------------------------------------------------------

    def _emit_branch(self, instr: pyqir.Instruction) -> None:
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

    def _emit_phi(self, phi_instr: pyqir.Phi) -> None:
        """Emit a PHI node with side table entries."""
        dst_reg = self._alloc_reg(phi_instr, self._type_tag(phi_instr.type))
        phi_offset = len(self.phi_entries)
        for value, block in phi_instr.incoming:
            operand = self._resolve_operand(value)
            if isinstance(operand, Reg):
                val_reg = operand.val
            else:
                # Immediate values must be materialized into a register
                # because the GPU phi_table stores register indices.
                reg = self._alloc_reg(None, self._type_tag(phi_instr.type))
                self._emit(OP_MOV | FLAG_SRC0_IMM, dst=reg, src0=operand.val)
                val_reg = reg.val
            block_id = self._block_to_id[block]
            phi_entry = PhiNodeEntry(block_id, val_reg)
            self.phi_entries.append(phi_entry)
        count = len(phi_instr.incoming)
        self._emit(OP_PHI, dst=dst_reg, aux0=phi_offset, aux1=count)

    def _emit_select(self, instr: pyqir.Instruction) -> None:
        """Emit a SELECT instruction."""
        dst = self._alloc_reg(instr, self._type_tag(instr.type))
        cond = self._resolve_operand(instr.operands[0])
        true_val = self._resolve_operand(instr.operands[1])
        false_val = self._resolve_operand(instr.operands[2])
        self._emit(OP_SELECT, dst=dst, src0=cond, aux0=true_val, aux1=false_val)

    def _emit_switch(self, switch_instr: pyqir.Switch) -> None:
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
            switch_case = SwitchCase(case_val.value, target_block)
            self.switch_cases.append(switch_case)
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
                self._emit(OP_RET, dst=IntOperand(0))

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
            operand = self._resolve_operand(arg)
            if isinstance(operand, Reg):
                self.call_args.append(operand.val)
            else:
                # Immediate values must be materialized into a register
                # because the GPU call_arg_table stores register indices.
                reg = self._alloc_reg(None, REG_TYPE_PTR)
                self._emit(OP_MOV | FLAG_SRC0_IMM, dst=reg, src0=operand.val)
                self.call_args.append(reg.val)
        # Allocate return register if function has non-void return type
        if call.type.is_void:
            return_reg = VOID_RETURN  # no return
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

    def _extract_label(self, value: Any) -> str:
        """Extract a label string from a call argument."""
        bs = pyqir.extract_byte_string(value)
        if bs is not None:
            return bs.decode("utf-8")
        return ""
