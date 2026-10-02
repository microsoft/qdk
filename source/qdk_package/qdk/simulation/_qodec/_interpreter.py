from __future__ import annotations
from qdk import Result
import math
import operator
import struct
from typing import Callable, Dict, List, Protocol, Sequence, Tuple, TypeAlias, Union

from qodec.instructions import InstructionCall

from ..._adaptive_bytecode import *
from ..._adaptive_pass import (
    CORRELATED_NOISE_OP_ID,
    GATE_MAP,
    AdaptiveProgram,
    Instruction,
)
from .bytecode import INSTRUCTION_CALL_OP_ID, QodecProgram

_MASK64 = 0xFFFF_FFFF_FFFF_FFFF
_SIGN64 = 0x8000_0000_0000_0000
_VOID_RETURN = _MASK64

# Operand index -> immediate flag, matching ``Runtime::resolve_u64``.
_IMM_FLAGS = (
    FLAG_SRC0_IMM,
    FLAG_SRC1_IMM,
    FLAG_DST_IMM,
    FLAG_AUX0_IMM,
    FLAG_AUX1_IMM,
    FLAG_AUX2_IMM,
    FLAG_AUX3_IMM,
)

# Record kinds encoded in ``aux1`` of ``OP_RECORD_OUTPUT``.
_RECORD_RESULT = 0
_RECORD_BOOL = 3
_RECORD_INT = 4
_RECORD_DOUBLE = 5

OutputRecordValue: TypeAlias = Union[Result, bool, int, float]
"""One value emitted by a ``__quantum__rt__*_record_output`` call."""


class QuantumSink(Protocol):
    """The quantum operations an adaptive-profile program can ask for.

    This mirrors the Rust ``Simulator`` trait minus the state-dump and
    noise-table accessors, plus :meth:`result`, which replaces Rust's
    ``measurements()`` slice so an implementation can produce an outcome on
    demand rather than keeping a whole buffer.
    """

    def x(self, target: int) -> None: ...
    def y(self, target: int) -> None: ...
    def z(self, target: int) -> None: ...
    def h(self, target: int) -> None: ...
    def s(self, target: int) -> None: ...
    def s_adj(self, target: int) -> None: ...
    def t(self, target: int) -> None: ...
    def t_adj(self, target: int) -> None: ...
    def sx(self, target: int) -> None: ...
    def sx_adj(self, target: int) -> None: ...
    def rx(self, angle: float, target: int) -> None: ...
    def ry(self, angle: float, target: int) -> None: ...
    def rz(self, angle: float, target: int) -> None: ...
    def cx(self, control: int, target: int) -> None: ...
    def cy(self, control: int, target: int) -> None: ...
    def cz(self, control: int, target: int) -> None: ...
    def rxx(self, angle: float, q1: int, q2: int) -> None: ...
    def ryy(self, angle: float, q1: int, q2: int) -> None: ...
    def rzz(self, angle: float, q1: int, q2: int) -> None: ...
    def swap(self, q1: int, q2: int) -> None: ...
    def mov(self, target: int) -> None: ...
    def mz(self, target: int, result_id: int) -> None: ...
    def mresetz(self, target: int, result_id: int) -> None: ...
    def resetz(self, target: int) -> None: ...
    def peek_loss(self, target: int, result_id: int) -> None: ...
    def apply_readout_noise(
        self, p_zero_as_one: float, p_one_as_zero: float, result_id: int
    ) -> None: ...
    def correlated_noise_intrinsic(
        self, intrinsic_id: int, targets: Sequence[int]
    ) -> None: ...
    def instruction(self, call: InstructionCall, results: Sequence[int]) -> None:
        """Invoke an ISA instruction by name, recording its outcomes in ``results``."""
        ...

    def result(self, result_id: int) -> Result:
        """Outcome recorded for ``result_id``, or ``Result.Zero`` if unmeasured."""
        ...


# ---------------------------------------------------------------------------
# Rust integer and float semantics
# ---------------------------------------------------------------------------


def _as_i64(value: int) -> int:
    value &= _MASK64
    return value - (1 << 64) if value & _SIGN64 else value


def _bits_to_f64(bits: int) -> float:
    return struct.unpack("<d", struct.pack("<Q", bits & _MASK64))[0]


