from collections import Counter
from collections.abc import Mapping, Sequence
from dataclasses import dataclass

from qodec import Code
from qodec.gadgets import Encoding
from qodec.instructions import BlockOperand

from .quantum_operations import LogicalSlot


@dataclass(frozen=True)
class EncodingLayout:
    block_type: str
    labels: tuple[str, ...]
    lower_types: tuple[str, ...]

    @property
    def resources(self) -> Counter[str]:
        return Counter(self.lower_types)

    def qubits(
        self, placement: Mapping[str, int], capacities: Mapping[str, int]
    ) -> tuple[int | LogicalSlot, ...]:
        bare = len(capacities) == 1 and next(iter(capacities.values())) == 1
        return tuple(
            (
                placement[label]
                if bare
                else LogicalSlot(placement[label], index, block_type)
            )
            for label, block_type in zip(self.labels, self.lower_types)
            for index in range(capacities[block_type])
        )


def layout_boundary(
    operands: Sequence[BlockOperand],
    encodings: Sequence[Encoding],
    codes: Mapping[str, Code],
    logical_capacities: Mapping[str, int],
    lower_capacities: Mapping[str, int],
) -> tuple[EncodingLayout, ...]:
    if len(operands) != len(encodings):
        raise ValueError("Encoding entries must match declared operand positions")
    layouts = []
    for operand, encoding in zip(operands, encodings):
        code = codes.get(operand.block)
        if code is None or encoding.code != code:
            raise ValueError(f"Encoding must use the layer code for {operand.block!r}")
        if len(code.x) != logical_capacities[operand.block] or len(code.z) != len(
            code.x
        ):
            raise ValueError("Code logical capacity must match its declared block type")
        labels = tuple(encoding.support)
        types = tuple(encoding.block_types)
        if not types and len(lower_capacities) == 1:
            types = (next(iter(lower_capacities)),) * len(labels)
        if len(types) != len(labels) or set(types) - lower_capacities.keys():
            raise ValueError("Encoding support requires declared lower block types")
        if (
            sum(lower_capacities[block_type] for block_type in types)
            != code.physical_qubit_count
        ):
            raise ValueError("Encoding support capacity must match its code qubits")
        if len(set(labels)) != len(labels):
            raise ValueError("Encoding support labels must be distinct")
        layouts.append(EncodingLayout(operand.block, labels, types))
    return tuple(layouts)
