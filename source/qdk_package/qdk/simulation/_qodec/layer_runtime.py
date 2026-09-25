from __future__ import annotations

from collections import Counter
from collections.abc import Mapping, Sequence
from contextlib import ExitStack, closing
from dataclasses import dataclass
from functools import cached_property
from typing import TypeVar, cast

from paulimer import DensePauli
from qodec import Gadget, Layer
from qodec.instructions import InstructionCall

from .call_binding import BoundCall, InstructionBinding
from .circuit_runtime import prepare_circuit as prepare_body
from .clifford_semantics import pauli
from .encoding_layout import EncodingLayout, layout_boundary
from .instruction_set import InstructionSet, UnboundOperation
from .layer_layout import LayerLayout, LiveBlock
from .operation_resolution import ResolveOperation, prepare_resolver
from .protocols import (
    BeforeInvocation,
    BlockObserver,
    BlockReference,
    Closable,
    Correction,
    Corrections,
    DecoderSession,
    Invocation,
    PrepareCircuit,
    PreparedCircuit,
    Readouts,
    Request,
    Requests,
    Resources,
)
from .quantum_operations import (
    LogicalSlot,
    Operation,
    RestoreMeasured,
    decompose_rotations,
    local_indices,
)
from .selection import Selection, prepare_selection

ResultT = TypeVar("ResultT")


def _pauli_corrections(
    block: BlockReference, operator: DensePauli
) -> Corrections[None]:
    for target in operator.support:
        yield Correction((block,), Operation(operator[target].lower(), (target,)))


@dataclass(frozen=True)
class GadgetPlan:
    gadget: Gadget
    body: PreparedCircuit
    labels: tuple[str, ...]
    inputs: tuple[EncodingLayout, ...]
    outputs: tuple[EncodingLayout, ...]
    label_types: Mapping[str, str]


@dataclass(frozen=True)
class InvocationPlan:
    gadget_plan: GadgetPlan
    call: InstructionCall
    selection: Selection
    binding: BoundCall
    targets: Sequence[int | str]
    input_placement: dict[str, int]


class LayerPlan:
    def __init__(
        self, layer: Layer, *, prepare_circuit: PrepareCircuit = prepare_body
    ) -> None:
        self.instruction_set = layer.instruction_set
        self.capacities = {
            block.name: block.encodes for block in self.instruction_set.blocks
        }
        self.bindings = {
            name: InstructionBinding(instruction, self.capacities)
            for name, instruction in self.instruction_set.instructions.items()
        }
        self.codes = layer.codes
        self.gadgets: dict[str, GadgetPlan] = {}
        self.lower_capacities: dict[str, int] = {}
        self.requirements: dict[str, Counter[str]] = {}
        self.scratch: Counter[str] = Counter()
        for name, gadget in layer.gadgets.items():
            capacities = {
                block.name: block.encodes
                for block in gadget.circuit.instruction_set.blocks
            }
            if self.gadgets and capacities != self.lower_capacities:
                raise ValueError("All gadgets must target the same lower block types")
            self.lower_capacities = capacities
            inputs = layout_boundary(
                gadget.implements.inputs,
                gadget.inputs,
                self.codes,
                self.capacities,
                capacities,
            )
            outputs = layout_boundary(
                gadget.implements.outputs,
                gadget.outputs,
                self.codes,
                self.capacities,
                capacities,
            )
            body = prepare_circuit(gadget.circuit)
            labels = tuple(
                dict.fromkeys(
                    label
                    for group in (
                        *(encoding.support for encoding in gadget.inputs),
                        body.labels,
                        *(encoding.support for encoding in gadget.outputs),
                    )
                    for label in group
                )
            )
            label_types = dict(body.block_types)
            for layout in (*inputs, *outputs):
                self.requirements[layout.block_type] = (
                    self.requirements.get(layout.block_type, Counter())
                    | layout.resources
                )
                label_types.update(zip(layout.labels, layout.lower_types))
            if len(capacities) == 1:
                for label in labels:
                    label_types.setdefault(label, next(iter(capacities)))
            if set(labels) - label_types.keys():
                raise ValueError("Prepared circuit labels require lower block types")
            input_counts = sum((layout.resources for layout in inputs), Counter())
            output_counts = sum((layout.resources for layout in outputs), Counter())
            scratch = Counter(label_types[label] for label in labels) - (
                input_counts | output_counts
            )
            self.scratch |= scratch
            self.gadgets[name] = GadgetPlan(
                gadget, body, labels, inputs, outputs, label_types
            )

    @cached_property
    def instructions(self) -> InstructionSet:
        return InstructionSet(self.instruction_set)

    @cached_property
    def resolve(self) -> ResolveOperation:
        return prepare_resolver(self.instructions, decompose_rotations)