def _f64_to_bits(value: float) -> int:
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def _trunc_div(a: int, b: int) -> int:
    """Rust ``i64::wrapping_div``: division truncated toward zero."""
    quotient = abs(a) // abs(b)
    return -quotient if (a < 0) != (b < 0) else quotient


def _trunc_rem(a: int, b: int) -> int:
    """Rust ``i64::wrapping_rem``: remainder with the sign of the dividend."""
    remainder = abs(a) % abs(b)
    return -remainder if a < 0 else remainder


def _fdiv(a: float, b: float) -> float:
    try:
        return a / b
    except ZeroDivisionError:
        if a == 0.0 or math.isnan(a):
            return math.nan
        return math.copysign(math.inf, a) * math.copysign(1.0, b)


def _frem(a: float, b: float) -> float:
    try:
        return math.fmod(a, b)
    except ValueError:
        return math.nan


def _f64_as_i64(value: float) -> int:
    """Rust ``f64 as i64``: saturating, with NaN mapping to zero."""
    if math.isnan(value):
        return 0
    if value >= float(1 << 63):
        return (1 << 63) - 1
    if value <= -float(1 << 63):
        return -(1 << 63)
    return int(value)


def _f64_as_u64(value: float) -> int:
    """Rust ``f64 as u64``: saturating, with NaN and negatives mapping to zero."""
    if math.isnan(value) or value <= 0.0:
        return 0
    if value >= float(1 << 64):
        return _MASK64
    return int(value)


# ---------------------------------------------------------------------------
# Quantum op ID tables, derived from the emitter's ``GATE_MAP``
# ---------------------------------------------------------------------------

_ONE_QUBIT_GATES: Dict[int, str] = {
    GATE_MAP[name]: method
    for name, method in (
        ("x", "x"),
        ("y", "y"),
        ("z", "z"),
        ("h", "h"),
        ("s", "s"),
        ("s__adj", "s_adj"),
        ("t", "t"),
        ("t__adj", "t_adj"),
        ("sx", "sx"),
        ("sx__adj", "sx_adj"),
        ("move", "mov"),
    )
}

_TWO_QUBIT_GATES: Dict[int, str] = {
    GATE_MAP[name]: method
    for name, method in (("cx", "cx"), ("cy", "cy"), ("cz", "cz"), ("swap", "swap"))
}

_ONE_QUBIT_ROTATIONS: Dict[int, str] = {
    GATE_MAP[name]: name for name in ("rx", "ry", "rz")
}

_TWO_QUBIT_ROTATIONS: Dict[int, str] = {
    GATE_MAP[name]: name for name in ("rxx", "ryy", "rzz")
}

_MEASURE_OPS: Dict[int, str] = {GATE_MAP["mz"]: "mz", GATE_MAP["mresetz"]: "mresetz"}

_RESET_OPS: Dict[int, str] = {GATE_MAP["reset"]: "resetz"}


# ---------------------------------------------------------------------------
# Operator tables
# ---------------------------------------------------------------------------

# opcode -> (operands are signed, binary operation)
_INT_BINARY: Dict[int, Tuple[bool, Callable[[int, int], int]]] = {
    OP_ADD: (True, operator.add),
    OP_SUB: (True, operator.sub),
    OP_MUL: (True, operator.mul),
    OP_UDIV: (False, operator.floordiv),
    OP_SDIV: (True, _trunc_div),
    OP_UREM: (False, operator.mod),
    OP_SREM: (True, _trunc_rem),
    OP_AND: (False, operator.and_),
    OP_OR: (False, operator.or_),
    OP_XOR: (False, operator.xor),
    OP_SHL: (False, lambda a, b: a << (b & 63)),
    OP_LSHR: (False, lambda a, b: a >> (b & 63)),
    OP_ASHR: (True, lambda a, b: a >> (b & 63)),
}

_FLOAT_BINARY: Dict[int, Callable[[float, float], float]] = {
    OP_FADD: operator.add,
    OP_FSUB: operator.sub,
    OP_FMUL: operator.mul,
    OP_FDIV: _fdiv,
    OP_FREM: _frem,
}

# condition code -> (operands are signed, comparison)
_ICMP_OPS: Dict[int, Tuple[bool, Callable[[int, int], bool]]] = {
    ICMP_EQ: (True, operator.eq),
    ICMP_NE: (True, operator.ne),
    ICMP_SLT: (True, operator.lt),
    ICMP_SLE: (True, operator.le),
    ICMP_SGT: (True, operator.gt),
    ICMP_SGE: (True, operator.ge),
    ICMP_ULT: (False, operator.lt),
    ICMP_ULE: (False, operator.le),
    ICMP_UGT: (False, operator.gt),
    ICMP_UGE: (False, operator.ge),
}

