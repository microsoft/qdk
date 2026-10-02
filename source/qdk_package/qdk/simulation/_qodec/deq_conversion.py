"""Compile reusable deq models from Qodec gadgets, without a program trace.

Encoding signs describe frame corrections, not logical measurement values.
Qodec signs name observables; deq logical targets name correction Paulis.
Flags use only local physical measurements and never receive frame corrections.
"""

from __future__ import annotations

from collections.abc import Collection, Hashable, Iterable, Mapping, Sequence
from contextlib import closing
from dataclasses import dataclass
from itertools import product
from math import expm1, log1p
from typing import cast

from qodec import Code, Gadget, Layer
from qodec.gadgets import Encoding
from qodec.instructions import InstructionCall
from deq.circuit import model as circuit  # pyright: ignore[reportMissingImports]
from deq.circuit.parser import parse  # pyright: ignore[reportMissingImports]
from deq.transpiler.jit_library_builder import (  # pyright: ignore[reportMissingImports]
    JitLibraryArtifacts,
    build_jit_library_artifacts,
)

from .. import NoiseConfig
from ..._native import QirInstructionId
from ...ec._analysis.channel_action import ChannelAction, _action_of, declared_action_of
from ...ec._analysis.propagation.frames import FrameGroup, PauliFrame
from ...ec._analysis.propagation.pauli import Pauli, complex_conjugate_of, relabel
from ...ec._layout import ProgramLayout
from .call_binding import validate_arguments
from .circuit_runtime import CallListRuntime
from .clifford_semantics import pauli
from .encoding_layout import EncodingLayout
from .instruction_set import InstructionRuntime, InstructionSet, PreparedAction
from .layer_runtime import GadgetPlan, LayerPlan
from .native_batch import _GATES, _RecordingBackend, _TABLE_WIDTHS
from .protocols import ExecutionUnresolved, Readouts, Requests, Resources
from .quantum_operations import Operation
from .readout_equations import (
    Parity,
    _local_parities,
    expression,
    prepare_frames,
)


@dataclass(frozen=True)
class _Parity:
    mask: int = 0
    constant: bool = False

    def __xor__(self, other: _Parity) -> _Parity:
        return _Parity(self.mask ^ other.mask, self.constant ^ other.constant)

    @property
    def indices(self) -> list[int]:
        return [
            index for index in range(self.mask.bit_length()) if self.mask >> index & 1
        ]


@dataclass(frozen=True)
class _GadgetModel:
    """Local layouts; readout_count excludes flags."""

    gtype: int
    inputs: tuple[str, ...]
    outputs: tuple[str, ...]
    measurement_count: int
    readout_count: int
    flags: tuple[_Parity, ...]


