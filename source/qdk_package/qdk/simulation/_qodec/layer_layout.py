from collections import Counter
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from heapq import heappop, heappush

from .encoding_layout import EncodingLayout
from .protocols import BlockReference, Resources
from .quantum_operations import LogicalSlot


@dataclass(frozen=True)
class LiveBlock:
    reference: BlockReference
    support: tuple[int, ...]
    qubits: tuple[int | LogicalSlot, ...]


class LayerLayout:
    def __init__(self) -> None:
        self.blocks: dict[int | str, LiveBlock] = {}
        self.free: list[int] = []
        self.generation_by_label: dict[int | str, int] = {}
        self._capacity_by_type: Counter[str] = Counter()
        self._type_by_lower_block: dict[int, str] = {}

    def start(
        self,
        resources: Resources,
        lower_capacities: Mapping[str, int],
    ) -> None:
        if resources.blocks.keys() - lower_capacities.keys():
            raise ValueError("Resources name undeclared lower block types")
        self.blocks.clear()
        self._capacity_by_type = Counter(resources.blocks)
        if resources.qubits:
            if len(lower_capacities) != 1 or next(iter(lower_capacities.values())) != 1:
                raise ValueError("Mixed lower blocks require typed resource capacities")
            self._capacity_by_type[next(iter(lower_capacities))] += resources.qubits
        self.free = list(range(resources.qubits + sum(resources.blocks.values())))
        self._type_by_lower_block.clear()
        self.generation_by_label.clear()

    def ensure_gadget_fits(
        self,
        circuit_labels: Sequence[str],
        label_types: Mapping[str, str],
        input_placement: Mapping[str, int],
        output_only_targets: Sequence[int | str],
    ) -> None:
        replaced_blocks = tuple(
            self.blocks[target]
            for target in output_only_targets
            if target in self.blocks
        )
        available_lower_blocks = len(self.free) + sum(
            len(block.support) for block in replaced_blocks
        )
        if (
            sum(label not in input_placement for label in circuit_labels)
            > available_lower_blocks
        ):
            raise ValueError("Gadget exceeds the reserved lower-block capacity")

        allocated_after_call = Counter(self._type_by_lower_block.values())
        for block in replaced_blocks:
            allocated_after_call.subtract(
                self._type_by_lower_block[lower_block] for lower_block in block.support
            )
        for label in circuit_labels:
            if label in input_placement:
                allocated_after_call[
                    self._type_by_lower_block[input_placement[label]]
                ] -= 1
            allocated_after_call[label_types[label]] += 1
        if allocated_after_call - self._capacity_by_type:
            raise ValueError("Gadget exceeds the reserved lower-block type capacity")

    def allocate_circuit_blocks(
        self,
        circuit_labels: Sequence[str],
        label_types: Mapping[str, str],
        input_placement: dict[str, int],
    ) -> dict[str, int]:
        for label in circuit_labels:
            if label not in input_placement:
                input_placement[label] = heappop(self.free)
                self._type_by_lower_block[input_placement[label]] = label_types[label]
        return input_placement

    def reference_for_output(self, label: int | str, block_type: str) -> BlockReference:
        if (
            label in self.blocks
            and self.blocks[label].reference.block_type == block_type
        ):
            return self.blocks[label].reference
        generation = self.generation_by_label.get(label, 0) + 1
        self.generation_by_label[label] = generation
        return BlockReference(label, generation, block_type)

    def publish_outputs(
        self,
        input_targets: Sequence[int | str],
        output_references: Sequence[BlockReference],
        output_layouts: Sequence[EncodingLayout],
        circuit_labels: Sequence[str],
        placement: Mapping[str, int],
        label_types: Mapping[str, str],
        lower_capacities: Mapping[str, int],
    ) -> set[int]:
        self._type_by_lower_block.update(
            (placement[label], label_types[label]) for label in circuit_labels
        )
        for target in input_targets:
            self.blocks.pop(target)
        retained_lower_blocks: set[int] = set()
        for reference, output_layout in zip(output_references, output_layouts):
            support = tuple(placement[label] for label in output_layout.labels)
            self.blocks[reference.label] = LiveBlock(
                reference,
                support,
                output_layout.qubits(placement, lower_capacities),
            )
            retained_lower_blocks.update(support)
        return retained_lower_blocks

    def remove_block(self, label: int | str) -> LiveBlock | None:
        return self.blocks.pop(label, None)

    def address_for_discard(
        self,
        lower_block: int,
        lower_capacities: Mapping[str, int],
    ) -> int | LogicalSlot:
        block_type = self._type_by_lower_block[lower_block]
        if len(lower_capacities) == 1 and lower_capacities[block_type] == 1:
            return lower_block
        return LogicalSlot(lower_block, 0, block_type)

    def finish_discard(self, lower_block: int) -> None:
        self._type_by_lower_block.pop(lower_block)
        heappush(self.free, lower_block)

    def qubits_for_correction(
        self, references: Sequence[BlockReference]
    ) -> tuple[int | LogicalSlot, ...]:
        qubits = []
        for reference in references:
            block = self.blocks.get(reference.label)
            if block is None or block.reference != reference:
                raise ValueError("Correction targets a block that is no longer live")
            qubits.extend(block.qubits)
        return tuple(qubits)

    def clear(self) -> None:
        self.blocks.clear()
        self.free.clear()
        self.generation_by_label.clear()
        self._type_by_lower_block.clear()
        self._capacity_by_type.clear()
