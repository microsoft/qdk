from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field, fields
from typing import Literal, TypeAlias

import pyqir
from pyqir import Module
from qodec import Instruction
from qodec.instructions import Parameter

from ..._adaptive_bytecode import FLAG_SRC0_IMM, OP_MOV, OP_QUANTUM_GATE
from ..._adaptive_pass import (
    AdaptiveProfilePass,
    AdaptiveProgram,
    Bytecode,
    IntOperand,
    Reg,
)

# Quantum op ID of a call to an ISA instruction by name. Only the qodec
# interpreter reads it, so it stays clear of the shared simulator op IDs.
INSTRUCTION_CALL_OP_ID = 132

ArgumentKind: TypeAlias = Literal["qubit", "result", "bool", "int", "double"]

_QIS_PREFIX = "__quantum__qis__"

# QIS calls that are classical or scheduling hints rather than quantum operations.
_CLASSICAL_QIS = frozenset(
    {
        "__quantum__qis__read_result__body",
        "__quantum__qis__barrier__body",
        "__quantum__qis__peek_loss__body",
    }
)

_PARAMETER_KINDS: Mapping[str, frozenset[Parameter.Kind]] = {
    "bool": frozenset({Parameter.Kind.BOOLEAN, Parameter.Kind.BIT}),
    "int": frozenset(
        {Parameter.Kind.INTEGER, Parameter.Kind.NUMBER, Parameter.Kind.BIT}
    ),
    "double": frozenset({Parameter.Kind.NUMBER}),
}


class UnknownInstruction(ValueError):
    pass


@dataclass(frozen=True)
class InstructionCallSite:
    """A QIR call site that invokes an ISA instruction by name."""

    mnemonic: str
    arguments: tuple[ArgumentKind, ...]
    parameters: tuple[str, ...]
    flags: tuple[str, ...]


@dataclass
class QodecProgram(AdaptiveProgram):
    instruction_calls: list[InstructionCallSite] = field(default_factory=list)


def instruction_name(callee: str) -> str | None:
    """The ISA mnemonic a QIR quantum call names, or ``None`` for runtime calls.

    ``__quantum__qis__name__body`` names ``name`` and any other QIS callee
    drops only the prefix, so ``__quantum__qis__s__adj`` names ``s__adj``.
    Other ``__quantum__`` callees are runtime functions; any remaining
    callee, such as a Q# ``body intrinsic`` operation, is its own name.
    """
    if callee in _CLASSICAL_QIS:
        return None
    if callee.startswith(_QIS_PREFIX):
        name = callee[len(_QIS_PREFIX) :]
        return name[: -len("__body")] if name.endswith("__body") else name
    if callee.startswith("__quantum__"):
        return None
    return callee


def compile(
    qir_mod: Module, instructions: Mapping[str, Instruction] | None = None
) -> QodecProgram:
    """Compile adaptive QIR for the qodec interpreter.

    Without ``instructions``, standard QIR gates lower to the operations the
    adaptive pass defines. With ``instructions``, every QIR quantum call
    invokes the instruction its callee names (see :func:`instruction_name`),
    and a QIS callee with no such instruction is an error. Other declared
    callees naming an instruction, such as Q# ``body intrinsic`` operations,
    invoke it too. As in QIR measurements, including Q# ``@Measurement()``
    intrinsics, the last pointer arguments are results that receive the
    instruction's outcomes; the pointers before them are qubits bound in order
    to its block operands. The remaining arguments bind the instruction's
    parameters in declaration order.
    """
    if instructions is None:
        adaptive_pass: AdaptiveProfilePass = AdaptiveProfilePass(Bytecode.Bit64)
        calls: list[InstructionCallSite] = []
    else:
        adaptive_pass = _QodecPass(Bytecode.Bit64, instructions)
        calls = adaptive_pass.instruction_calls
    program = adaptive_pass.run(qir_mod)
    return QodecProgram(
        **{item.name: getattr(program, item.name) for item in fields(AdaptiveProgram)},
        instruction_calls=calls,
    )


