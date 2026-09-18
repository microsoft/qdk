from typing import TypeAlias
from collections.abc import Callable, Sequence

from qodec.instructions import InstructionCall

from .instruction_set import InstructionSet, UnboundOperation
from .quantum_operations import LogicalSlot, Operation, local_indices

Decompose: TypeAlias = Callable[[Operation], tuple[Operation, ...] | None]
ResolveOperation: TypeAlias = Callable[
    [str, Sequence[int | str | LogicalSlot], float | str | None],
    tuple[InstructionCall, ...],
]


def prepare_resolver(
    instructions: InstructionSet, decompose: Decompose
) -> ResolveOperation:
    def bind_call(
        operation: Operation, targets: Sequence[int | str | LogicalSlot]
    ) -> InstructionCall:
        selected = tuple(targets[index] for index in local_indices(operation))
        slots = tuple(
            (
                target
                if isinstance(target, LogicalSlot)
                else LogicalSlot(target, 0, instructions.block_type)
            )
            for target in selected
        )
        mnemonic, operands, arguments = instructions.bind_slots(
            operation.name, slots, operation.angle
        )
        return InstructionCall(
            mnemonic,
            operands=list(operands),
            arguments=arguments,
        )

    def resolve(
        name: str, targets: Sequence[int | str | LogicalSlot], angle: float | str | None
    ) -> tuple[InstructionCall, ...]:
        operation = Operation(name, tuple(range(len(targets))), angle)
        try:
            return (bind_call(operation, targets),)
        except UnboundOperation:
            replacement = decompose(operation)
            if replacement is None:
                raise
        return tuple(bind_call(part, targets) for part in replacement)

    return resolve
