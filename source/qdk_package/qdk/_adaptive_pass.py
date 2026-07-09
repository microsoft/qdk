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
from enum import Enum
import pyqir
import struct
from typing import Any, Dict, List, Optional, Tuple, TypeAlias, cast
from ._adaptive_bytecode import *


class Bytecode(Enum):
    Bit32 = 32
    Bit64 = 64


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
    "move": 28,
}

# Gates that take a result ID as a second argument
MEASURE_GATES = {"m", "mz", "mresetz"}

# Gates that reset a qubit (single qubit argument, no result)
RESET_GATES = {"reset"}

# Rotation gates that take an angle parameter as first argument
ROTATION_GATES = {"rx", "ry", "rz", "rxx", "ryy", "rzz"}

# Single-qubit gates whose QIR signature carries device-specific extra
# arguments after the qubit pointer (e.g. ``move(qubit, i64, i64)``). The
# extra args are scheduling metadata for hardware backends and are not
# qubit IDs, so we resolve only ``args[0]`` and ignore the rest.
MOVE_GATES = {"move"}

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
    constant_data: List[int]
    memory_size: int

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
            "constant_data": self.constant_data,
            "memory_size": self.memory_size,
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
    # ``angle`` is stored as the raw bit pattern of an IEEE-754 float
    # (encoded via ``encode_float_as_bits``) so it can be packed into the
    # same integer-typed FFI table as the qubit indices. The Rust side
    # reinterprets these bits as f32/f64 depending on the bytecode width.
    #
    # This also follows the same pattern in which floats are encoded as ints
    # in the ``Instruction`` class.
    angle: int


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
    val: int
    bits: int

    def __post_init__(self):
        # Mask to the appropriate word-width so negative Python ints and
        # wider-than-target constants become their two's-complement
        # representation at the target bit width
        # (e.g. -7 → 0xFFFFFFF9 for 32-bit, 0xFFFFFFFFFFFFFFF9 for 64-bit).
        #
        # Note: we have no way to tell if a negative number, represented by
        #       pyqir as an u64 is an overflow or just a negative number.
        #       therefore we don't perform overflow checks here, and instead
        #       default to a wrapping behavior.
        mask = (1 << self.bits) - 1
        self.val = self.val & mask


class FloatOperand:
    def __init__(self, val: float, bytecode_kind: Bytecode) -> None:
        self.val: int = encode_float_as_bits(val, bytecode_kind)


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


def encode_float_as_bits(val: float, bytecode_kind: Bytecode) -> int:
    if bytecode_kind == Bytecode.Bit32:
        return struct.unpack("<I", struct.pack("<f", val))[0]
    else:
        return struct.unpack("<Q", struct.pack("<d", val))[0]


def void_return(bytecode_kind: Bytecode):
    if bytecode_kind == Bytecode.Bit32:
        return 0xFFFF_FFFF
    else:
        return 0xFFFF_FFFF_FFFF_FFFF


