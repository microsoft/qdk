from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from math import isfinite

from qodec import Instruction, InstructionSet
from qodec.instructions import BlockOperand, InstructionCall, Parameter


@dataclass(frozen=True)
class BoundOperand:
    label: int | str
    block_type: str
    offset: int
    width: int


@dataclass(frozen=True)
class BoundCall:
    instruction: Instruction
    inputs: tuple[BoundOperand, ...]
    outputs: tuple[BoundOperand, ...]

    @property
    def input_capacity(self) -> int:
        return sum(operand.width for operand in self.inputs)

    @property
    def output_capacity(self) -> int:
        return sum(operand.width for operand in self.outputs)


def bind_call(
    instruction_set: InstructionSet,
    call: InstructionCall,
    *,
    input_types: Mapping[int | str, str] | None = None,
    output_types: Mapping[int | str, str] | None = None,
) -> BoundCall:
    bound = bind_operands(
        instruction_set,
        call.mnemonic,
        call.operands,
        input_types=input_types,
        output_types=output_types,
    )
    validate_arguments(bound.instruction, call.arguments)
    return bound


def bind_operands(
    instruction_set: InstructionSet,
    mnemonic: str,
    targets: Sequence[int | str],
    *,
    input_types: Mapping[int | str, str] | None = None,
    output_types: Mapping[int | str, str] | None = None,
) -> BoundCall:
    try:
        instruction = instruction_set.instructions[mnemonic]
    except KeyError as error:
        raise ValueError(f"Unknown instruction {mnemonic!r}") from error
    if any(
        not (isinstance(target, str) or type(target) is int and target >= 0)
        for target in targets
    ):
        raise ValueError("Block operands must be non-negative integers or names")
    if len({str(target) for target in targets}) != len(targets):
        raise ValueError("Gadget operands must be distinct blocks")
    capacities = {block.name: block.encodes for block in instruction_set.blocks}
    inputs = _bind_side(instruction.inputs, targets, capacities, input_types or {})
    outputs = _bind_side(instruction.outputs, targets, capacities, output_types or {})
    if len(targets) != max(len(inputs), len(outputs)):
        raise ValueError(f"Wrong operand count for {mnemonic!r}")
    return BoundCall(instruction, inputs, outputs)


def _bind_side(
    operands: Sequence[BlockOperand],
    targets: Sequence[int | str],
    capacities: Mapping[str, int],
    known_types: Mapping[int | str, str],
) -> tuple[BoundOperand, ...]:
    count = (
        len(targets)
        if any(operand.is_variadic for operand in operands)
        else len(operands)
    )
    if count > len(targets):
        raise ValueError("Wrong operand count")
    candidates: set[tuple[str, ...]] = set()

    def expand(index: int, types: tuple[str, ...]) -> None:
        if len(candidates) > 1:
            return
        if index == len(operands):
            if len(types) == count:
                candidates.add(types)
            return
        operand = operands[index]
        if operand.block not in capacities:
            raise ValueError(f"Undeclared operand block type {operand.block!r}")
        remaining = count - len(types)
        for size in range(remaining + 1) if operand.is_variadic else (1,):
            if size <= remaining and all(
                known_types.get(target, operand.block) == operand.block
                for target in targets[len(types) : len(types) + size]
            ):
                expand(index + 1, types + (operand.block,) * size)

    expand(0, ())
    if not candidates:
        raise ValueError("No matching operand types or count")
    if len(candidates) != 1:
        raise ValueError(
            "Ambiguous variadic operand types; supply concrete block types"
        )
    offset = 0
    bound = []
    for target, block_type in zip(targets, candidates.pop()):
        width = capacities[block_type]
        bound.append(BoundOperand(target, block_type, offset, width))
        offset += width
    return tuple(bound)


def validate_arguments(
    instruction: Instruction, arguments: Mapping[str, InstructionCall.Argument]
) -> None:
    parameters = {
        parameter.name: parameter.kind for parameter in instruction.parameters
    }
    unknown = arguments.keys() - parameters.keys()
    if unknown:
        raise ValueError(
            f"Unknown parameter {min(unknown)!r} for {instruction.mnemonic!r}"
        )
    missing = parameters.keys() - arguments.keys()
    if missing:
        raise ValueError(
            f"Missing parameter {min(missing)!r} for {instruction.mnemonic!r}"
        )
    for name, value in arguments.items():
        kind = parameters[name]
        values = value if isinstance(value, list) else [value]
        if not all(_matches(kind, item) for item in values):
            raise TypeError(f"Parameter {name!r} expects {kind.value}")


def _matches(kind: Parameter.Kind, value: object) -> bool:
    match kind:
        case Parameter.Kind.NUMBER:
            return (
                isinstance(value, (int, float))
                and not isinstance(value, bool)
                and isfinite(value)
            )
        case Parameter.Kind.INTEGER:
            return type(value) is int
        case Parameter.Kind.BIT:
            return type(value) is bool or type(value) is int and value in (0, 1)
        case Parameter.Kind.BOOLEAN:
            return type(value) is bool
        case Parameter.Kind.STRING | Parameter.Kind.PAULI:
            return isinstance(value, str) and not value.startswith("circuit.readouts[")
    return False
