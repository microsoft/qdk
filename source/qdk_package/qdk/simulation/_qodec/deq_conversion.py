"""Compile reusable deq models from Qodec gadgets, without a program trace.

Encoding signs describe frame corrections, not logical measurement values.
Qodec signs name observables; deq logical targets name correction Paulis.
Flags use only local physical measurements and never receive frame corrections.
"""

from __future__ import annotations

from collections.abc import Hashable, Iterable, Mapping, Sequence
from contextlib import closing
from dataclasses import dataclass
from itertools import product
from math import expm1, log1p
from typing import cast

from paulimer import CliffordUnitary
from qodec import Code, Gadget, Layer
from qodec.gadgets import Encoding
from qodec.instructions import InstructionCall
from deq.circuit import model as circuit  # pyright: ignore[reportMissingImports]
from deq.circuit.parser import parse  # pyright: ignore[reportMissingImports]
from deq.transpiler.check_plugins import (  # pyright: ignore[reportMissingImports]
    resolve_gadget_checks,
)
from deq.transpiler.jit_library_builder import (  # pyright: ignore[reportMissingImports]
    JitLibraryArtifacts,
    build_jit_library_artifacts,
)
from deq.transpiler.jit_noise_builder import (  # pyright: ignore[reportMissingImports]
    compute_correction_propagation,
)
from deq.transpiler.jit_transpiler import (  # pyright: ignore[reportMissingImports]
    PortColumnLayout,
)

from .. import NoiseConfig
from ..._native import QirInstructionId
from .action_runtime import prepare_actions
from .call_binding import validate_arguments
from .circuit_runtime import CallListRuntime
from .clifford_semantics import pauli
from .encoding_layout import EncodingLayout
from .instruction_set import InstructionRuntime, InstructionSet, PreparedAction
from .layer_runtime import GadgetPlan, LayerPlan
from .native_batch import _GATES, _RecordingBackend, _TABLE_WIDTHS
from .protocols import ExecutionUnresolved, Readouts, Requests, Resources
from .quantum_instruments import CliffordGate, PauliGate
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
    def __init__(self, layer: Layer, noise: NoiseConfig | None) -> None:
        self.plan = LayerPlan(layer)
        self.noise = noise
        self.codes: dict[str, circuit.CodeDefinition] = {}
        self.gadgets: list[circuit.GadgetDefinition] = []
        self.cached: dict[tuple[str, str], _GadgetModel] = {}
        self.discards: dict[str, _GadgetModel] = {}

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
        codes = {code.name: code for code in self.codes.values()}
        inferred = resolve_gadget_checks(definition, codes)
        propagations = _complete_propagations(
            definition, codes, _action_propagations(gadget, arguments)
        )
        defaults = _default_signs(gadget, width, propagations, inferred.unfinished)
        _apply_contract(
            definition,
            _local_contract(gadget, width, defaults),
            propagations,
            inferred.unfinished,
        )
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
        physical = InstructionRuntime(
            InstructionSet(plan.gadget.circuit.instruction_set)
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
                *_physical_instructions(backend.instructions, self.noise),
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
    instructions: Sequence[tuple[object, ...]], noise: NoiseConfig | None
) -> Iterable[circuit.Instruction]:
    names = {"s_adj": "S_DAG", "sx": "SQRT_X", "sx_adj": "SQRT_X_DAG"}
    for opcode, *operands in instructions:
        if opcode in (QirInstructionId.MZ, QirInstructionId.RESET):
            gate = "M" if opcode == QirInstructionId.MZ else "R"
            name, targets = "mresetz", cast(list[int], operands[:1])
        else:
            name = next(name for name, value in _GATES.items() if value == opcode)
            gate, targets = names.get(name, name.upper()), cast(list[int], operands)
        yield circuit.Instruction(
            gate, targets=[circuit.QubitTarget(target) for target in targets]
        )
        if noise is not None:
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


