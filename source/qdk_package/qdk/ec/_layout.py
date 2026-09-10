"""Program-level placement of symbolic block instances onto logical qubits."""

from __future__ import annotations

from dataclasses import dataclass

import qodec as qc
from qodec.circuits import Program

from ._operands import QubitLabel, qubit_labels


@dataclass(frozen=True)
class ProgramLayout:
    """Stable logical-qubit ranges for the block instances in a program."""

    program: Program
    instance_bases: dict[QubitLabel, int]
    total_qubits: int

    @classmethod
    def of(cls, program: Program) -> "ProgramLayout":
        blocks = {block.name: block for block in program.isa.blocks}
        bindings: list[tuple[QubitLabel, int]] = []
        for call in program.instructions:
            instruction = program.lookup(call.mnemonic)
            pairs = [
                *zip(instruction.inputs, call.inputs.values()),
                *zip(instruction.outputs, call.outputs.values()),
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
        instruction = self.program.lookup(call.mnemonic)
        operands = list(instruction.inputs) or list(instruction.outputs)
        values = list(call.inputs.values()) or list(call.outputs.values())
        blocks = {block.name: block for block in self.program.isa.blocks}
        result: dict[int, int] = {}
        flat_index = 0
        for operand, value in zip(operands, values):
            block = blocks[operand.block]
            for instance in qubit_labels(value):
                base = self.instance_bases[instance]
                for offset in range(int(block.encodes)):
                    result[flat_index] = base + offset
                    flat_index += 1
        return result

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