class _LibraryBuilder:
    def __init__(self, layer: Layer | LayerPlan, noise: NoiseConfig | None) -> None:
        self.plan = layer if isinstance(layer, LayerPlan) else LayerPlan(layer)
        self.noise = noise
        self.codes: dict[str, circuit.CodeDefinition] = {}
        self.gadgets: list[circuit.GadgetDefinition] = []
        self.cached: dict[tuple[str, str], _GadgetModel] = {}
        self.discards: dict[str, _GadgetModel] = {}
        self.physical: dict[int, InstructionRuntime] = {}

    def add_code(self, code: Code) -> str:
        source = _code_source(code)
        if source not in self.codes:
            definition = cast(circuit.CodeDefinition, parse(source).definitions[0])
            definition.name = f"Code{len(self.codes)}"
            self.codes[source] = definition
        return self.codes[source].name

    def add_gadget(
        self, mnemonic: str, arguments: Mapping[str, InstructionCall.Argument]
    ) -> _GadgetModel:
        key = mnemonic, repr(sorted(arguments.items()))
        if key in self.cached:
            return self.cached[key]
        if mnemonic not in self.plan.gadgets:
            raise NotImplementedError(
                f"deq requires a top-level Qodec ISA gadget for {mnemonic!r}"
            )
        plan = self.plan.gadgets[mnemonic]
        gadget = plan.gadget
        validate_arguments(gadget.implements, arguments)
        for name in gadget.parameter_bindings:
            if name not in arguments:
                raise ValueError(f"Gadget parameter binding {name!r} has no argument")
        definition, width = self._gadget_definition(plan)
        flags = _flag_parities(gadget, width)
        defaults = _channel_defaults(plan, width, arguments)
        _apply_contract(definition, _local_contract(gadget, width, defaults), defaults)
        self.gadgets.append(definition)
        self.cached[key] = _GadgetModel(
            len(self.gadgets),
            tuple(port.code_name for port in definition.input_ports),
            tuple(port.code_name for port in definition.output_ports),
            width,
            gadget.implements.observe_count,
            flags,
        )
        return self.cached[key]

    def add_discard(self, name: str) -> _GadgetModel:
        if name not in self.discards:
            code = next(code for code in self.codes.values() if code.name == name)
            self.gadgets.append(
                circuit.GadgetDefinition(
                    f"Discard{len(self.gadgets)}",
                    [circuit.InputPort(name, list(range(code.n)))],
                )
            )
            self.discards[name] = _GadgetModel(len(self.gadgets), (name,), (), 0, 0, ())
        return self.discards[name]

    def build(self) -> tuple[circuit.DeqFile, JitLibraryArtifacts]:
        source = circuit.DeqFile(definitions=[*self.codes.values(), *self.gadgets])
        return source, build_jit_library_artifacts(source, jobs=1)

    def _gadget_definition(
        self, plan: GadgetPlan
    ) -> tuple[circuit.GadgetDefinition, int]:
        input_codes = [self.add_code(item.code) for item in plan.gadget.inputs]
        output_codes = [self.add_code(item.code) for item in plan.gadget.outputs]
        body = plan.body.create_runtime()
        isa = plan.gadget.circuit.instruction_set
        if id(isa) not in self.physical:
            self.physical[id(isa)] = InstructionRuntime(InstructionSet(isa))
        prepared = self.physical[id(isa)]
        physical = InstructionRuntime(
            prepared.instructions, operations=prepared.operations
        )
        if not isinstance(body, CallListRuntime) or any(
            call.call.arguments
            or call.call.select
            or call.declaration.flags
            or isinstance(physical.operations[call.call.mnemonic], PreparedAction)
            for call in body.calls
        ):
            raise NotImplementedError("deq requires a static Clifford gadget circuit")
        capacity = sum(
            physical.instructions.capacities[plan.label_types[label]]
            for label in plan.labels
        )
        physical.start(Resources(qubits=capacity))
        assert physical.layout is not None
        placement = physical.layout
        backend = _RecordingBackend(self.noise)
        backend.start(Resources(qubits=capacity))
        for layout in plan.inputs:
            for label, block_type in zip(layout.labels, layout.lower_types):
                placement.allocate(
                    label, block_type, physical.instructions.capacities[block_type]
                )

        def boundary(layouts: Sequence[EncodingLayout]) -> list[list[int]]:
            return [
                [
                    backend._target(qubit)
                    for label in layout.labels
                    for qubit in placement.blocks[label].qubits
                ]
                for layout in layouts
            ]

        inputs = boundary(plan.inputs)
        # Static calls have no arguments, selections, or flags to interpret.
        for prepared in body.calls:
            call = InstructionCall(
                prepared.call.mnemonic,
                operands=[str(operand) for operand in prepared.call.operands],
            )
            _record_operations(physical.handle(call), backend)
        outputs = boundary(plan.outputs)
        occupied = {qubit for port in inputs for qubit in port}
        definition = circuit.GadgetDefinition(
            f"Gadget{len(self.gadgets) + 1}",
            [
                *(
                    circuit.InputPort(code, port)
                    for code, port in zip(input_codes, inputs)
                ),
                *(
                    circuit.Instruction("R", targets=[circuit.QubitTarget(qubit)])
                    for qubit in range(backend.num_qubits)
                    if qubit not in occupied
                ),
                *_physical_instructions(
                    backend.instructions, self.noise, backend.noiseless
                ),
                *(
                    circuit.OutputPort(code, port)
                    for code, port in zip(output_codes, outputs)
                ),
            ],
        )
        return definition, backend.num_measurements


