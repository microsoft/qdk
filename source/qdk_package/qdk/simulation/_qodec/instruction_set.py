from __future__ import annotations

from collections.abc import Collection, Mapping, Sequence
from dataclasses import dataclass
from math import isclose
from types import MappingProxyType
from typing import cast

from qodec import Instruction, InstructionSet as QodecInstructionSet
import qodec.actions as actions
from qodec.instructions import InstructionCall
from paulimer import CliffordUnitary

from .clifford_semantics import (
    clifford_operations as clifford_operations,
    clifford_tableau,
    compose_cliffords,
    lower_clifford,
    named_clifford,
    pauli as pauli,
)
from .quantum_operations import (
    FrameUpdate,
    LogicalSlot,
    Operation,
    RestoreMeasured,
    local_indices,
)
from .protocols import Readouts, Request, Requests, Resources
from .action_runtime import ActionProgram, prepare_actions, temporary_count
from .call_binding import BoundCall, InstructionBinding
from .physical_layout import PhysicalLayout
from .quantum_lowering import lower_program
from .selection import prepare_selection


class UnboundOperation(NotImplementedError):
    pass


def action_semantics(
    instruction: Instruction, width: int | None = None
) -> tuple[Operation | CliffordUnitary, ...]:
    if width is None:
        width = max(len(instruction.inputs), len(instruction.outputs))
    if any(parameter.kind.value == "pauli" for parameter in instruction.parameters):
        raise NotImplementedError(
            "Parameterized Pauli actions require call-time interpretation"
        )
    operations: list[Operation | CliffordUnitary] = []
    for action in instruction.action:
        if getattr(action, "condition", None) is not None:
            raise NotImplementedError(
                "Conditional ISA actions need a classical action runtime"
            )
        if isinstance(action, actions.Clifford):
            operations.append(clifford_tableau(action.generators, width))
        elif isinstance(action, actions.Pauli):
            operator = pauli(action.operator)
            operations.extend(
                Operation(operator[index].lower(), (index,))
                for index in operator.support
            )
        elif isinstance(action, (actions.Stabilize, actions.Observe)):
            preparing = isinstance(action, actions.Stabilize)
            operators = (
                action.operators
                if isinstance(action, actions.Stabilize)
                else action.observables
            )
            for expression in operators:
                operator = pauli(expression)
                targets = tuple(operator.support)
                if isinstance(action, actions.Stabilize) and any(
                    target >= width for target in targets
                ):
                    raise NotImplementedError(
                        "Semantic temporaries require the action runtime"
                    )
                if (
                    len(targets) != 1
                    or operator[targets[0]] != "Z"
                    or operator.phase != 1
                ):
                    raise NotImplementedError(
                        "Preparation and observation require a single positive Z operator"
                    )
                operations.append(
                    Operation("prepare" if preparing else "measure", targets)
                )
        elif isinstance(action, actions.Rotate):
            operator = pauli(action.pauli)
            targets = tuple(operator.support)
            bases = {operator[index] for index in targets}
            if len(targets) not in (1, 2) or len(bases) != 1 or operator.phase != 1:
                raise NotImplementedError(
                    "Rotations require one or two equal positive Pauli axes"
                )
            axis = operator[targets[0]].lower()
            operations.append(
                Operation("r" + axis * len(targets), targets, action.angle)
            )
        else:
            raise NotImplementedError(
                f"Unsupported ISA action: {type(action).__name__}"
            )
    return tuple(operations)


def action_operations(instruction: Instruction) -> tuple[Operation, ...]:
    return tuple(
        operation
        for step in action_semantics(instruction)
        for operation in (
            lower_clifford(step) if isinstance(step, CliffordUnitary) else (step,)
        )
    )