class _QodecPass(AdaptiveProfilePass):
    def __init__(
        self, bytecode_kind: Bytecode, instructions: Mapping[str, Instruction]
    ) -> None:
        super().__init__(bytecode_kind)
        self._instruction_set = instructions
        self.instruction_calls: list[InstructionCallSite] = []

    def _emit_call(self, call: pyqir.Call) -> None:
        callee = call.callee.name
        name = None if callee in self._func_to_id else instruction_name(callee)
        if name is not None and name in self._instruction_set:
            self._emit_instruction_call(call, self._instruction_set[name])
        elif name is not None and callee.startswith(_QIS_PREFIX):
            raise UnknownInstruction(
                f"QIR call {callee!r} requires an instruction named {name!r} "
                "in the qodec's top instruction set"
            )
        else:
            super()._emit_call(call)

    def _emit_instruction_call(
        self, call: pyqir.Call, instruction: Instruction
    ) -> None:
        kinds = _argument_kinds(instruction, [arg.type for arg in call.args])
        _check_signature(instruction, kinds)
        offset = len(self.call_args)
        for arg in call.args:
            operand = self._resolve_operand(arg)
            if not isinstance(operand, Reg):
                # Call arguments are register indices, so immediates need a register.
                register = self._alloc_reg(None, self._type_tag(arg.type))
                self._emit(OP_MOV | FLAG_SRC0_IMM, dst=register, src0=operand.val)
                operand = register
            self.call_args.append(operand.val)
        site = len(self.instruction_calls)
        self.instruction_calls.append(
            InstructionCallSite(
                instruction.mnemonic,
                kinds,
                tuple(parameter.name for parameter in instruction.parameters),
                tuple(instruction.flags),
            )
        )
        qop_idx = self._emit_quantum_op(INSTRUCTION_CALL_OP_ID, site, len(call.args))
        self._emit(
            OP_QUANTUM_GATE,
            aux0=qop_idx,
            aux1=IntOperand(len(call.args), self._int_bits),
            aux2=IntOperand(offset, self._int_bits),
        )


def _argument_kinds(
    instruction: Instruction, types: Sequence[pyqir.Type]
) -> tuple[ArgumentKind, ...]:
    # Opaque pointers do not say whether they point to a qubit or a result.
    kinds: list[ArgumentKind] = []
    for ty in types:
        if isinstance(ty, pyqir.PointerType):
            kinds.append("qubit")
        elif isinstance(ty, pyqir.IntType):
            kinds.append("bool" if ty.width == 1 else "int")
        elif ty.is_double:
            kinds.append("double")
        else:
            raise TypeError(
                f"QIR calls to instruction {instruction.mnemonic!r} accept only "
                "qubit, result, integer, Boolean, and double arguments"
            )
    pointers = [index for index, kind in enumerate(kinds) if kind == "qubit"]
    outcomes = instruction.observe_count
    if len(pointers) < outcomes:
        raise ValueError(
            f"Instruction {instruction.mnemonic!r} reports {outcomes} outcomes, "
            f"but the QIR call passes only {len(pointers)} qubits and results"
        )
    for index in pointers[len(pointers) - outcomes :]:
        kinds[index] = "result"
    return tuple(kinds)


def _check_signature(instruction: Instruction, kinds: tuple[ArgumentKind, ...]) -> None:
    mnemonic = instruction.mnemonic
    operands = (*instruction.inputs, *instruction.outputs)
    if not any(operand.is_variadic for operand in operands):
        expected = max(len(instruction.inputs), len(instruction.outputs))
        if kinds.count("qubit") != expected:
            raise ValueError(
                f"Instruction {mnemonic!r} takes {expected} block operands and "
                f"reports {instruction.observe_count} outcomes, but the QIR call "
                f"passes {kinds.count('qubit') + kinds.count('result')} qubits "
                "and results"
            )
    classical = [kind for kind in kinds if kind not in ("qubit", "result")]
    parameters = instruction.parameters
    if len(classical) != len(parameters):
        raise ValueError(
            f"Instruction {mnemonic!r} takes {len(parameters)} parameters, but "
            f"the QIR call passes {len(classical)} classical arguments"
        )
    for kind, parameter in zip(classical, parameters):
        if parameter.kind not in _PARAMETER_KINDS[kind]:
            raise TypeError(
                f"Parameter {parameter.name!r} of {mnemonic!r} expects "
                f"{parameter.kind.value}, but the QIR call passes {kind}"
            )