def _record_operations(
    requests: Requests[Readouts], backend: _RecordingBackend
) -> Readouts:
    with closing(requests):
        reply = None
        while True:
            try:
                operation = requests.send(reply)
            except StopIteration as completed:
                return completed.value
            if not isinstance(operation, Operation):
                raise NotImplementedError("deq requires physical Clifford operations")
            reply = backend.execute(operation)


def _physical_instructions(
    instructions: Sequence[tuple[object, ...]],
    noise: NoiseConfig | None,
    noiseless: Collection[int] = (),
) -> Iterable[circuit.Instruction]:
    names = {"s_adj": "S_DAG", "sx": "SQRT_X", "sx_adj": "SQRT_X_DAG"}
    for position, (opcode, *operands) in enumerate(instructions):
        if opcode in (QirInstructionId.MZ, QirInstructionId.RESET):
            gate = "M" if opcode == QirInstructionId.MZ else "R"
            name, targets = "mresetz", cast(list[int], operands[:1])
        else:
            name = next(name for name, value in _GATES.items() if value == opcode)
            gate, targets = names.get(name, name.upper()), cast(list[int], operands)
        yield circuit.Instruction(
            gate, targets=[circuit.QubitTarget(target) for target in targets]
        )
        if noise is not None and position not in noiseless:
            for axes, probability in _channel(noise, name, len(targets)):
                yield circuit.Instruction(
                    "CORRELATED_ERROR",
                    arguments=[probability],
                    targets=[
                        circuit.PauliTarget(axis, target)
                        for target, axis in zip(targets, axes)
                        if axis != "I"
                    ],
                )


def _channel(noise: NoiseConfig, name: str, width: int) -> list[tuple[str, float]]:
    table = getattr(noise, name)
    if any(
        getattr(table, "".join(axes))
        for axes in product("IXYZL", repeat=width)
        if "L" in axes
    ):
        raise NotImplementedError("deq does not support loss")
    alternatives = [
        ("".join(axes), float(getattr(table, "".join(axes))))
        for axes in product("IXYZ", repeat=width)
        if axes != ("I",) * width
    ]
    nonzero = [(axes, probability) for axes, probability in alternatives if probability]
    if len(nonzero) <= 1:
        return nonzero
    probabilities = {probability for _, probability in alternatives}
    if len(probabilities) == 1:
        probability = alternatives[0][1]
        size = 4**width
        if size * probability <= 1:
            independent = (
                0.5
                if size * probability == 1
                else -expm1(log1p(-size * probability) * (2 / size)) / 2
            )
            return [(axes, independent) for axes, _ in alternatives]
    raise NotImplementedError(
        f"deq requires a single Pauli mechanism or representable "
        f"depolarizing channel for {name}; general Pauli channels are not approximated"
    )


def _noise_key(noise: NoiseConfig | None) -> tuple[float, ...]:
    return tuple(
        0.0 if noise is None else float(getattr(getattr(noise, name), "".join(axes)))
        for name, width in (*_TABLE_WIDTHS.items(), ("mresetz", 1))
        for axes in product("IXYZL", repeat=width)
        if axes != ("I",) * width
    )


def _code_source(code: Code) -> str:
    def operator(text: str) -> str:
        value = pauli(text, code.physical_qubit_count)
        sign = "-" if value.phase == -1 else ""
        return sign + "*".join(f"{value[index]}{index}" for index in value.support)

    logicals = [f"LOGICAL {operator(x)} {operator(z)}" for x, z in zip(code.x, code.z)]
    stabilizers = " ".join(operator(text) for text in code.stabilizers)
    return (
        f"CODE Code [[{code.physical_qubit_count},{len(code.x)},1]] {{\n"
        + "\n".join(logicals)
        + (f"\nSTABILIZER {stabilizers}" if stabilizers else "")
        + "\n}"
    )


