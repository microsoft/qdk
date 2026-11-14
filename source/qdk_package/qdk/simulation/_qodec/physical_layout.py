from dataclasses import dataclass
from heapq import heappop, heappush

from .call_binding import BoundCall


@dataclass(frozen=True)
class PhysicalBlock:
    block_type: str
    qubits: tuple[int, ...]


class PhysicalLayout:
    def __init__(self, capacity: int) -> None:
        self.blocks: dict[int | str, PhysicalBlock] = {}
        self.free = list(range(capacity))
        self._consumed: set[int | str] = set()

    def allocate(
        self, label: int | str, block_type: str, width: int, *, preparing: bool = False
    ) -> PhysicalBlock:
        block = self.blocks.get(label)
        if block is not None:
            if block.block_type != block_type:
                raise ValueError("Physical operand type does not match its live block")
            return block
        if label in self._consumed and not preparing:
            raise ValueError(
                f"Physical input block {label!r} was consumed and must be prepared again"
            )
        if width > len(self.free):
            raise ValueError("Physical block allocation exceeds reserved capacity")
        block = PhysicalBlock(
            block_type, tuple(heappop(self.free) for _ in range(width))
        )
        self.blocks[label] = block
        self._consumed.discard(label)
        return block

    def begin(
        self, binding: BoundCall, width: int
    ) -> tuple[tuple[int, ...], tuple[int, ...]]:
        live_width = 0
        for operand in binding.inputs:
            block = self.blocks.get(operand.label)
            if block is None:
                if operand.label in self._consumed:
                    raise ValueError(
                        f"Physical input block {operand.label!r} was consumed and must be prepared again"
                    )
            elif block.block_type != operand.block_type:
                raise ValueError(
                    f"Physical input block {operand.label!r} has the wrong live type"
                )
            else:
                live_width += len(block.qubits)
        input_labels = {operand.label for operand in binding.inputs}
        replaced = tuple(
            operand.label
            for operand in binding.outputs
            if operand.label not in input_labels and operand.label in self.blocks
        )
        discarded = tuple(
            qubit for label in replaced for qubit in self.blocks[label].qubits
        )
        if width - live_width > len(self.free) + len(discarded):
            raise ValueError("Instruction exceeds reserved physical capacity")
        for label in replaced:
            self.release(label)
        inputs = tuple(
            qubit
            for operand in binding.inputs
            for qubit in self.allocate(
                operand.label, operand.block_type, operand.width
            ).qubits
        )
        qubits = inputs + tuple(heappop(self.free) for _ in range(width - len(inputs)))
        return qubits, discarded

    def commit(self, binding: BoundCall, qubits: tuple[int, ...]) -> None:
        for operand in binding.inputs:
            self.blocks.pop(operand.label)
            self._consumed.add(operand.label)
        for operand in binding.outputs:
            self.blocks[operand.label] = PhysicalBlock(
                operand.block_type,
                qubits[operand.offset : operand.offset + operand.width],
            )
            self._consumed.discard(operand.label)
        for qubit in qubits[binding.output_capacity :]:
            heappush(self.free, qubit)

    def release(self, label: int | str) -> tuple[int, ...]:
        self._consumed.discard(label)
        block = self.blocks.pop(label, None)
        if block is None:
            return ()
        for qubit in block.qubits:
            heappush(self.free, qubit)
        return block.qubits