class InstructionSet:
    def __init__(self, instruction_set: QodecInstructionSet) -> None:
        self.instruction_set = instruction_set
        blocks = instruction_set.blocks
        self.name = instruction_set.name
        self.capacities = {block.name: block.encodes for block in blocks}
        self.declarations = dict(instruction_set.instructions)
        self.bindings = {
            name: InstructionBinding(instruction, self.capacities)
            for name, instruction in self.declarations.items()
        }
        self.semantics: dict[str, tuple[Operation | CliffordUnitary, ...]] = {}
        self.cliffords: dict[str, CliffordUnitary | None] = {}
        for name, instruction in self.declarations.items():
            if any(
                operand.is_variadic
                for operand in (*instruction.inputs, *instruction.outputs)
            ):
                continue
            width = max(
                sum(self.capacities[operand.block] for operand in instruction.inputs),
                sum(self.capacities[operand.block] for operand in instruction.outputs),
            )
            try:
                steps = action_semantics(instruction, width)
            except NotImplementedError:
                continue
            self.semantics[name] = steps
            self.cliffords[name] = compose_cliffords(steps, width)

    @property
    def block_type(self) -> str:
        if len(self.capacities) != 1:
            raise ValueError("An operation requires explicit block types for this ISA")
        return next(iter(self.capacities))

    def bind(
        self, operation: str, arity: int, angle: float | str | None = None
    ) -> tuple[str, dict[str, InstructionCall.Argument]]:
        blocks = (self.block_type,) * arity
        return self._bind(operation, tuple(range(arity)), blocks, angle)

    def bind_slots(
        self,
        operation: str,
        slots: Sequence[LogicalSlot],
        angle: float | str | None = None,
        *,
        spare: Collection[LogicalSlot] = (),
    ) -> tuple[str, tuple[int | str, ...], dict[str, InstructionCall.Argument]]:
        """Bind ``operation`` on ``slots`` to one instruction.

        ``spare`` lists other slots of the same blocks that hold no data. A
        preparation or measurement may also reset or measure those slots,
        which lets a block that encodes several logical qubits serve a single
        one. Instructions that leave spare slots untouched are preferred.
        """
        labels = tuple(dict.fromkeys(slot.block for slot in slots))
        block_types = {}
        for slot in slots:
            if (
                slot.block_type not in self.capacities
                or not 0 <= slot.index < self.capacities[slot.block_type]
            ):
                raise ValueError("Logical slot is outside its declared block capacity")
            if block_types.setdefault(slot.block, slot.block_type) != slot.block_type:
                raise ValueError("Logical slots disagree about a block's type")
        offsets = {}
        width = 0
        for label in labels:
            offsets[label] = width
            width += self.capacities[block_types[label]]
        positions = tuple(offsets[slot.block] + slot.index for slot in slots)
        if len(set(positions)) != len(positions):
            raise ValueError("Operation targets must be distinct logical slots")
        spare_positions = frozenset(
            offsets[slot.block] + slot.index
            for slot in spare
            if block_types.get(slot.block) == slot.block_type
            and 0 <= slot.index < self.capacities[slot.block_type]
        ) - set(positions)
        mnemonic, arguments = self._bind(
            operation,
            positions,
            tuple(block_types[label] for label in labels),
            angle,
            spare_positions,
        )
        return mnemonic, labels, arguments

    def outcome_index(self, mnemonic: str, position: int) -> int | None:
        """The outcome of a bound measurement that reports ``position``."""
        measured = [
            step.targets
            for step in self.semantics.get(mnemonic, ())
            if isinstance(step, Operation) and step.name == "measure"
        ]
        return measured.index((position,)) if (position,) in measured else None

    def _bind(
        self,
        operation: str,
        positions: tuple[int, ...],
        blocks: tuple[str, ...],
        angle: float | str | None,
        spare: frozenset[int] = frozenset(),
    ) -> tuple[str, dict[str, InstructionCall.Argument]]:
        width = sum(self.capacities[block_type] for block_type in blocks)
        gate = named_clifford(operation, len(positions), angle)
        requested_clifford = None
        if gate is not None:
            requested_clifford = CliffordUnitary.identity(width)
            requested_clifford.left_mul_clifford(gate, positions)
        requested = set(positions)
        # Each match is ranked (flagged, touches spare slots, parameterized):
        # unflagged instructions never reject a shot, instructions confined to
        # the requested slots leave spare ones alone, and exact angles beat
        # parameterized ones.
        matches: list[
            tuple[tuple[bool, bool, bool], str, dict[str, InstructionCall.Argument]]
        ] = []
        for name, operations in self.semantics.items():
            binding = self.bindings[name]
            inputs = binding.input_types
            outputs = binding.output_types
            if operation == "prepare":
                compatible = outputs == blocks and inputs in ((), blocks)
            elif operation == "measure":
                compatible = inputs == blocks and outputs in ((), blocks)
            else:
                compatible = inputs == outputs == blocks
            if not compatible:
                continue
            # Creating or consuming a block affects every slot it encodes.
            touched = set(range(width)) - requested if inputs != outputs else set()
            if touched - spare:
                continue
            flagged = bool(binding.flags)
            if requested_clifford is not None:
                if self.cliffords[name] == requested_clifford:
                    matches.append(((flagged, False, False), name, {}))
                continue
            if operation == "mov" and not operations:
                matches.append(((flagged, False, False), name, {}))
            if operation in ("prepare", "measure"):
                steps = [step for step in operations if isinstance(step, Operation)]
                if (
                    angle is not None
                    or not steps
                    or len(steps) != len(operations)
                    or any(
                        step.name != operation
                        or len(step.targets) != 1
                        or step.angle is not None
                        for step in steps
                    )
                ):
                    continue
                targets = [cast(int, step.targets[0]) for step in steps]
                if len(set(targets)) != len(targets) or not requested <= set(targets):
                    continue
                touched |= set(targets) - requested
                if touched - spare:
                    continue
                matches.append(((flagged, bool(touched), False), name, {}))
                continue
            if len(operations) != 1:
                continue
            action = operations[0]
            if isinstance(action, CliffordUnitary):
                continue
            if action.name != operation or action.targets != positions:
                continue
            if isinstance(action.angle, str) and angle is not None:
                matches.append(((flagged, False, True), name, {action.angle: angle}))
            elif action.angle is None and angle is None:
                matches.append(((flagged, False, False), name, {}))
            elif (
                isinstance(action.angle, (int, float))
                and isinstance(angle, (int, float))
                and isclose(action.angle, angle)
            ):
                matches.append(((flagged, False, False), name, {}))
        if not matches:
            raise UnboundOperation(
                f"ISA {self.name!r} does not implement {operation!r}"
            )
        best = min(rank for rank, _, _ in matches)
        chosen = [
            (name, arguments) for rank, name, arguments in matches if rank == best
        ]
        if len(chosen) > 1:
            raise ValueError(f"Ambiguous {operation!r} binding in ISA {self.name!r}")
        return chosen[0]