def _parity(parity: Parity, columns: Mapping[Hashable, int]) -> _Parity:
    mask = 0
    for variable in parity.variables:
        try:
            mask ^= 1 << columns[variable]
        except KeyError as error:
            raise ExecutionUnresolved(
                f"Gadget equation reference is unavailable: {variable}"
            ) from error
    return _Parity(mask, parity.constant)


def _flag_parities(gadget: Gadget, width: int) -> tuple[_Parity, ...]:
    observed = gadget.implements.observe_count
    if len(gadget.readouts) != observed + len(gadget.implements.flags):
        raise ExecutionUnresolved("Gadget readout equations have the wrong shape")
    columns: dict[Hashable, int] = {
        ("circuit_readout", None, None, None, index): index for index in range(width)
    }
    return tuple(
        _parity(parity, columns)
        for parity in _local_parities(
            gadget, (readout.equation for readout in gadget.readouts[observed:])
        )
    )


def _eliminate_aliases(
    equations: Iterable[_Parity],
    origin: int,
    count: int,
    *,
    defaults: Iterable[_Parity] = (),
) -> tuple[list[_Parity], list[_Parity]]:
    rows: dict[int, _Parity] = {}
    checks = []
    for equation in equations:
        remainder = _insert_equation(equation, rows, origin)
        if remainder is not None:
            checks.append(remainder)
    # Defaults fill only missing pivots; their residuals are not authored checks.
    for equation in defaults:
        _insert_equation(equation, rows, origin)
    readouts = []
    for index in range(count):
        value = _Parity(1 << (origin + index))
        while value.mask.bit_length() > origin:
            pivot = value.mask.bit_length() - 1
            if pivot not in rows:
                raise ExecutionUnresolved(
                    "Gadget readout equations are underdetermined"
                )
            value ^= rows[pivot]
        readouts.append(value)
    return readouts, checks


def _insert_equation(
    equation: _Parity, rows: dict[int, _Parity], origin: int
) -> _Parity | None:
    """Eliminate columns at or above origin; return any remaining constraint."""
    while equation.mask.bit_length() > origin:
        pivot = equation.mask.bit_length() - 1
        if pivot not in rows:
            rows[pivot] = equation
            return None
        equation ^= rows[pivot]
    return equation


@dataclass(frozen=True)
class _LocalContract:
    gadget: Gadget
    width: int
    readouts: list[_Parity]
    checks: list[_Parity]
    signs: dict[
        tuple[int, str, int], _Parity
    ]  # Only signs constrained by authored checks.
    inputs: tuple[tuple[int, str, int], ...]
    frames: dict[tuple[int, str, int], _Parity]


def _encoding_signs(encodings: Sequence[Encoding]) -> tuple[tuple[int, str, int], ...]:
    return tuple(
        (port, basis, index)
        for port, encoding in enumerate(encodings)
        for basis in ("stabilizers", "x", "z")
        for index in range(len(getattr(encoding.code, basis)))
    )