class AdaptiveProfilePass:
    """Walks Adaptive Profile QIR and emits the intermediate format for Rust."""

    def __init__(self, bytecode_kind: Bytecode):
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
        self._bytecode_kind = bytecode_kind
        self._int_bits = bytecode_kind.value
        self._next_reg: int = 0
        self._next_block: int = 0
        self._next_qop: int = 0
        self._value_to_reg: Dict[Any, Reg] = {}  # pyqir.Value id → register ID
        self._block_to_id: Dict[Any, int] = {}  # pyqir.BasicBlock id → block ID
        self._func_to_id: Dict[str, int] = {}  # function name → function ID
        self._current_func_is_entry: bool = True
        self._noise_intrinsics: Optional[Dict[str, int]] = None

        # Memory / array tracking.
        self.constant_data: List[int] = []
        self._memory_size: int = 0
        self._global_to_address: Dict[str, int] = {}
        self._alloca_ptr: int = 0

        # Running count of output record outputs (result / bool / int / double).
        # Each leaf record is assigned a stable ordinal (emitted in the
        # instruction's aux2 field) so the GPU shader can write its value to a
        # fixed per-shot slot. Array and tuple records are structural and do not
        # consume an ordinal.
        self._output_record_count: int = 0

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

        # Pre-pass: Scan global constant arrays
        self._scan_global_arrays(mod)

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
            constant_data=self.constant_data,
            memory_size=self._memory_size,
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
        dst, src0, src1, aux0, aux1, aux2, aux3 = unwrap_operands(
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
        angle: int = 0,
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
            return IntOperand(val, self._int_bits)

        if isinstance(value, pyqir.FloatConstant):
            val = value.value
            return FloatOperand(val, self._bytecode_kind)

        # Global variable reference (e.g., @array0)
        if hasattr(value, "name") and value.name in self._global_to_address:
            addr = self._global_to_address[value.name]
            return IntOperand(addr, self._int_bits)

        # Forward reference (e.g. phi incoming from a later block).
        # Pre-allocate a register; the defining instruction will reuse it
        # via _alloc_reg's dedup check.
        if isinstance(value, pyqir.Instruction):
            return self._alloc_reg(value, self._type_tag(value.type))

        # Constant expressions (e.g. inttoptr (i64 N to ptr)).
        if isinstance(value, pyqir.Constant):
            # Named global constants (e.g. @array0) that were not found in
            # _global_to_address.
            if hasattr(value, "name") and value.name:
                init = getattr(value, "initializer", None)
                if init and self._is_output_recording_label(init):
                    # Trying to index into a output recording label.
                    raise ValueError(
                        f"Byte-string global @{value.name} is not indexable: "
                        f"[N x i8] globals are reserved for output labels "
                        f"consumed by __quantum__rt__*_record_output calls."
                    )
                raise ValueError(f"Unresolved global reference: @{value.name}")
            # Try extracting as a qubit/result pointer constant.
            pid = pyqir.ptr_id(value)
            if pid is not None:
                return IntOperand(pid, self._int_bits)
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
    # Global variable scanning (pre-pass)
    # ------------------------------------------------------------------

    @staticmethod
    def _is_output_recording_label(value: pyqir.Constant):
        ty = value.type
        return (
            isinstance(ty, pyqir.ArrayType)
            and isinstance(ty.element, pyqir.IntType)
            and ty.element.width == 8
        )

    @staticmethod
    def _is_global_array(global_variable: pyqir.GlobalVariable):
        init = global_variable.initializer
        if init is None:
            return False
        # If the global variable has an initializer, but the initializer value
        # is not an array, return False. E.g.: This can happens when initializing
        # a global integer constant.
        if not isinstance(init, pyqir.ArrayConstant):
            return False

        # ``[N x i8]`` globals are excluded on purpose: in Adaptive Profile QIR
        # produced by the QDK frontends they exist solely as null-terminated
        # output labels passed to ``__quantum__rt__*_record_output`` runtime
        # calls. Those labels are read directly from the GEP constexpr argument
        # via ``pyqir.extract_byte_string`` in ``_extract_label`` and never need
        # an address in ``_global_to_address``. Encoding them into
        # ``constant_data`` would shift the addresses of every subsequent
        # global without any consumer benefiting from the bytes being
        # indexable. If a future frontend ever emits indexable byte arrays,
        # this predicate (and the matching diagnostic in ``_resolve_operand``)
        # is the place to lift the restriction.
        if AdaptiveProfilePass._is_output_recording_label(init):
            return False

        return True

    def _scan_global_arrays(self, mod: pyqir.Module) -> None:
        """Scan module for global constant arrays and populate constant_data.

        Uses ``Module.global_variables`` to iterate globals, reads each
        initializer via ``GlobalVariable.initializer``, and encodes element
        values into ``constant_data``.

        ``[N x i8]`` byte-string globals are skipped here. See
        ``_is_global_array`` for the rationale. They are consumed by
        ``_extract_label`` directly from the call site.

        This is done in two passes so that pointer-valued arrays (e.g.
        ``[N x ptr]`` where elements reference other globals) work
        regardless of the declaration order of the globals in the module.
        """
        # Pass 1: assign addresses to every supported global array without
        # encoding elements.  This ensures that forward references between
        # globals (e.g. @matrix declared before @row0/@row1) are resolved
        # correctly in Pass 2.
        supported_globals: list[tuple[pyqir.GlobalVariable, pyqir.ArrayConstant]] = []
        base = len(self.constant_data)
        for gv in mod.global_variables:
            if not self._is_global_array(gv):
                continue
            init = cast(pyqir.ArrayConstant, gv.initializer)
            self._global_to_address[gv.name] = base
            base += self._size_in_words(init.type)
            supported_globals.append((gv, init))

        # Pass 2: encode elements now that all addresses are known.
        for gv, init in supported_globals:
            self._encode_array_elements(init, gv.name)

        self._alloca_ptr = len(self.constant_data)
        self._memory_size = self._alloca_ptr

    def _encode_array_elements(self, arr: pyqir.ArrayConstant, gv_name: str) -> None:
        """Recursively encode ArrayConstant elements into constant_data.

        Nested ``ArrayConstant`` elements (e.g. ``[2 x [2 x i32]]``) are
        flattened in row-major order.
        """
        mask = (1 << self._int_bits) - 1
        for elem in arr.elements:
            if isinstance(elem, pyqir.IntConstant):
                self.constant_data.append(elem.value & mask)
            elif isinstance(elem, pyqir.FloatConstant):
                self.constant_data.append(
                    encode_float_as_bits(elem.value, self._bytecode_kind)
                )
            elif isinstance(elem, pyqir.ArrayConstant):
                # Nested array — flatten recursively.
                self._encode_array_elements(elem, gv_name)
            elif isinstance(elem, pyqir.GlobalVariable):
                # Pointer to another global (e.g. @row0 in [N x ptr]).
                if elem.name in self._global_to_address:
                    self.constant_data.append(self._global_to_address[elem.name] & mask)
                else:
                    raise ValueError(
                        f"Global @{gv_name} references @{elem.name} "
                        f"which has not been scanned yet"
                    )
            elif isinstance(elem, pyqir.Constant):
                # Constant expression (e.g. inttoptr (i64 N to ptr)).
                pid = pyqir.ptr_id(elem)
                if pid is not None:
                    self.constant_data.append(pid & mask)
                else:
                    raise ValueError(
                        f"Cannot resolve element in global @{gv_name}: "
                        f"{type(elem).__name__}"
                    )
            else:
                raise ValueError(
                    f"Unsupported element type in global @{gv_name}: "
                    f"{type(elem).__name__}"
                )

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
            case pyqir.Opcode.FREM:
                self._emit_binary(OP_FREM | FLAG_FLOAT, instr)
            case pyqir.Opcode.FP_EXT:
                self._emit_unary(OP_FPEXT | FLAG_FLOAT, instr)
            case pyqir.Opcode.FP_TRUNC:
                self._emit_unary(OP_FPTRUNC | FLAG_FLOAT, instr)
            case pyqir.Opcode.FP_TO_SI:
                self._emit_unary(OP_FPTOSI, instr)
            case pyqir.Opcode.FP_TO_UI:
                self._emit_unary(OP_FPTOUI, instr)
            case pyqir.Opcode.SI_TO_FP:
                self._emit_unary(OP_SITOFP | FLAG_FLOAT, instr)
            case pyqir.Opcode.UI_TO_FP:
                self._emit_unary(OP_UITOFP | FLAG_FLOAT, instr)
            case pyqir.Opcode.INT_TO_PTR:
                self._emit_inttoptr(instr)
            case pyqir.Opcode.ALLOCA:
                self._emit_alloca(instr)
            case pyqir.Opcode.LOAD:
                self._emit_load(instr)
            case pyqir.Opcode.STORE:
                self._emit_store(instr)
            case pyqir.Opcode.GET_ELEMENT_PTR:
                self._emit_gep(instr)
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
            case "__quantum__rt__result_record_output":
                result_reg = self._resolve_result_operand(call.args[0])
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(
                    OP_RECORD_OUTPUT,
                    src0=result_reg,
                    aux0=label_idx,
                    aux1=0,
                    aux2=self._output_record_count,
                )  # aux1=0 -> result
                self._output_record_count += 1
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
                    OP_RECORD_OUTPUT,
                    src0=src,
                    aux0=label_idx,
                    aux1=3,
                    aux2=self._output_record_count,
                )  # aux1=3 -> bool
                self._output_record_count += 1
            case "__quantum__rt__int_record_output":
                src = self._resolve_operand(call.args[0])
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(
                    OP_RECORD_OUTPUT,
                    src0=src,
                    aux0=label_idx,
                    aux1=4,
                    aux2=self._output_record_count,
                )  # aux1=4 -> int
                self._output_record_count += 1
            case "__quantum__rt__double_record_output":
                src = self._resolve_operand(call.args[0])
                label_str = self._extract_label(call.args[1])
                label_idx = len(self.labels)
                self.labels.append(label_str)
                self._emit(
                    OP_RECORD_OUTPUT,
                    src0=src,
                    aux0=label_idx,
                    aux1=5,
                    aux2=self._output_record_count,
                )  # aux1=5 -> double
                self._output_record_count += 1
            case (
                "__quantum__rt__initialize"
                | "__quantum__rt__begin_parallel"
                | "__quantum__rt__end_parallel"
                | "__quantum__qis__barrier__body"
            ):
                pass  # No-op
            case "__quantum__rt__read_loss":
                # Allocate a bool register and emit OP_READ_LOSS so the runtime
                # can ask the simulator whether the given result was produced
                # by measuring a lost qubit. Programs may branch on this value.
                dst = self._alloc_reg(call, REG_TYPE_BOOL)
                result_reg = self._resolve_result_operand(call.args[0])
                self._emit(OP_READ_LOSS, dst=dst, src0=result_reg)
            case _ if callee.startswith("__quantum__qis__"):
                self._emit_quantum_call(call)
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
        qs: List[IntOperand | Reg] = [
            IntOperand(0, self._int_bits),
            IntOperand(0, self._int_bits),
            IntOperand(0, self._int_bits),
        ]
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
        if gate_name in MOVE_GATES:
            # ``move(qubit, i64, i64)``: only the first arg is a qubit; the
            # remaining args are device-specific scheduling metadata that
            # the simulator ignores. Emit a single-qubit OP_QUANTUM_GATE so
            # the runtime invokes ``Simulator::mov`` (which applies the
            # configured ``noise.mov`` faults to that qubit).
            q1, q2, q3 = self._resolve_qubit_operands([call.args[0]])
            angle = FloatOperand(0.0, self._bytecode_kind)
            qop_idx = self._emit_quantum_op(op_id, q1.val, q2.val, q3.val, angle.val)
            self._emit(
                OP_QUANTUM_GATE,
                src0=angle,
                aux0=qop_idx,
                aux1=q1,
                aux2=q2,
                aux3=q3,
            )
            return
        if gate_name in ROTATION_GATES:
            qubit_arg_offset = 1
            angle = self._resolve_angle_operand(call.args[0])
        else:
            qubit_arg_offset = 0
            angle = FloatOperand(0.0, self._bytecode_kind)
        qubit_arg_offset = 1 if gate_name in ROTATION_GATES else 0
        q1, q2, q3 = self._resolve_qubit_operands(call.args[qubit_arg_offset:])
        qop_idx = self._emit_quantum_op(op_id, q1.val, q2.val, q3.val, angle.val)
        self._emit(
            OP_QUANTUM_GATE,
            src0=angle,
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
                aux1=IntOperand(qubit_count, self._int_bits),
                aux2=IntOperand(arg_offset, self._int_bits),
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
        cond_reg = self._resolve_operand(switch_instr.operands[0])
        default_block = self._block_to_id[switch_instr.default]
        case_offset = len(self.switch_cases)
        for case_val, block in switch_instr.cases:
            target_block = self._block_to_id[block]
            switch_case = SwitchCase(case_val.value, target_block)
            self.switch_cases.append(switch_case)
        case_count = len(switch_instr.cases)
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
                self._emit(OP_RET, dst=IntOperand(0, self._int_bits))

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
    # Memory operation emitters
    # ------------------------------------------------------------------

    def _emit_alloca(self, instr: pyqir.Instruction) -> None:
        """Emit OP_ALLOCA — stack memory allocation."""
        dst = self._alloc_reg(instr, REG_TYPE_PTR)
        alloc_type = instr.type

        # Determine the number of words to allocate
        if isinstance(alloc_type, pyqir.PointerType):
            pointee = alloc_type.pointee
            if isinstance(pointee, pyqir.ArrayType):
                num_words = pointee.count
            else:
                # Reject alloca of aggregate data (arrays and structs).
                text = str(instr).lstrip()
                _, _, after_alloca = text.partition("alloca ")
                if after_alloca.startswith("[") or after_alloca.startswith("{"):
                    raise NotImplementedError(
                        "Aggregate stack allocations (alloca of an array "
                        "or struct type) are not supported by the adaptive "
                        "GPU bytecode pass under LLVM opaque pointers; "
                        f"got: {text!r}"
                    )
                num_words = 1
        else:
            num_words = 1

        addr = self._alloca_ptr
        self._alloca_ptr += num_words
        self._memory_size = max(self._memory_size, self._alloca_ptr)

        self._emit(
            OP_ALLOCA,
            dst=dst,
            src0=IntOperand(num_words, self._int_bits),
            src1=IntOperand(addr, self._int_bits),
        )

    def _emit_load(self, instr: pyqir.Instruction) -> None:
        """Emit OP_LOAD — load value from memory address."""
        loaded_type = instr.type
        type_tag = self._type_tag(loaded_type)

        is_float = type_tag in (REG_TYPE_F32, REG_TYPE_F64)

        dst = self._alloc_reg(instr, type_tag)
        ptr = self._resolve_operand(instr.operands[0])

        opcode = OP_LOAD
        if is_float:
            opcode |= FLAG_FLOAT

        self._emit(opcode, dst=dst, src0=ptr)

    def _emit_store(self, instr: pyqir.Instruction) -> None:
        """Emit OP_STORE — store value to memory address."""
        value = self._resolve_operand(instr.operands[0])
        ptr = self._resolve_operand(instr.operands[1])
        self._emit(OP_STORE, src0=value, src1=ptr)

    def _size_in_words(self, ty: pyqir.Type) -> int:
        """Number of storage words occupied by `ty`.

        Scalars (int, float) and pointers occupy one word. Arrays flatten
        row-major to ``count * size(element)``. Structs and function
        types are not produced by the QDK frontends.
        """
        # Base case
        # Pyqir doesn't have a FloatType, so, we instead check that the type
        # is not an array, struct, or function.
        if not isinstance(ty, (pyqir.ArrayType, pyqir.StructType, pyqir.Function)):
            # Return 1 if the type is an int, float, or pointer
            return 1

        # Recursive case
        if isinstance(ty, pyqir.ArrayType):
            return ty.count * self._size_in_words(ty.element)

        raise TypeError(f"Expected a scalar or pointer type, found {ty}")

    def _gep_source_type(self, ptr: pyqir.Value) -> pyqir.Type:
        """Recover the source-element-type `T` of a *multi-index* GEP.

        For a GEP `getelementptr T, ptr %p, i_0, i_1, ..., i_n` with
        `n >= 1`, LLVM defines the address as

            %p + i_0 * sizeof(T) + i_1 * sizeof(T_1) + ...

        where `T_k` is the type reached after the first `k` indices. To
        emit the right strides we need `T`. The single-index case is
        *not* handled here — see `_emit_gep` for why.

        pyqir limitations worked around in this method:

        * `pyqir.Instruction` does not expose a GEP's source-element-type
          (the leading type token in `getelementptr T, ptr %p, ...`). The
          only instruction attributes available are `opcode`, `operands`,
          `type`, `name`. There is no way to read `T` directly.
        * Under LLVM 15+ opaque pointers, `PointerType.pointee` returns
          an opaque `Type`, so `T` cannot be recovered from the pointer
          operand's declared type either.
        """
        if isinstance(ptr, pyqir.GlobalVariable) and ptr.initializer is not None:
            return ptr.initializer.type
        # SSA pointer base — safe only under the QDK convention that the
        # outermost index is the constant `0`. See docstring.
        return ptr.type

    def _emit_gep(self, instr: pyqir.Instruction) -> None:
        """Lower `getelementptr` to a chain of OP_GEPs.

        LLVM GEP address formula for `getelementptr T, ptr %p, i_0, ..., i_n`:

            addr = %p + sum_k(i_k * sizeof(T_k))

        where `T_0 = T` and `T_{k+1} = T_k.element` when `T_k` is an
        array type. Each iteration of the loop below emits one OP_GEP
        with the stride for that level.

        Single-index vs. multi-index handling
        -------------------------------------

        Because pyqir does not expose the source-element-type
        `T` of a GEP (see `_gep_source_type`), the two forms are
        handled differently:

        * **Single-index** `getelementptr T, ptr %p, i_0`: the type
          `T` may be unrelated to anything we can observe on `%p`. For
          example, `getelementptr ptr, ptr @array_of_ptrs, i64 j`
          declares `T = ptr` even though `@array_of_ptrs.initializer`
          has type `[N x ptr]`. Under the QDK codegen convention,
          single-index GEPs always step over scalar/pointer-sized
          slots, so we hardcode stride = 1. *This is a QDK convention,
          not LLVM-general semantics.*

        * **Multi-index** `getelementptr T, ptr %p, i_0, ..., i_n`
          (n >= 1): the QDK codegen convention is that `T` matches
          the pointer base's aggregate (see `_gep_source_type`), so
          we can compute every stride from the recovered `T`.
        """
        ptr = instr.operands[0]
        indices = instr.operands[1:]
        base_addr = self._resolve_operand(ptr)
        end_dst = self._alloc_reg(instr, REG_TYPE_PTR)

        if not indices:
            # `getelementptr T, ptr %p` with no indices is just %p; emit
            # a MOV so `end_dst` is still defined for downstream uses.
            self._emit(OP_MOV, dst=end_dst, src0=base_addr)
            return

        if len(indices) == 1:
            # Single-index GEP. The textual stride sizeof(T) is hidden
            # by pyqir. Under the QDK convention, single-index GEPs
            # step over scalar/pointer-sized slots, so stride = 1.
            self._emit(
                OP_GEP,
                dst=end_dst,
                src0=base_addr,
                src1=self._resolve_operand(indices[0]),
                aux0=IntOperand(1, self._int_bits),
            )
            return

        # Multi-index GEP. Recover T (LLVM-correct under the QDK
        # codegen conventions documented in `_gep_source_type`) and
        # walk it level by level.
        ty = self._gep_source_type(ptr)
        cur_addr = base_addr
        last = len(indices) - 1
        for k, idx_val in enumerate(indices):
            stride = IntOperand(self._size_in_words(ty), self._int_bits)
            out = end_dst if k == last else self._alloc_reg(None, REG_TYPE_PTR)
            self._emit(
                OP_GEP,
                dst=out,
                src0=cur_addr,
                src1=self._resolve_operand(idx_val),
                aux0=stride,
            )
            cur_addr = out
            # Descend into the aggregate for the next index level. If
            # `ty` is not an array (the SSA-base opaque fallback
            # documented in `_gep_source_type`), leave it alone: the
            # subsequent strides stay at 1, which is correct under the
            # QDK convention that the outermost index is `0` and the
            # innermost levels reach scalar leaves.
            if isinstance(ty, pyqir.ArrayType):
                ty = ty.element

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
            return_reg = void_return(self._bytecode_kind)  # no return
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