_FCMP_OPS: Dict[int, Callable[[float, float], bool]] = {
    FCMP_OEQ: operator.eq,
    FCMP_ONE: operator.ne,
    FCMP_OLT: operator.lt,
    FCMP_OLE: operator.le,
    FCMP_OGT: operator.gt,
    FCMP_OGE: operator.ge,
}


class _Interpreter:
    """Executes one shot. One instance per shot; not reusable."""

    def __init__(self, program: AdaptiveProgram, sink: QuantumSink) -> None:
        self._program = program
        self._sink = sink
        self._instructions = program.instructions
        # The emitter assigns block IDs densely in emission order, so a block's
        # position in ``blocks`` is its ID -- the same assumption the Rust FFI
        # makes when it drops the ID and keeps a positional table.
        self._block_offsets = [block.instr_offset for block in program.blocks]
        self._registers = [0] * program.num_registers
        self._memory = list(program.constant_data)
        # (return block, return pc, return register)
        self._call_stack: List[Tuple[int, int, int]] = []
        self._records: List[OutputRecordValue] = []
        self._block = program.entry_block
        self._previous_block = 0
        self._pc = self._block_offsets[program.entry_block]
        self.exit_code = 0

    # -- operand resolution --------------------------------------------------

    def _u(self, operand: int, flags: int, index: int) -> int:
        if flags & _IMM_FLAGS[index]:
            return operand & _MASK64
        return self._registers[operand]

    def _i(self, operand: int, flags: int, index: int) -> int:
        return _as_i64(self._u(operand, flags, index))

    def _f(self, operand: int, flags: int, index: int) -> float:
        return _bits_to_f64(self._u(operand, flags, index))

    def _jump_to(self, block_id: int) -> None:
        self._previous_block = self._block
        self._block = block_id
        self._pc = self._block_offsets[block_id]

    # -- control flow --------------------------------------------------------

    def _op_nop(self, instr: Instruction) -> None:
        self._pc += 1

    def _op_ret(self, instr: Instruction) -> bool:
        self.exit_code = self._u(instr.dst, instr.opcode, 2)
        return True

    def _op_jump(self, instr: Instruction) -> None:
        self._jump_to(instr.dst)

    def _op_branch(self, instr: Instruction) -> None:
        taken = self._u(instr.src0, instr.opcode, 0) != 0
        self._jump_to(instr.aux0 if taken else instr.aux1)

    def _op_switch(self, instr: Instruction) -> None:
        value = self._u(instr.src0, instr.opcode, 0)
        target = instr.aux0
        cases = self._program.switch_cases
        for i in range(instr.aux1, instr.aux1 + instr.aux2):
            if cases[i].case_val == value:
                target = cases[i].target_block
                break
        self._jump_to(target)

    def _op_call(self, instr: Instruction) -> None:
        function = self._program.functions[instr.aux0]
        self._call_stack.append((self._block, self._pc + 1, instr.dst))
        args = self._program.call_args
        for i in range(instr.aux1):
            self._registers[function.param_base + i] = self._registers[
                args[instr.aux2 + i]
            ]
        self._block = function.func_entry_block
        self._pc = self._block_offsets[self._block]

    def _op_call_return(self, instr: Instruction) -> None:
        self._block, self._pc, return_reg = self._call_stack.pop()
        if return_reg != _VOID_RETURN:
            self._registers[return_reg] = self._u(instr.src0, instr.opcode, 0)

    # -- quantum -------------------------------------------------------------

    def _op_quantum_gate(self, instr: Instruction) -> None:
        op = self._program.quantum_ops[instr.aux0]
        op_id = op.op_id
        sink = self._sink
        if op_id == CORRELATED_NOISE_OP_ID:
            count = self._u(instr.aux1, instr.opcode, 4)
            offset = self._u(instr.aux2, instr.opcode, 5)
            args = self._program.call_args
            targets = [self._registers[args[offset + i]] for i in range(count)]
            sink.correlated_noise_intrinsic(op.q1, targets)
        elif op_id == INSTRUCTION_CALL_OP_ID:
            sink.instruction(*self._instruction_call(op.q1, instr))
        elif op_id in _ONE_QUBIT_GATES:
            getattr(sink, _ONE_QUBIT_GATES[op_id])(self._u(instr.aux1, instr.opcode, 4))
        elif op_id in _TWO_QUBIT_GATES:
            getattr(sink, _TWO_QUBIT_GATES[op_id])(
                self._u(instr.aux1, instr.opcode, 4),
                self._u(instr.aux2, instr.opcode, 5),
            )
        elif op_id in _ONE_QUBIT_ROTATIONS:
            getattr(sink, _ONE_QUBIT_ROTATIONS[op_id])(
                self._f(instr.src0, instr.opcode, 0),
                self._u(instr.aux1, instr.opcode, 4),
            )
        elif op_id in _TWO_QUBIT_ROTATIONS:
            getattr(sink, _TWO_QUBIT_ROTATIONS[op_id])(
                self._f(instr.src0, instr.opcode, 0),
                self._u(instr.aux1, instr.opcode, 4),
                self._u(instr.aux2, instr.opcode, 5),
            )
        else:
            raise ValueError(f"unsupported quantum gate op_id={op_id}")
        self._pc += 1

    def _instruction_call(
        self, site_index: int, instr: Instruction
    ) -> Tuple[InstructionCall, Tuple[int, ...]]:
        if not isinstance(self._program, QodecProgram):
            raise ValueError("Instruction calls require a qodec-compiled program")
        site = self._program.instruction_calls[site_index]
        count = self._u(instr.aux1, instr.opcode, 4)
        offset = self._u(instr.aux2, instr.opcode, 5)
        args = self._program.call_args
        operands: List[int | str] = []
        results: List[int] = []
        values: List[bool | int | float] = []
        for i, kind in zip(range(count), site.arguments):
            value = self._registers[args[offset + i]]
            if kind == "qubit":
                operands.append(value)
            elif kind == "result":
                results.append(value)
            elif kind == "bool":
                values.append(value != 0)
            elif kind == "int":
                values.append(_as_i64(value))
            else:
                values.append(_bits_to_f64(value))
        call = InstructionCall(
            site.mnemonic,
            operands=operands,
            arguments=dict(zip(site.parameters, values)),
        )
        if site.selected_flags:
            call.select = [dict.fromkeys(site.selected_flags, 0)]
        return call, tuple(results)

    def _op_measure(self, instr: Instruction) -> None:
        op_id = self._program.quantum_ops[instr.aux0].op_id
        if op_id not in _MEASURE_OPS:
            raise ValueError(f"unsupported measure op_id={op_id}")
        getattr(self._sink, _MEASURE_OPS[op_id])(
            self._u(instr.aux1, instr.opcode, 4),
            self._u(instr.aux2, instr.opcode, 5),
        )
        self._pc += 1

    def _op_reset(self, instr: Instruction) -> None:
        op_id = self._program.quantum_ops[instr.aux0].op_id
        if op_id not in _RESET_OPS:
            raise ValueError(f"unsupported reset op_id={op_id}")
        getattr(self._sink, _RESET_OPS[op_id])(self._u(instr.aux1, instr.opcode, 4))
        self._pc += 1

    def _op_read_result(self, instr: Instruction) -> None:
        result = self._sink.result(self._u(instr.src0, instr.opcode, 0))
        self._registers[instr.dst] = 1 if result == Result.One else 0
        self._pc += 1

    def _op_read_loss(self, instr: Instruction) -> None:
        result = self._sink.result(self._u(instr.src0, instr.opcode, 0))
        self._registers[instr.dst] = 1 if result == Result.Loss else 0
        self._pc += 1

    def _op_peek_loss(self, instr: Instruction) -> None:
        self._sink.peek_loss(
            self._u(instr.aux0, instr.opcode, 3),
            self._u(instr.aux1, instr.opcode, 4),
        )
        self._pc += 1

    def _op_readout_noise(self, instr: Instruction) -> None:
        self._sink.apply_readout_noise(
            self._f(instr.aux0, instr.opcode, 3),
            self._f(instr.aux1, instr.opcode, 4),
            self._u(instr.aux2, instr.opcode, 5),
        )
        self._pc += 1

    def _op_record_output(self, instr: Instruction) -> None:
        # Array and tuple records are structural markers rebuilt by the host
        # from the static QIR, so they contribute no value here.
        kind = instr.aux1
        if kind == _RECORD_RESULT:
            self._records.append(
                self._sink.result(self._u(instr.src0, instr.opcode, 0))
            )
        elif kind == _RECORD_BOOL:
            self._records.append(self._u(instr.src0, instr.opcode, 0) != 0)
        elif kind == _RECORD_INT:
            self._records.append(self._i(instr.src0, instr.opcode, 0))
        elif kind == _RECORD_DOUBLE:
            self._records.append(self._f(instr.src0, instr.opcode, 0))
        self._pc += 1

    # -- arithmetic and comparison -------------------------------------------

    def _op_int_binary(self, instr: Instruction) -> None:
        signed, apply = _INT_BINARY[instr.opcode & 0xFF]
        resolve = self._i if signed else self._u
        a = resolve(instr.src0, instr.opcode, 0)
        b = resolve(instr.src1, instr.opcode, 1)
        self._registers[instr.dst] = apply(a, b) & _MASK64
        self._pc += 1

    def _op_float_binary(self, instr: Instruction) -> None:
        apply = _FLOAT_BINARY[instr.opcode & 0xFF]
        a = self._f(instr.src0, instr.opcode, 0)
        b = self._f(instr.src1, instr.opcode, 1)
        self._registers[instr.dst] = _f64_to_bits(apply(a, b))
        self._pc += 1

    def _op_icmp(self, instr: Instruction) -> None:
        code = (instr.opcode >> 8) & 0xFF
        if code not in _ICMP_OPS:
            raise ValueError(f"unsupported icmp condition code {code}")
        signed, compare = _ICMP_OPS[code]
        resolve = self._i if signed else self._u
        a = resolve(instr.src0, instr.opcode, 0)
        b = resolve(instr.src1, instr.opcode, 1)
        self._registers[instr.dst] = int(compare(a, b))
        self._pc += 1

    def _op_fcmp(self, instr: Instruction) -> None:
        code = (instr.opcode >> 8) & 0xFF
        if code not in _FCMP_OPS:
            raise ValueError(f"unsupported fcmp condition code {code}")
        a = self._f(instr.src0, instr.opcode, 0)
        b = self._f(instr.src1, instr.opcode, 1)
        self._registers[instr.dst] = int(_FCMP_OPS[code](a, b))
        self._pc += 1

    # -- conversion ----------------------------------------------------------

    def _op_bit_copy(self, instr: Instruction) -> None:
        """``zext``/``trunc``/``inttoptr``/``fpext``/``fptrunc``/``mov``.

        All of these are width changes between representations that share the
        same 64-bit register encoding, so the bits pass through unchanged.
        """
        self._registers[instr.dst] = self._u(instr.src0, instr.opcode, 0)
        self._pc += 1

    def _op_sext(self, instr: Instruction) -> None:
        value = self._i(instr.src0, instr.opcode, 0)
        src_bits = instr.aux0
        if 0 < src_bits < 64:
            shift = 64 - src_bits
            value = _as_i64(value << shift) >> shift
        self._registers[instr.dst] = value & _MASK64
        self._pc += 1

    def _op_fptosi(self, instr: Instruction) -> None:
        value = _f64_as_i64(self._f(instr.src0, instr.opcode, 0))
        self._registers[instr.dst] = value & _MASK64
        self._pc += 1

    def _op_fptoui(self, instr: Instruction) -> None:
        self._registers[instr.dst] = _f64_as_u64(self._f(instr.src0, instr.opcode, 0))
        self._pc += 1

    def _op_sitofp(self, instr: Instruction) -> None:
        value = float(self._i(instr.src0, instr.opcode, 0))
        self._registers[instr.dst] = _f64_to_bits(value)
        self._pc += 1

    def _op_uitofp(self, instr: Instruction) -> None:
        value = float(self._u(instr.src0, instr.opcode, 0))
        self._registers[instr.dst] = _f64_to_bits(value)
        self._pc += 1

    # -- SSA and data movement -----------------------------------------------

    def _op_phi(self, instr: Instruction) -> None:
        entries = self._program.phi_entries
        for i in range(instr.aux0, instr.aux0 + instr.aux1):
            if entries[i].block_id == self._previous_block:
                self._registers[instr.dst] = self._registers[entries[i].val_reg]
                break
        self._pc += 1

    def _op_select(self, instr: Instruction) -> None:
        taken = self._u(instr.src0, instr.opcode, 0) != 0
        index, operand = (3, instr.aux0) if taken else (4, instr.aux1)
        self._registers[instr.dst] = self._u(operand, instr.opcode, index)
        self._pc += 1

    def _op_const(self, instr: Instruction) -> None:
        self._registers[instr.dst] = instr.src0
        self._pc += 1

    # -- memory --------------------------------------------------------------

    def _op_alloca(self, instr: Instruction) -> None:
        num_words = self._u(instr.src0, instr.opcode, 0)
        address = self._u(instr.src1, instr.opcode, 1)
        end = address + num_words
        if end > len(self._memory):
            self._memory.extend([0] * (end - len(self._memory)))
        self._registers[instr.dst] = address
        self._pc += 1

    def _op_load(self, instr: Instruction) -> None:
        self._registers[instr.dst] = self._memory[self._u(instr.src0, instr.opcode, 0)]
        self._pc += 1

    def _op_store(self, instr: Instruction) -> None:
        value = self._u(instr.src0, instr.opcode, 0)
        address = self._u(instr.src1, instr.opcode, 1)
        if address >= len(self._memory):
            self._memory.extend([0] * (address + 1 - len(self._memory)))
        self._memory[address] = value
        self._pc += 1

    def _op_gep(self, instr: Instruction) -> None:
        base = self._u(instr.src0, instr.opcode, 0)
        index = self._u(instr.src1, instr.opcode, 1)
        element_size = self._u(instr.aux0, instr.opcode, 3)
        self._registers[instr.dst] = (base + index * element_size) & _MASK64
        self._pc += 1

    # -- driver --------------------------------------------------------------

    @property
    def records(self) -> list[OutputRecordValue]:
        return list(self._records)

    def step(self) -> bool:
        instruction = self._instructions[self._pc]
        handler = _DISPATCH.get(instruction.opcode & 0xFF)
        if handler is None:
            raise ValueError(
                f"Unsupported opcode {instruction.opcode & 0xFF:#04x} at pc={self._pc}"
            )
        return bool(handler(self, instruction))