@dataclass(frozen=True)
class PreparedAction:
    instruction: Instruction
    program: ActionProgram | None
    scratch: int

    def bind(
        self, call: BoundCall, arguments: Mapping[str, InstructionCall.Argument]
    ) -> ActionProgram:
        return (
            self.program
            if self.program is not None
            else prepare_actions(
                self.instruction, call.input_capacity, call.output_capacity, arguments
            )
        )


def prepare_operations(
    instructions: InstructionSet,
) -> Mapping[str, tuple[Operation, ...] | PreparedAction]:
    prepared = {}
    bare = (
        len(instructions.capacities) == 1
        and next(iter(instructions.capacities.values())) == 1
    )
    for name, instruction in instructions.declarations.items():
        variadic = any(
            operand.is_variadic
            for operand in (*instruction.inputs, *instruction.outputs)
        )
        if bare and not variadic:
            try:
                prepared[name] = action_operations(instruction)
                continue
            except NotImplementedError:
                pass
        program = None
        inputs = sum(
            instructions.capacities[operand.block]
            for operand in instruction.inputs
            if not operand.is_variadic
        )
        outputs = sum(
            instructions.capacities[operand.block]
            for operand in instruction.outputs
            if not operand.is_variadic
        )
        if not instruction.parameters and not variadic:
            program = prepare_actions(instruction, inputs, outputs, {})
        prepared[name] = PreparedAction(
            instruction, program, temporary_count(instruction, max(inputs, outputs))
        )
    return MappingProxyType(prepared)