def _default_signs(
    gadget: Gadget,
    width: int,
    propagations: Sequence[circuit.PropagateStatement],
    unfinished: Sequence[tuple[frozenset[int], bool]],
) -> dict[tuple[int, str, int], _Parity]:
    """Express local frame defaults in the authored equations' column space."""
    inputs = _encoding_signs(gadget.inputs)
    columns = {key: width + index for index, key in enumerate(inputs)}
    signs = {}
    for statement in propagations:
        target = statement.target
        assert target.port_index is not None
        value = _Parity(constant=statement.flip)
        for term in statement.terms:
            if isinstance(term, circuit.PhysicalMeasurementTarget):
                column = term.index
            elif isinstance(term, circuit.DestabilizerTarget):
                column = columns[term.port_index, "stabilizers", term.stab_index]
            elif isinstance(term, circuit.LogicalPauliTarget):
                assert term.port_index is not None
                column = columns[
                    term.port_index, "z" if term.pauli == "X" else "x", term.index
                ]
            else:
                raise ExecutionUnresolved(f"Unsupported default frame term: {term}")
            value ^= _Parity(1 << column)
        signs[target.port_index, "z" if target.pauli == "X" else "x", target.index] = (
            value
        )

    input_stabilizers = [columns[key] for key in inputs if key[1] == "stabilizers"]
    measurements = [*input_stabilizers, *range(width)]
    output_stabilizers = [
        key for key in _encoding_signs(gadget.outputs) if key[1] == "stabilizers"
    ]
    resolved, _ = _eliminate_aliases(
        (
            _Parity(sum(1 << index for index in indices), flip)
            for indices, flip in unfinished
        ),
        len(measurements),
        len(output_stabilizers),
    )
    for key, row in zip(output_stabilizers, resolved):
        signs[key] = _Parity(
            sum(1 << measurements[index] for index in row.indices), row.constant
        )
    return signs


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
    inferred: Sequence[tuple[frozenset[int], bool]], contract: _LocalContract
) -> tuple[list[_Parity], list[_Parity]]:
    """Complete only missing port propagation; never audit authored checks."""
    gadget = contract.gadget
    input_count = sum(len(encoding.code.stabilizers) for encoding in gadget.inputs)
    origin = input_count + contract.width
    output_count = sum(len(encoding.code.stabilizers) for encoding in gadget.outputs)
    authored = [_check_row(row, contract) for row in contract.checks]
    for (port, basis, index), value in contract.signs.items():
        if basis != "stabilizers":
            continue
        offset = sum(
            len(encoding.code.stabilizers) for encoding in gadget.outputs[:port]
        )
        authored.append(
            _Parity(1 << (origin + offset + index)) ^ _check_row(value, contract)
        )
    pivots: dict[int, _Parity] = {}
    finished = []
    for row in authored:
        remainder = _insert_equation(row, pivots, origin)
        if remainder is not None:
            finished.append(remainder)
    for indices, constant in inferred:
        row = _Parity(sum(1 << index for index in indices), constant)
        _insert_equation(row, pivots, origin)
    resolved, _ = _eliminate_aliases(pivots.values(), origin, output_count)
    return finished, [
        value ^ _Parity(1 << (origin + index)) for index, value in enumerate(resolved)
    ]


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
    propagations: Sequence[circuit.PropagateStatement],
    unfinished_checks: Sequence[tuple[frozenset[int], bool]],
) -> None:
    finished, unfinished = _manual_checks(unfinished_checks, contract)
    gadget.decorators.append(
        circuit.Decorator("CHECKS", ("manual", circuit.KeywordArg("verify", 0)))
    )
    for row in (*finished, *unfinished):
        gadget.body.append(
            circuit.CheckStatement(
                targets=[_measurement_target(index, contract) for index in row.indices],
                flip=row.constant,
            )
        )
    _apply_authored_frames(contract, propagations)
    gadget.body.extend(propagations)
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