class LayerRuntime:
    def __init__(self, plan: LayerPlan, decoder: DecoderSession) -> None:
        self.plan = plan
        self.decoder = decoder
        self.layout = LayerLayout()
        # Slots that hold program data, for live blocks this layer prepared on
        # the program's behalf. A live block missing here counts as fully
        # occupied; its other slots are spare only while this set leaves them out.
        self._occupied: dict[int | str, set[int]] = {}
        self._next_invocation = 0
        self._failed = False

    def required_resources(self, upper: Resources) -> Resources:
        upper_blocks = Counter(upper.blocks)
        if upper.qubits:
            if len(self.plan.capacities) != 1:
                raise ValueError(
                    "Individual qubits require an unambiguous source block type"
                )
            upper_blocks[next(iter(self.plan.capacities))] += upper.qubits
        if upper_blocks.keys() - self.plan.requirements.keys():
            raise ValueError("Block resources require a declared encoding layout")
        required_lower_blocks = self.plan.scratch.copy()
        for block_type, count in upper_blocks.items():
            for lower_type, required in self.plan.requirements[block_type].items():
                required_lower_blocks[lower_type] += count * required
        if (
            len(self.plan.lower_capacities) == 1
            and next(iter(self.plan.lower_capacities.values())) == 1
        ):
            return Resources(qubits=sum(required_lower_blocks.values()))
        return Resources(blocks=required_lower_blocks)

    def start(self, resources: Resources) -> None:
        self.layout.start(resources, self.plan.lower_capacities)
        self._occupied.clear()
        self._next_invocation = 0
        self._failed = False

    def handle(self, request: Request) -> Requests[Readouts]:
        if isinstance(request, RestoreMeasured):
            slot = self._resolve_logical_slot(request.target)
            if slot.block not in self.layout.blocks:
                yield from self.prepare(slot)
                if request.value:
                    yield from self.apply("x", (slot,))
            return ()
        if isinstance(request, InstructionCall):
            readouts = yield from self.execute(
                request.mnemonic,
                request.operands,
                request.arguments,
                select=request.select,
            )
            for operand in request.operands:
                self._occupied.pop(operand, None)
            return readouts
        targets = tuple(
            self._resolve_logical_slot(target) for target in request.targets
        )
        if request.name == "prepare":
            yield from self.prepare(targets[0])
        elif request.name == "measure":
            readout = yield from self.measure(targets[0])
            return (readout,)
        elif request.name == "discard":
            yield from self.discard(targets[0].block)
        else:
            yield from self.apply(request.name, targets, angle=request.angle)
        return ()

    def _resolve_logical_slot(self, target: int | str | LogicalSlot) -> LogicalSlot:
        if isinstance(target, LogicalSlot):
            slot = target
        elif target in self.layout.blocks:
            slot = LogicalSlot(
                target, 0, self.layout.blocks[target].reference.block_type
            )
        elif len(self.plan.capacities) == 1:
            slot = LogicalSlot(target, 0, next(iter(self.plan.capacities)))
        else:
            raise ValueError("Logical slot requires an explicit block type")
        if (
            slot.block_type not in self.plan.capacities
            or not 0 <= slot.index < self.plan.capacities[slot.block_type]
        ):
            raise ValueError("Logical slot is outside its declared block capacity")
        block = self.layout.blocks.get(slot.block)
        if block is not None and block.reference.block_type != slot.block_type:
            raise ValueError("Logical slot does not match the live block type")
        return slot

    def _spare_slots(self, slot: LogicalSlot) -> tuple[LogicalSlot, ...]:
        others = set(range(self.plan.capacities[slot.block_type])) - {slot.index}
        if slot.block in self.layout.blocks:
            others -= self._occupied.get(slot.block, others)
        return tuple(
            LogicalSlot(slot.block, index, slot.block_type) for index in sorted(others)
        )

    def prepare(self, target: int | str | LogicalSlot) -> Requests[None]:
        slot = self._resolve_logical_slot(target)
        (call,) = self.plan.resolve(
            "prepare", (slot,), None, spare=self._spare_slots(slot)
        )
        yield from self.execute(
            call.mnemonic, call.operands, call.arguments, select=call.select
        )
        if not self.plan.bindings[call.mnemonic].input_types:
            self._occupied[slot.block] = {slot.index}
        elif slot.block in self._occupied:
            self._occupied[slot.block].add(slot.index)

    def apply(
        self,
        operation: str,
        targets: Sequence[int | str | LogicalSlot],
        angle: float | str | None = None,
    ) -> Requests[None]:
        slots = tuple(self._resolve_logical_slot(target) for target in targets)
        try:
            calls = self.plan.resolve(operation, slots, angle)
        except UnboundOperation:
            logical = self._logical_pauli(operation, slots, angle)
            if logical is None:
                raise
            yield from self._apply_decoder_corrections(_pauli_corrections(*logical))
            calls = ()
        for call in calls:
            if call.mnemonic not in self.plan.gadgets:
                raise NotImplementedError(f"No gadget implements {call.mnemonic!r}")
        for call in calls:
            yield from self.execute(
                call.mnemonic, call.operands, call.arguments, select=call.select
            )
        for slot in slots:
            if slot.block in self._occupied:
                self._occupied[slot.block].add(slot.index)

    def _logical_pauli(
        self,
        operation: str,
        slots: Sequence[LogicalSlot],
        angle: float | str | None,
    ) -> tuple[BlockReference, DensePauli] | None:
        """The code's logical operator for a Pauli the ISA does not declare.

        Logical Paulis are commonly left out of an ISA and tracked as frame
        updates. Without an instruction, the Pauli is applied as the layer code's
        logical operator on the block's qubits in the layer below, the same way
        decoder corrections are, so each lower layer resolves it in turn.
        """
        if operation not in ("x", "y", "z") or angle is not None or len(slots) != 1:
            return None
        (slot,) = slots
        code = self.plan.codes.get(slot.block_type)
        if code is None or not 0 <= slot.index < len(code.x):
            return None
        block = self.layout.blocks.get(slot.block)
        if block is None:
            raise ValueError(f"Input block {slot.block!r} has not been prepared")
        width = code.physical_qubit_count
        operator = pauli("", width)
        if operation in ("x", "y"):
            operator *= pauli(code.x[slot.index], width)
        if operation in ("z", "y"):
            operator *= pauli(code.z[slot.index], width)
        return block.reference, operator

    def measure(self, target: int | str | LogicalSlot) -> Requests[bool | None]:
        slot = self._resolve_logical_slot(target)
        (call,) = self.plan.resolve(
            "measure", (slot,), None, spare=self._spare_slots(slot)
        )
        readouts = yield from self.execute(
            call.mnemonic, call.operands, call.arguments, select=call.select
        )
        if slot.block not in self.layout.blocks:
            self._occupied.pop(slot.block, None)
        index = self.plan.instructions.outcome_index(call.mnemonic, slot.index)
        if index is None:
            if len(readouts) != 1:
                raise ValueError("A logical Z measurement must produce one readout")
            index = 0
        return readouts[index]

    def discard(self, target: int | str) -> Requests[None]:
        self._occupied.pop(target, None)
        block = self.layout.remove_block(target)
        if block is None:
            return
        for lower_block in block.support:
            yield from self._discard_lower_block(lower_block)
        self._notify_decoder_of_discarded_blocks((block.reference,))

    def _discard_lower_block(self, lower_block: int) -> Requests[None]:
        target = self.layout.address_for_discard(
            lower_block, self.plan.lower_capacities
        )
        yield Operation("discard", (target,))
        self.layout.finish_discard(lower_block)

    def _notify_decoder_of_discarded_blocks(
        self, blocks: tuple[BlockReference, ...]
    ) -> None:
        if blocks and isinstance(self.decoder, BlockObserver):
            self.decoder.discarded(blocks)

    def _apply_decoder_corrections(
        self, corrections: Corrections[ResultT]
    ) -> Requests[ResultT]:
        with closing(corrections):
            lower_readouts: Readouts | None = None
            while True:
                try:
                    correction = corrections.send(lower_readouts)
                except StopIteration as completed:
                    return cast(ResultT, completed.value)
                if len(set(correction.blocks)) != len(correction.blocks):
                    raise ValueError("Correction blocks must be distinct")
                correction_qubits = self.layout.qubits_for_correction(correction.blocks)
                operation = correction.operation
                target_indices = local_indices(operation)
                if any(
                    index < 0 or index >= len(correction_qubits)
                    for index in target_indices
                ):
                    raise ValueError("Correction targets an out-of-range code qubit")
                lower_readouts = yield Operation(
                    operation.name,
                    tuple(correction_qubits[index] for index in target_indices),
                    operation.angle,
                )
                if lower_readouts is None:
                    raise TypeError("Correction execution must return a readout tuple")

    def _build_invocation_plan(
        self,
        mnemonic: str,
        targets: Sequence[int | str],
        arguments: Mapping[str, InstructionCall.Argument],
        select: Sequence[Mapping[str, int]],
    ) -> InvocationPlan:
        try:
            gadget_plan = self.plan.gadgets[mnemonic]
        except KeyError as error:
            raise NotImplementedError(f"No gadget implements {mnemonic!r}") from error
        gadget = gadget_plan.gadget
        call = InstructionCall(
            mnemonic,
            operands=list(targets),
            arguments=dict(arguments),
            select=[dict(pattern) for pattern in select],
        )
        selection = prepare_selection(gadget.implements.flags, select)
        prepared = self.plan.bindings[mnemonic]
        binding = prepared.bind(
            targets,
            input_types={
                label: block.reference.block_type
                for label, block in self.layout.blocks.items()
            },
        )
        prepared.validate(arguments)
        if len(binding.inputs) != len(gadget.inputs) or len(binding.outputs) != len(
            gadget.outputs
        ):
            raise NotImplementedError(
                "Gadget boundaries must match the expanded call operands"
            )
        input_placement: dict[str, int] = {}
        for target, encoding in zip(targets, gadget.inputs):
            if target not in self.layout.blocks:
                raise ValueError(f"Input block {target!r} has not been prepared")
            block = self.layout.blocks[target]
            if len(encoding.support) != len(block.support):
                raise ValueError("Gadget input layout does not match the live block")
            for label, lower_block in zip(encoding.support, block.support):
                if label in input_placement and input_placement[label] != lower_block:
                    raise ValueError("Gadget input supports overlap")
                input_placement[label] = lower_block
        output_only_targets = targets[len(gadget.inputs) :]
        self.layout.ensure_gadget_fits(
            gadget_plan.labels,
            gadget_plan.label_types,
            input_placement,
            output_only_targets,
        )
        return InvocationPlan(
            gadget_plan,
            call,
            selection,
            binding,
            targets,
            input_placement,
        )

    def execute(
        self,
        mnemonic: str,
        targets: Sequence[int | str],
        arguments: Mapping[str, InstructionCall.Argument],
        *,
        select: Sequence[Mapping[str, int]] = (),
    ) -> Requests[Readouts]:
        if self._failed:
            raise RuntimeError("Layer has a failed invocation and cannot continue")
        invocation_count_before_call = self._next_invocation
        try:
            invocation_plan = self._build_invocation_plan(
                mnemonic, targets, arguments, select
            )
            invocation_id = self._next_invocation
            self._next_invocation += 1
            return (yield from self._run_invocation(invocation_plan, invocation_id))
        except BaseException:
            self._failed |= invocation_count_before_call != self._next_invocation
            raise

    def _run_invocation(
        self, invocation_plan: InvocationPlan, invocation_id: int
    ) -> Requests[Readouts]:
        gadget_plan = invocation_plan.gadget_plan
        gadget = gadget_plan.gadget
        for target in invocation_plan.targets[len(gadget.inputs) :]:
            yield from self.discard(target)
        input_references = tuple(
            self.layout.blocks[target].reference
            for target in invocation_plan.targets[: len(gadget.inputs)]
        )
        output_references = tuple(
            self.layout.reference_for_output(operand.label, operand.block_type)
            for operand in invocation_plan.binding.outputs
        )
        invocation = Invocation(
            invocation_id,
            gadget,
            invocation_plan.call,
            input_references,
            output_references,
        )
        if isinstance(self.decoder, BeforeInvocation):
            yield from self._apply_decoder_corrections(self.decoder.before(invocation))
        circuit_placement = self.layout.allocate_circuit_blocks(
            gadget_plan.labels,
            gadget_plan.label_types,
            invocation_plan.input_placement,
        )
        circuit_readouts = yield from self._run_gadget_circuit(
            gadget_plan, invocation, circuit_placement
        )
        retained_lower_blocks = self.layout.publish_outputs(
            invocation_plan.targets[: len(gadget.inputs)],
            output_references,
            gadget_plan.outputs,
            gadget_plan.labels,
            circuit_placement,
            gadget_plan.label_types,
            self.plan.lower_capacities,
        )
        decoded_readouts = yield from self._apply_decoder_corrections(
            self.decoder.decode(invocation, circuit_readouts)
        )
        if len(decoded_readouts.outcomes) != gadget.implements.observe_count or len(
            decoded_readouts.flags
        ) != len(gadget.implements.flags):
            raise ValueError(
                f"Decoder returned the wrong number of readouts for {invocation_plan.call.mnemonic!r}"
            )
        invocation_plan.selection.require(decoded_readouts.flags)
        for lower_block in sorted(
            set(circuit_placement.values()) - retained_lower_blocks
        ):
            yield from self._discard_lower_block(lower_block)
        self._notify_decoder_of_discarded_blocks(
            tuple(
                reference
                for reference in input_references
                if reference not in output_references
            )
        )
        return decoded_readouts.readouts

    def _run_gadget_circuit(
        self,
        gadget_plan: GadgetPlan,
        invocation: Invocation,
        circuit_placement: Mapping[str, int],
    ) -> Requests[Readouts]:
        circuit_runtime = gadget_plan.body.create_runtime()
        with ExitStack() as resources:
            if isinstance(circuit_runtime, Closable):
                resources.callback(circuit_runtime.close)
            required_resources = circuit_runtime.required_resources(invocation)
            if required_resources.qubits + sum(
                required_resources.blocks.values()
            ) > len(circuit_placement):
                raise NotImplementedError(
                    "Circuit resources exceed the prepared label layout"
                )
            if Counter(required_resources.blocks) - Counter(
                gadget_plan.label_types.values()
            ):
                raise NotImplementedError(
                    "Circuit resources exceed the prepared typed layout"
                )
            circuit_requests = resources.enter_context(
                closing(circuit_runtime.run(invocation))
            )
            lower_readouts: Readouts | None = None
            while True:
                try:
                    circuit_request = circuit_requests.send(lower_readouts)
                except StopIteration as completed:
                    if not isinstance(completed.value, tuple):
                        raise TypeError("Circuit execution must return a readout tuple")
                    return cast(Readouts, completed.value)
                if isinstance(circuit_request, InstructionCall):
                    lower_request = InstructionCall(
                        circuit_request.mnemonic,
                        operands=[
                            circuit_placement[str(operand)]
                            for operand in circuit_request.operands
                        ],
                        arguments=circuit_request.arguments,
                        select=circuit_request.select,
                    )
                elif isinstance(circuit_request, RestoreMeasured):
                    lower_request = RestoreMeasured(
                        self._map_circuit_target(
                            circuit_request.target,
                            gadget_plan,
                            circuit_placement,
                        ),
                        circuit_request.value,
                    )
                else:
                    lower_request = Operation(
                        circuit_request.name,
                        tuple(
                            self._map_circuit_target(
                                target, gadget_plan, circuit_placement
                            )
                            for target in circuit_request.targets
                        ),
                        circuit_request.angle,
                    )
                lower_readouts = yield lower_request
                if lower_readouts is None:
                    raise TypeError("Instruction execution must return a readout tuple")

    def _map_circuit_target(
        self,
        target: int | LogicalSlot,
        gadget_plan: GadgetPlan,
        circuit_placement: Mapping[str, int],
    ) -> int | LogicalSlot:
        if isinstance(target, LogicalSlot):
            if gadget_plan.label_types[str(target.block)] != target.block_type:
                raise ValueError("Circuit logical slot has the wrong block type")
            return LogicalSlot(
                circuit_placement[str(target.block)], target.index, target.block_type
            )
        return circuit_placement[str(target)]

    def close(self) -> None:
        self.layout.clear()
        self._occupied.clear()
        self.decoder.close()
