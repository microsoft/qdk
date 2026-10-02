from typing import Protocol, TypeAlias
from collections.abc import Callable, Collection, Sequence

from qodec.instructions import InstructionCall

from .instruction_set import InstructionSet, UnboundOperation
from .quantum_operations import LogicalSlot, Operation, local_indices

Decompose: TypeAlias = Callable[[Operation], tuple[Operation, ...] | None]


class ResolveOperation(Protocol):
    def __call__(
        self,
        name: str,
        targets: Sequence[int | str | LogicalSlot],
        angle: float | str | None,
        *,
        spare: Collection[LogicalSlot] = (),
    ) -> tuple[InstructionCall, ...]: ...


def prepare_resolver(
    instructions: InstructionSet, decompose: Decompose
) -> ResolveOperation:
    def bind_call(
        operation: Operation,
        targets: Sequence[int | str | LogicalSlot],
        spare: Collection[LogicalSlot],
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
            operation.name, slots, operation.angle, spare=spare
        )
        call = InstructionCall(
            mnemonic,
            operands=list(operands),
            arguments=arguments,
        )
        flags = instructions.bindings[mnemonic].flags
        if flags:
            # The runtime issues this call on the program's behalf and cannot
            # act on a raised flag, so it accepts only outcomes with every
            # flag clear; anything else rejects the shot.
            call.select = [dict.fromkeys(flags, 0)]
        return call

    def resolve(
        name: str,
        targets: Sequence[int | str | LogicalSlot],
        angle: float | str | None,
        *,
        spare: Collection[LogicalSlot] = (),
    ) -> tuple[InstructionCall, ...]:
        operation = Operation(name, tuple(range(len(targets))), angle)
        try:
            return (bind_call(operation, targets, spare),)
        except UnboundOperation:
            replacement = decompose(operation)
            if replacement is None:
                raise
        return tuple(bind_call(part, targets, spare) for part in replacement)

    return resolve