def _logical_targets(layout: PortColumnLayout) -> dict[int, circuit.LogicalPauliTarget]:
    targets = {}
    for column, (observable, is_x) in layout.col_to_obs.items():
        port, index = layout.obs_to_port[observable]
        targets[column] = circuit.LogicalPauliTarget(
            "Z" if is_x else "X", index, layout.port_kind, port
        )
    return targets


def _complete_propagations(
    gadget: circuit.GadgetDefinition,
    codes: dict[str, circuit.CodeDefinition],
    action: Sequence[circuit.PropagateStatement] | None,
) -> list[circuit.PropagateStatement]:
    """Add deq's physical correction terms to the declared logical transport."""
    inputs, outputs = gadget.input_ports, gadget.output_ports
    input_layout = PortColumnLayout(inputs, codes)
    declared = {statement.target: statement for statement in action or ()}
    statements = {
        row: declared.get(target, circuit.PropagateStatement(target))
        for row, target in _logical_targets(PortColumnLayout(outputs, codes)).items()
    }
    if not statements:
        return []
    propagation, measurements = compute_correction_propagation(
        gadget,
        codes,
        input_ports=inputs,
        output_ports=outputs,
        unfinished_checks=(),
        input_virtual_count=sum(
            len(codes[port.code_name].stabilizers) for port in inputs
        ),
    )
    targets: dict[int, circuit.PropagateTerm] = {
        column: circuit.DestabilizerTarget(port, index)
        for column, (port, index) in input_layout.generator_map.items()
    }
    if action is None:
        targets.update(_logical_targets(input_layout))
    for row, column in zip(propagation.i, propagation.j):
        if row in statements and column in targets:
            statements[row].terms.append(targets[column])
    for row, measurement in measurements:
        statements[row].terms.append(circuit.PhysicalMeasurementTarget(measurement))
    return list(statements.values())


def _action_propagations(
    gadget: Gadget, arguments: Mapping[str, InstructionCall.Argument]
) -> list[circuit.PropagateStatement] | None:
    inputs = [
        (port, index)
        for port, encoding in enumerate(gadget.inputs)
        for index in range(len(encoding.code.x))
    ]
    outputs = [
        (port, index)
        for port, encoding in enumerate(gadget.outputs)
        for index in range(len(encoding.code.x))
    ]
    if len(inputs) != len(outputs):
        return None
    program = prepare_actions(gadget.implements, len(inputs), len(outputs), arguments)
    action = CliffordUnitary.identity(len(inputs))
    for step in program.steps:
        if step.guard is not None:
            if step.guard.outcomes:
                return None
            if not step.guard.accepts(()):
                continue
        if isinstance(step.instrument, CliffordGate):
            action.left_mul_clifford(step.instrument.operator, list(range(len(inputs))))
        elif not isinstance(step.instrument, PauliGate):
            return None
    # Conjugation signs are global phases of corrections, not frame bits.
    statements = []
    for logical, (port, index) in enumerate(outputs):
        for axis, image in (
            ("X", action.preimage_z(logical)),
            ("Z", action.preimage_x(logical)),
        ):
            statements.append(
                circuit.PropagateStatement(
                    circuit.LogicalPauliTarget(axis, index, "OUT", port),
                    [
                        circuit.LogicalPauliTarget(
                            incoming, inputs[source][1], "IN", inputs[source][0]
                        )
                        for source in image.support
                        for incoming, paulis in (("Z", ("X", "Y")), ("X", ("Z", "Y")))
                        if image[source] in paulis
                    ],
                )
            )
    return statements


def _apply_authored_frames(
    contract: _LocalContract, statements: Sequence[circuit.PropagateStatement]
) -> None:
    for statement in statements:
        target = statement.target
        assert target.port_index is not None
        key = (target.port_index, "z" if target.pauli == "X" else "x", target.index)
        statement.flip = contract.frames.get(key, _Parity()).constant
        if key not in contract.signs:
            continue
        value = contract.signs[key]
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
        statement.terms = terms
        statement.flip ^= value.constant