def _channel_equations(
    action: ChannelAction, inputs: Sequence[Pauli], outputs: Sequence[Pauli], width: int
) -> list[_Parity]:
    """Express channel relations over measurements and input/output sign columns."""

    def on_side(operator: Pauli, side: int) -> Pauli:
        operator = complex_conjugate_of(operator) if side == 0 else operator
        return relabel(
            operator, {qubit: 2 * qubit + side for qubit in operator.support}
        )

    relations = [
        *(
            PauliFrame(on_side(item.pauli, 0), item.frame)
            for item in action._observables.generators
        ),
        *(
            PauliFrame(on_side(item.pauli, 1), item.frame)
            for item in action._stabilizers.generators
        ),
        *(
            PauliFrame(on_side(source, 0) * on_side(image.pauli, 1), image.frame)
            for source, image in action._mapping.items()
        ),
    ]
    operators = [
        *(on_side(item, 0) for item in inputs),
        *(on_side(item, 1) for item in outputs),
    ]
    origin = 1 + max(
        (
            qubit
            for operator in [*(item.pauli for item in relations), *operators]
            for qubit in operator.support
        ),
        default=-1,
    )
    relations.extend(
        PauliFrame(operator * Pauli.z(origin + index))
        for index, operator in enumerate(operators)
    )
    constraints, _, _ = FrameGroup(relations).partition(
        over=range(origin, origin + len(operators))
    )
    return [
        _Parity(
            sum(1 << bit for bit in item.frame)
            ^ sum(1 << (width + qubit - origin) for qubit in item.pauli.support),
            item.pauli.phase == -1,
        )
        for item in constraints.generators
    ]


def _channel_defaults(
    plan: GadgetPlan, width: int, arguments: Mapping[str, InstructionCall.Argument]
) -> dict[tuple[int, str, int], _Parity]:
    gadget = plan.gadget
    layout = ProgramLayout.of(gadget.circuit)
    capacities = {
        block.name: block.encodes for block in gadget.circuit.instruction_set.blocks
    }
    bases = {str(label): base for label, base in layout.instance_bases.items()}
    next_qubit = layout.total_qubits
    for label in plan.labels:
        if label not in bases:
            width_of_block = capacities[plan.label_types[label]]
            bases[label] = (
                int(label) * width_of_block if label.isdecimal() else next_qubit
            )
            next_qubit = max(next_qubit, bases[label] + width_of_block)

    def boundary(
        encodings: Sequence[Encoding], layouts: Sequence[EncodingLayout]
    ) -> list[Pauli]:
        result = []
        for encoding, placement in zip(encodings, layouts):
            qubits = [
                bases[label] + index
                for label, block in zip(placement.labels, placement.lower_types)
                for index in range(capacities[block])
            ]
            for basis in ("stabilizers", "x", "z"):
                result.extend(
                    relabel(Pauli(text), dict(enumerate(qubits)))
                    for text in getattr(encoding.code, basis)
                )
        return result

    inputs = boundary(gadget.inputs, plan.inputs)
    outputs = boundary(gadget.outputs, plan.outputs)
    physical = _action_of(
        gadget.circuit,
        input_qubits=sorted(
            {qubit for operator in inputs for qubit in operator.support}
        ),
        output_support=sorted(
            {qubit for operator in outputs for qubit in operator.support}
        ),
    )
    origin = width + len(inputs)
    physical_rows = _channel_equations(physical, inputs, outputs, width)
    physical_signs, _ = _eliminate_aliases(
        (row for row in physical_rows if row.mask >> origin),
        origin,
        len(outputs),
        defaults=(_Parity(1 << (origin + index)) for index in range(len(outputs))),
    )
    logical = _action_propagations(gadget, arguments)
    input_keys = _encoding_signs(gadget.inputs)
    keep = (1 << width) - 1
    keep |= sum(
        1 << (width + index)
        for index, key in enumerate(input_keys)
        if key[1] == "stabilizers"
    )
    defaults = {}
    for key, row in zip(_encoding_signs(gadget.outputs), physical_signs):
        if key[1] == "stabilizers":
            defaults[key] = row
        else:
            declared = logical[key]
            mask = sum(1 << (width + input_keys.index(source)) for source in declared)
            defaults[key] = _Parity((row.mask & keep) ^ mask)
    return defaults


