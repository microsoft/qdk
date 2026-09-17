from __future__ import annotations

from collections import Counter
from collections.abc import Mapping, Sequence
from contextlib import ExitStack, closing
from dataclasses import dataclass
from functools import cached_property
from heapq import heappop, heappush
from typing import TypeVar, cast

from qodec import Gadget, Layer
from qodec.instructions import InstructionCall

from .call_binding import bind_call
from .circuit_runtime import prepare_circuit as prepare_body
from .encoding_layout import EncodingLayout, layout_boundary
from .instruction_set import InstructionSet
from .operation_resolution import ResolveOperation, prepare_resolver
from .protocols import (
    BeforeInvocation,
    BlockObserver,
    BlockReference,
    Closable,
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
from .selection import prepare_selection

ResultT = TypeVar("ResultT")


@dataclass(frozen=True)
class GadgetPlan:
    gadget: Gadget
    body: PreparedCircuit
    labels: tuple[str, ...]
    inputs: tuple[EncodingLayout, ...]
    outputs: tuple[EncodingLayout, ...]
    label_types: Mapping[str, str]


@dataclass(frozen=True)
class LiveBlock:
    reference: BlockReference
    support: tuple[int, ...]
    qubits: tuple[int | LogicalSlot, ...]


class LayerPlan:
    def __init__(
        self, layer: Layer, *, prepare_circuit: PrepareCircuit = prepare_body
    ) -> None:
        self.instruction_set = layer.instruction_set
        self.capacities = {
            block.name: block.encodes for block in self.instruction_set.blocks
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
    def resolve(self) -> ResolveOperation:
        return prepare_resolver(
            InstructionSet(self.instruction_set), decompose_rotations
        )


class LayerRuntime:
    def __init__(self, plan: LayerPlan, decoder: DecoderSession) -> None:
        self.plan = plan
        self.decoder = decoder
        self.blocks: dict[int | str, LiveBlock] = {}
        self.free: list[int] = []
        self._generations: dict[int | str, int] = {}
        self._next_invocation = 0
        self._limits: Counter[str] = Counter()
        self._allocated: dict[int, str] = {}
        self._failed = False

    def required_resources(self, upper: Resources) -> Resources:
        blocks = Counter(upper.blocks)
        if upper.qubits:
            if len(self.plan.capacities) != 1:
                raise ValueError(
                    "Individual qubits require an unambiguous source block type"
                )
            blocks[next(iter(self.plan.capacities))] += upper.qubits
        if blocks.keys() - self.plan.requirements.keys():
            raise ValueError("Block resources require a declared encoding layout")
        lower = self.plan.scratch.copy()
        for block_type, count in blocks.items():
            for lower_type, required in self.plan.requirements[block_type].items():
                lower[lower_type] += count * required
        if (
            len(self.plan.lower_capacities) == 1
            and next(iter(self.plan.lower_capacities.values())) == 1
        ):
            return Resources(qubits=sum(lower.values()))
        return Resources(blocks=lower)

    def start(self, resources: Resources) -> None:
        if resources.blocks.keys() - self.plan.lower_capacities.keys():
            raise ValueError("Resources name undeclared lower block types")
        self.blocks.clear()
        self._limits = Counter(resources.blocks)
        if resources.qubits:
            if (
                len(self.plan.lower_capacities) != 1
                or next(iter(self.plan.lower_capacities.values())) != 1
            ):
                raise ValueError("Mixed lower blocks require typed resource capacities")
            self._limits[next(iter(self.plan.lower_capacities))] += resources.qubits
        self.free = list(range(resources.qubits + sum(resources.blocks.values())))
        self._allocated.clear()
        self._generations.clear()
        self._next_invocation = 0
        self._failed = False

    def handle(self, request: Request) -> Requests[Readouts]:
        if isinstance(request, RestoreMeasured):
            slot = self._slot(request.target)
            if slot.block not in self.blocks:
                yield from self.prepare(slot)
                if request.value:
                    yield from self.apply("x", (slot,))
            return ()
        if isinstance(request, InstructionCall):
            return (
                yield from self.execute(
                    request.mnemonic,
                    request.operands,
                    request.arguments,
                    select=request.select,
                )
            )
        targets = tuple(self._slot(target) for target in request.targets)
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

    def _slot(self, target: int | str | LogicalSlot) -> LogicalSlot:
        if isinstance(target, LogicalSlot):
            slot = target
        elif target in self.blocks:
            slot = LogicalSlot(target, 0, self.blocks[target].reference.block_type)
        elif len(self.plan.capacities) == 1:
            slot = LogicalSlot(target, 0, next(iter(self.plan.capacities)))
        else:
            raise ValueError("Logical slot requires an explicit block type")
        if (
            slot.block_type not in self.plan.capacities
            or not 0 <= slot.index < self.plan.capacities[slot.block_type]
        ):
            raise ValueError("Logical slot is outside its declared block capacity")
        block = self.blocks.get(slot.block)
        if block is not None and block.reference.block_type != slot.block_type:
            raise ValueError("Logical slot does not match the live block type")
        return slot

    def prepare(self, target: int | str | LogicalSlot) -> Requests[None]:
        (call,) = self.plan.resolve("prepare", (self._slot(target),), None)
        yield from self.execute(call.mnemonic, call.operands, call.arguments)

    def apply(
        self,
        operation: str,
        targets: Sequence[int | str | LogicalSlot],
        angle: float | str | None = None,
    ) -> Requests[None]:
        slots = tuple(self._slot(target) for target in targets)
        calls = self.plan.resolve(operation, slots, angle)
        for call in calls:
            if call.mnemonic not in self.plan.gadgets:
                raise NotImplementedError(f"No gadget implements {call.mnemonic!r}")
        for call in calls:
            yield from self.execute(call.mnemonic, call.operands, call.arguments)

    def measure(self, target: int | str | LogicalSlot) -> Requests[bool | None]:
        (call,) = self.plan.resolve("measure", (self._slot(target),), None)
        readouts = yield from self.execute(call.mnemonic, call.operands, call.arguments)
        if len(readouts) != 1:
            raise ValueError("A logical Z measurement must produce one readout")
        return readouts[0]

    def discard(self, target: int | str) -> Requests[None]:
        block = self.blocks.pop(target, None)
        if block is None:
            return
        for child in block.support:
            yield from self._release(child)
        self._discarded((block.reference,))

    def _release(self, child: int) -> Requests[None]:
        block_type = self._allocated[child]
        target = (
            child
            if len(self.plan.lower_capacities) == 1
            and self.plan.lower_capacities[block_type] == 1
            else LogicalSlot(child, 0, block_type)
        )
        yield Operation("discard", (target,))
        self._allocated.pop(child)
        heappush(self.free, child)

    def _discarded(self, blocks: tuple[BlockReference, ...]) -> None:
        if blocks and isinstance(self.decoder, BlockObserver):
            self.decoder.discarded(blocks)

    def _new_reference(self, target: int | str, block_type: str) -> BlockReference:
        generation = self._generations.get(target, 0) + 1
        self._generations[target] = generation
        return BlockReference(target, generation, block_type)

    def _correct(self, corrections: Corrections[ResultT]) -> Requests[ResultT]:
        with closing(corrections):
            reply: Readouts | None = None
            while True:
                try:
                    correction = corrections.send(reply)
                except StopIteration as completed:
                    return cast(ResultT, completed.value)
                if len(set(correction.blocks)) != len(correction.blocks):
                    raise ValueError("Correction blocks must be distinct")
                support = []
                for reference in correction.blocks:
                    block = self.blocks.get(reference.label)
                    if block is None or block.reference != reference:
                        raise ValueError(
                            "Correction targets a block that is no longer live"
                        )
                    support.extend(block.qubits)
                operation = correction.operation
                indices = local_indices(operation)
                if any(index < 0 or index >= len(support) for index in indices):
                    raise ValueError("Correction targets an out-of-range code qubit")
                reply = yield Operation(
                    operation.name,
                    tuple(support[index] for index in indices),
                    operation.angle,
                )
                if reply is None:
                    raise TypeError("Correction execution must return a readout tuple")

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
        previous = self._next_invocation
        try:
            return (yield from self._execute(mnemonic, targets, arguments, select))
        except BaseException:
            self._failed |= previous != self._next_invocation
            raise

    def _execute(
        self,
        mnemonic: str,
        targets: Sequence[int | str],
        arguments: Mapping[str, InstructionCall.Argument],
        select: Sequence[Mapping[str, int]],
    ) -> Requests[Readouts]:
        try:
            prepared = self.plan.gadgets[mnemonic]
        except KeyError as error:
            raise NotImplementedError(f"No gadget implements {mnemonic!r}") from error
        gadget = prepared.gadget
        call = InstructionCall(
            mnemonic,
            operands=list(targets),
            arguments=dict(arguments),
            select=[dict(pattern) for pattern in select],
        )
        selection = prepare_selection(gadget.implements.flags, select)
        binding = bind_call(
            self.plan.instruction_set,
            call,
            input_types={
                label: block.reference.block_type
                for label, block in self.blocks.items()
            },
        )
        if len(binding.inputs) != len(gadget.inputs) or len(binding.outputs) != len(
            gadget.outputs
        ):
            raise NotImplementedError(
                "Gadget boundaries must match the expanded call operands"
            )
        labels: dict[str, int] = {}
        for target, encoding in zip(targets, gadget.inputs):
            if target not in self.blocks:
                raise ValueError(f"Input block {target!r} has not been prepared")
            block = self.blocks[target]
            if len(encoding.support) != len(block.support):
                raise ValueError("Gadget input layout does not match the live block")
            for label, child in zip(encoding.support, block.support):
                if label in labels and labels[label] != child:
                    raise ValueError("Gadget input supports overlap")
                labels[label] = child
        replaced = tuple(
            self.blocks[target]
            for target in targets[len(gadget.inputs) :]
            if target in self.blocks
        )
        available = len(self.free) + sum(len(block.support) for block in replaced)
        if sum(label not in labels for label in prepared.labels) > available:
            raise ValueError("Gadget exceeds the reserved lower-block capacity")
        prospective = Counter(self._allocated.values())
        for block in replaced:
            prospective.subtract(self._allocated[child] for child in block.support)
        for label in prepared.labels:
            if label in labels:
                prospective[self._allocated[labels[label]]] -= 1
            prospective[prepared.label_types[label]] += 1
        if prospective - self._limits:
            raise ValueError("Gadget exceeds the reserved lower-block type capacity")
        invocation_id = self._next_invocation
        self._next_invocation += 1
        for target in targets[len(gadget.inputs) :]:
            yield from self.discard(target)
        inputs = tuple(
            self.blocks[target].reference for target in targets[: len(gadget.inputs)]
        )
        outputs = tuple(
            (
                self.blocks[operand.label].reference
                if operand.label in self.blocks
                and self.blocks[operand.label].reference.block_type
                == operand.block_type
                else self._new_reference(operand.label, operand.block_type)
            )
            for operand in binding.outputs
        )
        invocation = Invocation(
            invocation_id,
            gadget,
            call,
            inputs,
            outputs,
        )
        if isinstance(self.decoder, BeforeInvocation):
            yield from self._correct(self.decoder.before(invocation))
        for label in prepared.labels:
            if label not in labels:
                labels[label] = heappop(self.free)
                self._allocated[labels[label]] = prepared.label_types[label]
        readouts = yield from self._run_body(prepared, invocation, labels)
        self._allocated.update(
            (labels[label], prepared.label_types[label]) for label in prepared.labels
        )
        for target in targets[: len(gadget.inputs)]:
            self.blocks.pop(target)
        retained: set[int] = set()
        for reference, layout in zip(outputs, prepared.outputs):
            support = tuple(labels[label] for label in layout.labels)
            self.blocks[reference.label] = LiveBlock(
                reference, support, layout.qubits(labels, self.plan.lower_capacities)
            )
            retained.update(support)
        decoded = yield from self._correct(self.decoder.decode(invocation, readouts))
        if len(decoded.outcomes) != gadget.implements.observe_count or len(
            decoded.flags
        ) != len(gadget.implements.flags):
            raise ValueError(
                f"Decoder returned the wrong number of readouts for {mnemonic!r}"
            )
        selection.require(decoded.flags)
        for child in sorted(set(labels.values()) - retained):
            yield from self._release(child)
        self._discarded(
            tuple(reference for reference in inputs if reference not in outputs)
        )
        return decoded.readouts

    def _run_body(
        self,
        prepared: GadgetPlan,
        invocation: Invocation,
        labels: Mapping[str, int],
    ) -> Requests[Readouts]:
        runtime = prepared.body.create_runtime()
        with ExitStack() as resources:
            if isinstance(runtime, Closable):
                resources.callback(runtime.close)
            required = runtime.required_resources(invocation)
            if required.qubits + sum(required.blocks.values()) > len(labels):
                raise NotImplementedError(
                    "Circuit resources exceed the prepared label layout"
                )
            if Counter(required.blocks) - Counter(prepared.label_types.values()):
                raise NotImplementedError(
                    "Circuit resources exceed the prepared typed layout"
                )
            requests = resources.enter_context(closing(runtime.run(invocation)))
            reply: Readouts | None = None
            while True:
                try:
                    request = requests.send(reply)
                except StopIteration as completed:
                    if not isinstance(completed.value, tuple):
                        raise TypeError("Circuit execution must return a readout tuple")
                    return cast(Readouts, completed.value)
                if isinstance(request, InstructionCall):
                    placed = InstructionCall(
                        request.mnemonic,
                        operands=[labels[str(operand)] for operand in request.operands],
                        arguments=request.arguments,
                        select=request.select,
                    )
                elif isinstance(request, RestoreMeasured):
                    placed = RestoreMeasured(
                        self._place_target(request.target, prepared, labels),
                        request.value,
                    )
                else:
                    placed = Operation(
                        request.name,
                        tuple(
                            self._place_target(target, prepared, labels)
                            for target in request.targets
                        ),
                        request.angle,
                    )
                reply = yield placed
                if reply is None:
                    raise TypeError("Instruction execution must return a readout tuple")

    def _place_target(
        self, target: int | LogicalSlot, prepared: GadgetPlan, labels: Mapping[str, int]
    ) -> int | LogicalSlot:
        if isinstance(target, LogicalSlot):
            if prepared.label_types[str(target.block)] != target.block_type:
                raise ValueError("Circuit logical slot has the wrong block type")
            return LogicalSlot(
                labels[str(target.block)], target.index, target.block_type
            )
        return labels[str(target)]

    def close(self) -> None:
        self.blocks.clear()
        self.free.clear()
        self._generations.clear()
        self._allocated.clear()
        self._limits.clear()
        self.decoder.close()
