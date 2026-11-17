"""Circuit-level placement of symbolic block instances onto logical qubits."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

import qodec as qc
from qodec.gadgets import Circuit

from ._operands import QubitLabel, qubit_labels


@dataclass(frozen=True)
class ProgramLayout:
    """Stable logical-qubit ranges for the block instances in a program."""

    program: Circuit
    instance_bases: dict[QubitLabel, int]
    total_qubits: int

    @classmethod
    def of(cls, program: Circuit) -> "ProgramLayout":
        blocks = {block.name: block for block in program.instruction_set.blocks}
        bindings: list[tuple[QubitLabel, int]] = []
        for call in program.calls():
            instruction = program.instruction_set.instructions[call.mnemonic]
            pairs = [
                *cls._bound_operands(instruction.inputs, call.operands),
                *cls._bound_operands(instruction.outputs, call.operands),
            ]
            for operand, value in pairs:
                try:
                    block = blocks[operand.block]
                except KeyError as error:
                    raise ValueError(
                        f"call {call.mnemonic!r} uses operand block "
                        f"{operand.block!r}; ISA has blocks {sorted(blocks)}"
                    ) from error
                bindings.extend(
                    (instance, int(block.encodes)) for instance in qubit_labels(value)
                )

        widths: dict[QubitLabel, int] = {}
        for instance, width in bindings:
            previous = widths.setdefault(instance, width)
            if previous != width:
                raise ValueError(
                    f"block instance {instance!r} is used with widths "
                    f"{previous} and {width}"
                )

        instance_bases: dict[QubitLabel, int] = {}
        for instance, width in widths.items():
            if isinstance(instance, int):
                instance_bases[instance] = instance * width
        next_qubit = max(
            (base + widths[instance] for instance, base in instance_bases.items()),
            default=0,
        )
        for instance, width in bindings:
            if instance in instance_bases:
                continue
            instance_bases[instance] = next_qubit
            next_qubit += width
        return cls(program, instance_bases, next_qubit)

    def call_qubit_map(self, call: qc.instructions.InstructionCall) -> dict[int, int]:
        """Map one call's flat action indices to program logical qubits."""
        instruction = self.program.instruction_set.instructions[call.mnemonic]
        inputs = self._bound_operands(instruction.inputs, call.operands)
        outputs = self._bound_operands(instruction.outputs, call.operands)
        pairs = inputs if len(inputs) >= len(outputs) else outputs
        blocks = {block.name: block for block in self.program.instruction_set.blocks}
        result: dict[int, int] = {}
        flat_index = 0
        for operand, value in pairs:
            block = blocks[operand.block]
            for instance in qubit_labels(value):
                base = self.instance_bases[instance]
                for offset in range(int(block.encodes)):
                    result[flat_index] = base + offset
                    flat_index += 1
        return result

    @staticmethod
    def _bound_operands(
        operands: Sequence[qc.instructions.BlockOperand], values: Sequence[int | str]
    ) -> list[tuple[qc.instructions.BlockOperand, int | str]]:
        variadic_count = sum(operand.is_variadic for operand in operands)
        if variadic_count > 1:
            raise ValueError("cannot bind more than one variadic operand per boundary")
        fixed_count = len(operands) - variadic_count
        if len(values) < fixed_count:
            raise ValueError("call supplies fewer blocks than its instruction declares")
        expanded = [
            operand
            for operand in operands
            for _ in range(len(values) - fixed_count if operand.is_variadic else 1)
        ]
        return list(zip(expanded, values))

    def qubit_of(self, call: qc.instructions.InstructionCall, flat_index: int) -> int:
        """Resolve one flat action index for ``call``."""
        mapping = self.call_qubit_map(call)
        try:
            return mapping[flat_index]
        except KeyError as error:
            raise ValueError(
                f"call {call.mnemonic!r}: flat logical index {flat_index} is "
                f"out of range (operands cover {len(mapping)})"
            ) from error


__all__ = ["ProgramLayout"]