def _local_contract(
    gadget: Gadget, width: int, defaults: Mapping[tuple[int, str, int], _Parity]
) -> _LocalContract:
    inputs = _encoding_signs(gadget.inputs)
    outputs = _encoding_signs(gadget.outputs)
    variables = [
        *(("circuit_readout", None, None, None, index) for index in range(width)),
        *(("encoding", "in", *key) for key in inputs),
        *(("encoding", "out", *key) for key in outputs),
        *(
            ("readout", None, None, None, index)
            for index in range(len(gadget.readouts))
        ),
    ]
    columns: dict[Hashable, int] = {
        variable: index for index, variable in enumerate(variables)
    }
    origin = width + len(inputs)
    readouts = [expression(readout.equation) for readout in gadget.readouts]
    declared_checks = [expression(check) for check in gadget.checks]
    equations = [
        _Parity(1 << (origin + len(outputs) + index)) ^ _parity(readout, columns)
        for index, readout in enumerate(readouts)
    ]
    equations.extend(_parity(check, columns) for check in declared_checks)
    resolved, checks = _eliminate_aliases(
        equations,
        origin,
        len(outputs) + gadget.implements.observe_count,
        defaults=(
            _Parity(1 << (origin + index)) ^ defaults[key]
            for index, key in enumerate(outputs)
        ),
    )
    specified = set()
    aliases = set()
    pending = list(declared_checks)
    while pending:
        for variable in pending.pop().variables:
            kind, boundary, entry, basis, index = cast(
                tuple[str, str, int, str, int], variable
            )
            if kind == "encoding" and boundary == "out":
                specified.add((entry, basis, index))
            elif kind == "readout" and index not in aliases:
                aliases.add(index)
                pending.append(readouts[index])
    frames = {
        (frame.output, frame.basis, frame.logical): _parity(frame.parity, columns)
        for frame in prepare_frames(gadget)
    }
    return _LocalContract(
        gadget,
        width,
        resolved[len(outputs) :],
        checks,
        {key: value for key, value in zip(outputs, resolved) if key in specified},
        inputs,
        frames,
    )


def _check_row(row: _Parity, contract: _LocalContract) -> _Parity:
    inputs = contract.gadget.inputs
    input_count = sum(len(encoding.code.stabilizers) for encoding in inputs)
    result = _Parity(constant=row.constant)
    for index in row.indices:
        if index < contract.width:
            result ^= _Parity(1 << (input_count + index))
        else:
            port, basis, position = contract.inputs[index - contract.width]
            if basis != "stabilizers":
                raise NotImplementedError(
                    "deq cannot use an input logical sign as a detection check"
                )
            offset = sum(len(encoding.code.stabilizers) for encoding in inputs[:port])
            result ^= _Parity(1 << (offset + position))
    return result


def _manual_checks(
    contract: _LocalContract, defaults: Mapping[tuple[int, str, int], _Parity]
) -> list[_Parity]:
    """Translate authored checks and completed output stabilizer relations."""
    gadget = contract.gadget
    input_count = sum(len(encoding.code.stabilizers) for encoding in gadget.inputs)
    origin = input_count + contract.width
    authored = [_check_row(row, contract) for row in contract.checks]
    for (port, basis, index), default in defaults.items():
        if basis != "stabilizers":
            continue
        value = contract.signs.get((port, basis, index), default)
        offset = sum(
            len(encoding.code.stabilizers) for encoding in gadget.outputs[:port]
        )
        authored.append(
            _Parity(1 << (origin + offset + index)) ^ _check_row(value, contract)
        )
    return authored


def _measurement_target(
    index: int, contract: _LocalContract
) -> (
    circuit.InputVirtualTarget
    | circuit.PhysicalMeasurementTarget
    | circuit.OutputVirtualTarget
):
    for port, encoding in enumerate(contract.gadget.inputs):
        width = len(encoding.code.stabilizers)
        if index < width:
            return circuit.InputVirtualTarget(port, index)
        index -= width
    if index < contract.width:
        return circuit.PhysicalMeasurementTarget(index)
    index -= contract.width
    for port, encoding in enumerate(contract.gadget.outputs):
        width = len(encoding.code.stabilizers)
        if index < width:
            return circuit.OutputVirtualTarget(port, index)
        index -= width
    raise ExecutionUnresolved("Encoding sign reference is unavailable")