class InstructionRuntime:
    def __init__(
        self,
        instructions: InstructionSet,
        *,
        operations: Mapping[str, tuple[Operation, ...] | PreparedAction] | None = None,
    ) -> None:
        self.instructions = instructions
        self.operations = (
            prepare_operations(instructions) if operations is None else operations
        )
        self.layout: PhysicalLayout | None = None
        self.failed = False
        self.scratch = max(
            (
                operation.scratch
                for operation in self.operations.values()
                if isinstance(operation, PreparedAction)
            ),
            default=0,
        )

    def required_resources(self, upper: Resources) -> Resources:
        if upper.blocks.keys() - self.instructions.capacities.keys():
            raise NotImplementedError(
                "Physical block resources must match the instruction set"
            )
        return Resources(
            qubits=upper.qubits
            + sum(
                count * self.instructions.capacities[block_type]
                for block_type, count in upper.blocks.items()
            )
            + self.scratch
        )

    def start(self, resources: Resources) -> None:
        if resources.blocks:
            raise ValueError("Physical action execution requires qubit resources")
        self.layout = PhysicalLayout(resources.qubits)
        self.failed = False

    def close(self) -> None:
        self.layout = None

    def handle(self, request: Request) -> Requests[Readouts]:
        if self.failed:
            raise RuntimeError("Physical instruction runtime has failed")
        try:
            return (yield from self._handle(request))
        except BaseException:
            self.failed = True
            raise

    def _handle(self, request: Request) -> Requests[Readouts]:
        if isinstance(request, RestoreMeasured):
            if self.layout is None:
                raise RuntimeError(
                    "Measured-state restoration requires physical startup resources"
                )
            label = (
                request.target.block
                if isinstance(request.target, LogicalSlot)
                else request.target
            )
            if label not in self.layout.blocks:
                yield from self.handle(Operation("prepare", (request.target,)))
                if request.value:
                    yield from self.handle(Operation("x", (request.target,)))
            return ()
        if isinstance(request, FrameUpdate):
            if self.layout is None:
                readouts = yield request
            else:
                slot = (
                    request.target
                    if isinstance(request.target, LogicalSlot)
                    else LogicalSlot(request.target, 0, self.instructions.block_type)
                )
                block = self.layout.blocks.get(slot.block)
                if block is None or block.block_type != slot.block_type:
                    raise ValueError(
                        f"Frame update targets physical block {slot.block!r}, "
                        "which is not live"
                    )
                if not 0 <= slot.index < len(block.qubits):
                    raise ValueError("Physical logical slot is out of range")
                readouts = yield FrameUpdate(request.pauli, block.qubits[slot.index])
            if readouts is None:
                raise TypeError("Frame updates must return a readout tuple")
            return ()
        if isinstance(request, Operation):
            if self.layout is None:
                readouts = yield request
                if readouts is None:
                    raise TypeError("Instruction execution must return a readout tuple")
                return readouts
            targets = []
            for target in request.targets:
                slot = (
                    target
                    if isinstance(target, LogicalSlot)
                    else LogicalSlot(target, 0, self.instructions.block_type)
                )
                if request.name == "discard":
                    for qubit in self.layout.release(slot.block):
                        yield Operation("discard", (qubit,))
                    continue
                if not 0 <= slot.index < self.instructions.capacities[slot.block_type]:
                    raise ValueError("Physical logical slot is out of range")
                block = self.layout.allocate(
                    slot.block,
                    slot.block_type,
                    self.instructions.capacities[slot.block_type],
                    preparing=request.name == "prepare",
                )
                targets.append(block.qubits[slot.index])
            if request.name == "discard":
                return ()
            readouts = yield Operation(request.name, tuple(targets), request.angle)
            if readouts is None:
                raise TypeError("Instruction execution must return a readout tuple")
            return readouts
        prepared = self.instructions.bindings[request.mnemonic]
        selection = prepare_selection(prepared.flags, request.select)
        readouts = yield from self.execute(
            request.mnemonic, request.operands, request.arguments
        )
        selection.require(readouts[prepared.observe_count :])
        return readouts

    def execute(
        self,
        mnemonic: str,
        targets: Sequence[int | str],
        arguments: Mapping[str, InstructionCall.Argument],
    ) -> Requests[Readouts]:
        if self.failed:
            raise RuntimeError("Physical instruction runtime has failed")
        prepared = self.instructions.bindings[mnemonic]
        binding = prepared.bind(targets)
        prepared.validate(arguments)
        operations = self.operations[mnemonic]
        program = (
            operations.bind(binding, arguments)
            if isinstance(operations, PreparedAction)
            else None
        )
        width = (
            program.num_qubits
            if program is not None
            else max(binding.input_capacity, binding.output_capacity)
        )
        discarded = ()
        if self.layout is None:
            if program is not None:
                raise RuntimeError("General physical actions require startup resources")
            if not all(isinstance(target, int) for target in targets):
                raise ValueError(
                    "Physical instructions require integer targets before startup"
                )
            qubits = cast(tuple[int, ...], tuple(targets))
        else:
            qubits, discarded = self.layout.begin(binding, width)
        try:
            for qubit in discarded:
                yield Operation("discard", (qubit,))
            if isinstance(operations, tuple):
                records: list[bool | None] = []
                for operation in operations:
                    operands = tuple(
                        qubits[index] for index in local_indices(operation)
                    )
                    angle = (
                        arguments[operation.angle]
                        if isinstance(operation.angle, str)
                        else operation.angle
                    )
                    if angle is not None and not isinstance(angle, (int, float)):
                        raise TypeError("Physical rotation angles must be numeric")
                    readouts = yield Operation(operation.name, operands, angle)
                    if readouts is None:
                        raise TypeError(
                            "Instruction execution must return a readout tuple"
                        )
                    records.extend(readouts)
                if self.layout is not None:
                    for qubit in qubits[binding.output_capacity :]:
                        yield Operation("discard", (qubit,))
            else:
                if program is None:
                    raise RuntimeError("Physical action program was not prepared")
                records = list((yield from lower_program(program, qubits)))
            if len(records) != prepared.observe_count:
                raise ValueError(
                    f"Instruction {mnemonic!r} returned the wrong number of observable results"
                )
            if self.layout is not None:
                self.layout.commit(binding, qubits)
            return tuple(records) + (False,) * len(prepared.flags)
        except BaseException:
            self.failed = True
            raise