_DISPATCH: Dict[int, Callable[[_Interpreter, Instruction], object]] = {
    OP_NOP: _Interpreter._op_nop,
    OP_RET: _Interpreter._op_ret,
    OP_JUMP: _Interpreter._op_jump,
    OP_BRANCH: _Interpreter._op_branch,
    OP_SWITCH: _Interpreter._op_switch,
    OP_CALL: _Interpreter._op_call,
    OP_CALL_RETURN: _Interpreter._op_call_return,
    OP_QUANTUM_GATE: _Interpreter._op_quantum_gate,
    OP_MEASURE: _Interpreter._op_measure,
    OP_RESET: _Interpreter._op_reset,
    OP_READ_RESULT: _Interpreter._op_read_result,
    OP_READ_LOSS: _Interpreter._op_read_loss,
    OP_PEEK_LOSS: _Interpreter._op_peek_loss,
    OP_READOUT_NOISE: _Interpreter._op_readout_noise,
    OP_RECORD_OUTPUT: _Interpreter._op_record_output,
    OP_ICMP: _Interpreter._op_icmp,
    OP_FCMP: _Interpreter._op_fcmp,
    OP_SEXT: _Interpreter._op_sext,
    OP_FPTOSI: _Interpreter._op_fptosi,
    OP_FPTOUI: _Interpreter._op_fptoui,
    OP_SITOFP: _Interpreter._op_sitofp,
    OP_UITOFP: _Interpreter._op_uitofp,
    OP_PHI: _Interpreter._op_phi,
    OP_SELECT: _Interpreter._op_select,
    OP_CONST: _Interpreter._op_const,
    OP_ALLOCA: _Interpreter._op_alloca,
    OP_LOAD: _Interpreter._op_load,
    OP_STORE: _Interpreter._op_store,
    OP_GEP: _Interpreter._op_gep,
    **{opcode: _Interpreter._op_int_binary for opcode in _INT_BINARY},
    **{opcode: _Interpreter._op_float_binary for opcode in _FLOAT_BINARY},
    **{
        opcode: _Interpreter._op_bit_copy
        for opcode in (OP_ZEXT, OP_TRUNC, OP_INTTOPTR, OP_FPEXT, OP_FPTRUNC, OP_MOV)
    },
}