def _apply_contract(
    gadget: circuit.GadgetDefinition,
    contract: _LocalContract,
    defaults: Mapping[tuple[int, str, int], _Parity],
) -> None:
    gadget.decorators.append(
        circuit.Decorator("CHECKS", ("manual", circuit.KeywordArg("verify", 0)))
    )
    for row in _manual_checks(contract, defaults):
        gadget.body.append(
            circuit.CheckStatement(
                targets=[_measurement_target(index, contract) for index in row.indices],
                flip=row.constant,
            )
        )
    gadget.body.extend(_frame_propagations(contract, defaults))
    for row in contract.readouts:
        gadget.body.append(
            circuit.ReadoutStatement(
                targets=[
                    circuit.PhysicalMeasurementTarget(index)
                    for index in row.indices
                    if index < contract.width
                ],
                flip=row.constant,
            )
        )
    for (output, basis, logical), value in contract.frames.items():
        for index in value.indices:
            gadget.body.append(
                circuit.ConditionalStatement(
                    circuit.PhysicalMeasurementTarget(index),
                    [
                        circuit.LogicalPauliTarget(
                            "X" if basis == "z" else "Z", logical, "OUT", output
                        )
                    ],
                )
            )


def _action_propagations(
    gadget: Gadget, arguments: Mapping[str, InstructionCall.Argument]
) -> dict[tuple[int, str, int], tuple[tuple[int, str, int], ...]]:
    action = declared_action_of(gadget, arguments=arguments)
    inputs = [key for key in _encoding_signs(gadget.inputs) if key[1] != "stabilizers"]
    outputs = [
        key for key in _encoding_signs(gadget.outputs) if key[1] != "stabilizers"
    ]

    def operators(encodings: Sequence[Encoding]) -> list[Pauli]:
        result = []
        offset = 0
        for encoding in encodings:
            for axis in ("X", "Z"):
                result.extend(
                    Pauli({offset + index: axis})
                    for index in range(len(encoding.code.x))
                )
            offset += len(encoding.code.x)
        return result

    width = len(gadget.readouts)
    equations = _channel_equations(
        action, operators(gadget.inputs), operators(gadget.outputs), width
    )
    count = len(outputs) + width
    rows = [
        _Parity(
            (row.mask >> width)
            | ((row.mask & ((1 << width) - 1)) << (len(inputs) + len(outputs)))
        )
        for row in equations
    ]
    resolved, _ = _eliminate_aliases(
        rows,
        len(inputs),
        count,
        defaults=(_Parity(1 << (len(inputs) + index)) for index in range(count)),
    )
    return {
        key: tuple(inputs[index] for index in row.indices)
        for key, row in zip(outputs, resolved)
    }


def _frame_propagations(
    contract: _LocalContract, defaults: Mapping[tuple[int, str, int], _Parity]
) -> list[circuit.PropagateStatement]:
    statements = []
    for key, default in defaults.items():
        port, basis, logical = key
        if basis == "stabilizers":
            continue
        value = contract.signs.get(key, default)
        terms: list[circuit.PropagateTerm] = []
        for index in value.indices:
            if index < contract.width:
                terms.append(circuit.PhysicalMeasurementTarget(index))
            else:
                entry, input_basis, position = contract.inputs[index - contract.width]
                terms.append(
                    circuit.DestabilizerTarget(entry, position)
                    if input_basis == "stabilizers"
                    else circuit.LogicalPauliTarget(
                        "X" if input_basis == "z" else "Z", position, "IN", entry
                    )
                )
        statements.append(
            circuit.PropagateStatement(
                circuit.LogicalPauliTarget(
                    "X" if basis == "z" else "Z", logical, "OUT", port
                ),
                terms,
                flip=value.constant ^ contract.frames.get(key, _Parity()).constant,
            )
        )
    return statements
