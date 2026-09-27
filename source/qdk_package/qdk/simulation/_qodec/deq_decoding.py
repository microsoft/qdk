"""Decode bounded groups of connected Clifford gadgets with deq windows.

Encoding signs denote Pauli-frame corrections, not logical measurement values.
Qodec signs name observables; deq logical targets name correction Paulis.
deq owns code-port connections, circuit fault propagation, and decoding.
Raw rejection flags are compiled separately: unlike logical readouts, they must
include authored frame changes but must not receive inferred error corrections.
"""

from __future__ import annotations

import asyncio
from collections.abc import Hashable, Iterable, Mapping, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from itertools import product
from math import expm1, log1p
from typing import cast, Literal

import numpy as np
from paulimer import CliffordUnitary
from qodec import Code, Gadget
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
from deq.proto import (  # pyright: ignore[reportMissingImports]
    coordinator_pb2 as coordinator,
    deq_bin_pb2 as model,
    deq_jit_pb2 as jit,
    util_pb2 as util,
)
from deq.runtime import Runtime  # pyright: ignore[reportMissingImports]

from qdk import Result
from .. import NoiseConfig
from ..._native import QirInstructionId
from ._interpreter import OutputRecordValue
from .action_runtime import prepare_actions
from .clifford_semantics import pauli
from .deq_composition import _Composite, _compose_gadgets, _model_size
from .native_batch import (
    CircuitTrace,
    _Before,
    _Decode,
    _Discard,
    _FrameMasks,
    _GATES,
    _Readout,
    _TABLE_WIDTHS,
    _static_flips,
)
from .protocols import (
    BlockReference,
    DecoderSession,
    ExecutionRejected,
    ExecutionUnresolved,
    Invocation,
)
from .quantum_instruments import CliffordGate, PauliGate
from .readout_equations import InconsistentParity, Parity, expression, prepare_frames
from .selection import Selection


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


class DeqModel:
    def __call__(self, seed: int | None = None) -> DecoderSession:
        raise NotImplementedError("deq requires a complete native trace")

    def prepare_circuit(
        self, trace: CircuitTrace, noise: NoiseConfig | None, /
    ) -> DeqBatch:
        source, artifacts, gadgets = _connected_library(trace, noise)
        readouts = _readout_plan(trace, gadgets)
        library, composites = _compose_gadgets(source, artifacts, gadgets)
        return DeqBatch(trace, readouts, library, _noise_key(noise), composites)


@dataclass(frozen=True)
class _ReadoutPlan:
    """Route flat deq replies into QDK records while leaving raw flags untouched."""

    count: int
    flags: tuple[tuple[int, _Parity], ...]
    selections: tuple[tuple[Selection, tuple[int, ...]], ...]
    sources: tuple[int, ...]
    decoded_sources: tuple[int, ...]
    decoded_destinations: tuple[int, ...]
    check_indices: tuple[int, ...]


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


def _physical_parity(parity: _Parity, records: Sequence[_Parity]) -> _Parity:
    result = _Parity(constant=parity.constant)
    for index in parity.indices:
        # Input-frame effects are already carried by these physical records.
        if index < len(records):
            result ^= records[index]
    return result


def _eliminate_aliases(
    equations: Iterable[_Parity], origin: int, count: int, *, free_count: int = 0
) -> tuple[list[_Parity], list[_Parity]]:
    rows: dict[int, _Parity] = {}
    checks = []
    for equation in equations:
        while equation.mask.bit_length() > origin:
            pivot = equation.mask.bit_length() - 1
            if pivot not in rows:
                rows[pivot] = equation
                break
            equation ^= rows[pivot]
        else:
            if equation.constant and not equation.mask:
                raise InconsistentParity(
                    "Parity equations contain contradictory evidence"
                )
            checks.append(equation)
    readouts = []
    for index in range(count):
        value = _Parity(1 << (origin + index))
        while value.mask.bit_length() > origin:
            pivot = value.mask.bit_length() - 1
            if pivot not in rows:
                if pivot < origin + free_count:
                    value ^= _Parity(1 << pivot)
                    continue
                raise ExecutionUnresolved(
                    "Gadget readout equations are underdetermined"
                )
            value ^= rows[pivot]
        readouts.append(value)
    return readouts, checks


def _output_frames(
    event: _Decode, signs: Mapping[tuple[int, str, int], _Parity], origin: int
) -> tuple[list[tuple[int, str, _Parity]], list[_Parity]]:
    frames = []
    checks = []
    for entry, encoding in enumerate(event.invocation.gadget.outputs):
        if not any(
            value.mask or value.constant
            for key, value in signs.items()
            if key[0] == entry
        ):
            continue
        block = event.invocation.outputs[entry]
        qubits = event.qubits[block]
        axes = [(target, axis) for target in range(len(qubits)) for axis in ("x", "z")]
        equations = []
        for (output, basis, index), value in signs.items():
            if output != entry:
                continue
            operator = pauli(getattr(encoding.code, basis)[index], len(qubits))
            mask = sum(
                1 << (origin + column)
                for column, (target, axis) in enumerate(axes)
                if not operator.commutes_with(
                    pauli(f"{axis.upper()}_{target}", len(qubits))
                )
            )
            equations.append(_Parity(mask) ^ value)
        corrections, constraints = _eliminate_aliases(
            equations, origin, len(axes), free_count=len(axes)
        )
        checks.extend(constraints)
        frames.extend(
            (qubits[target], axis, value)
            for (target, axis), value in zip(axes, corrections)
            if value.mask or value.constant
        )
    return frames, checks


def _apply_frame(records: list[_Parity], affected: int, value: _Parity) -> None:
    for record in range(affected.bit_length()):
        if affected >> record & 1:
            records[record] ^= value


def _flag_parities(
    trace: CircuitTrace, gadgets: Sequence[_Gadget]
) -> tuple[tuple[int, _Parity], ...]:
    masks = _FrameMasks(trace.instructions)
    static = _static_flips(trace.instructions, trace.frames, masks)
    records = [
        _Parity(1 << index, bool(static >> index & 1))
        for index in range(trace.num_measurements)
    ]
    flags = []
    offset = 0
    contracts = iter(
        gadget.contract for gadget in gadgets if gadget.contract is not None
    )
    for event in trace.events:
        if not isinstance(event, _Decode):
            continue
        gadget = event.invocation.gadget
        count = len(gadget.readouts)
        contract = next(contracts)
        local = records[event.start : event.start + event.width]
        for check in contract.checks:
            value = _physical_parity(check, local)
            if not value.mask and value.constant:
                raise InconsistentParity(
                    "Parity equations contain contradictory evidence"
                )
        flags.extend(
            (offset + index, _physical_parity(contract.readouts[index], local))
            for index in range(gadget.implements.observe_count, count)
        )
        offset += count
        signs = {
            key: _physical_parity(value, local) for key, value in contract.signs.items()
        }
        corrections, _ = _output_frames(event, signs, len(records))
        for target, axis, value in corrections:
            _apply_frame(records, masks.mask(event.position, target, axis), value)
        for (output, basis, logical), parity in contract.frames.items():
            value = _physical_parity(parity, local)
            block = event.invocation.outputs[output]
            qubits = event.qubits[block]
            code = gadget.outputs[output].code
            operator = pauli(
                getattr(code, "x" if basis == "z" else "z")[logical],
                len(qubits),
            )
            affected = 0
            for target in operator.support:
                affected ^= masks.mask(
                    event.position, qubits[target], operator[target].lower()
                )
            _apply_frame(records, affected, value)
    return tuple(flags)


def _readout_plan(
    trace: CircuitTrace,
    gadgets: Sequence[_Gadget],
) -> _ReadoutPlan:
    count = 0
    selections = []
    offsets = {}
    for index, event in enumerate(trace.events):
        if not isinstance(event, _Decode):
            continue
        gadget = event.invocation.gadget
        width = len(gadget.readouts)
        if width != gadget.implements.observe_count + len(gadget.implements.flags):
            raise ExecutionUnresolved("Gadget readout equations have the wrong shape")
        offsets[index] = count
        indices = tuple(range(count + gadget.implements.observe_count, count + width))
        selections.append((event.selection, indices))
        count += width
    decoded = []
    checks = []
    offset = 0
    for gadget in gadgets:
        stop = offset + len(gadget.readouts)
        decoded.extend(range(offset, stop))
        checks.extend(range(stop, stop + gadget.checks))
        offset = stop + gadget.checks
    return _ReadoutPlan(
        count,
        (
            _flag_parities(trace, gadgets)
            if any(indices for _, indices in selections)
            else ()
        ),
        tuple(selections),
        tuple(offsets[event] + index for event, index in trace.sources),
        tuple(decoded),
        tuple(index for gadget in gadgets for index in gadget.readouts),
        tuple(checks),
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


@dataclass(frozen=True)
class _Gadget:
    gtype: int
    contract: _LocalContract | None
    start: int
    width: int
    readouts: range
    checks: int
    connectors: tuple[tuple[int, int], ...]


def _code_source(code: Code, width: int, name: str) -> str:
    def operator(text: str) -> str:
        value = pauli(text, width)
        sign = "-" if value.phase == -1 else ""
        return sign + "*".join(f"{value[index]}{index}" for index in value.support)

    logicals = [f"LOGICAL {operator(x)} {operator(z)}" for x, z in zip(code.x, code.z)]
    stabilizers = " ".join(operator(text) for text in code.stabilizers)
    return (
        f"CODE {name} [[{width},{len(code.x)},1]] {{\n"
        + "\n".join(logicals)
        + (f"\nSTABILIZER {stabilizers}" if stabilizers else "")
        + "\n}"
    )


def _body_source(
    trace: CircuitTrace,
    before: _Before,
    event: _Decode,
    qubits: Mapping[int, int],
    noise: NoiseConfig | None,
) -> list[str]:
    inputs = {target for block in before.qubits.values() for target in block}
    body = [f"R {local}" for target, local in qubits.items() if target not in inputs]
    frames: dict[int, list[tuple[int, str]]] = {}
    for position, target, axis in trace.frames[
        before.frame_position : event.frame_position
    ]:
        frames.setdefault(position, []).append((qubits[target], axis))
    names = {"s_adj": "S_DAG", "sx": "SQRT_X", "sx_adj": "SQRT_X_DAG"}
    for position in range(before.position, event.position + 1):
        body.extend(
            f"{axis.upper()} {target}" for target, axis in frames.get(position, ())
        )
        if position == event.position:
            break
        opcode, *operands = trace.instructions[position]
        if opcode in (QirInstructionId.MZ, QirInstructionId.RESET):
            gate = "M" if opcode == QirInstructionId.MZ else "R"
            name, targets = "mresetz", cast(list[int], operands[:1])
        else:
            name = next(name for name, value in _GATES.items() if value == opcode)
            gate, targets = names.get(name, name.upper()), cast(list[int], operands)
        local = [qubits[target] for target in targets]
        body.append(f"{gate} " + " ".join(map(str, local)))
        if noise is not None:
            for axes, probability in _channel(noise, name, len(local)):
                targets_text = " ".join(
                    f"{axis}{target}"
                    for target, axis in zip(local, axes)
                    if axis != "I"
                )
                body.append(f"CORRELATED_ERROR({probability}) {targets_text}")
    return body


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


@dataclass(frozen=True)
class _LocalContract:
    gadget: Gadget
    width: int
    readouts: list[_Parity]
    checks: list[_Parity]
    signs: dict[tuple[int, str, int], _Parity]
    inputs: tuple[tuple[int, str, int], ...]
    specified: frozenset[tuple[int, str, int]]
    frames: dict[tuple[int, str, int], _Parity]


def _local_contract(event: _Decode) -> _LocalContract:
    gadget = event.invocation.gadget
    inputs = tuple(
        (port, basis, index)
        for port, encoding in enumerate(gadget.inputs)
        for basis in ("stabilizers", "x", "z")
        for index in range(len(getattr(encoding.code, basis)))
    )
    outputs = tuple(
        (port, basis, index)
        for port, encoding in enumerate(gadget.outputs)
        for basis in ("stabilizers", "x", "z")
        for index in range(len(getattr(encoding.code, basis)))
    )
    variables = [
        *(("circuit_readout", None, None, None, index) for index in range(event.width)),
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
    origin = event.width + len(inputs)
    readouts = [expression(readout.equation) for readout in gadget.readouts]
    declared_checks = [expression(check) for check in gadget.checks]
    equations = [
        _Parity(1 << (origin + len(outputs) + index)) ^ _parity(readout, columns)
        for index, readout in enumerate(readouts)
    ]
    equations.extend(_parity(check, columns) for check in declared_checks)
    resolved, checks = _eliminate_aliases(
        equations, origin, len(outputs) + len(readouts), free_count=len(outputs)
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
        event.width,
        resolved[len(outputs) :],
        checks,
        dict(zip(outputs, resolved[: len(outputs)])),
        inputs,
        frozenset(specified),
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
    inferred: Sequence[tuple[frozenset[int], bool]],
    contract: _LocalContract,
) -> tuple[list[_Parity], list[_Parity]]:
    """Complete only missing port propagation; never audit authored checks."""
    gadget = contract.gadget
    input_count = sum(len(encoding.code.stabilizers) for encoding in gadget.inputs)
    origin = input_count + contract.width
    output_count = sum(len(encoding.code.stabilizers) for encoding in gadget.outputs)
    authored = [_check_row(row, contract) for row in contract.checks]
    for (port, basis, index), value in contract.signs.items():
        if basis != "stabilizers" or (port, basis, index) not in contract.specified:
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
        while row.mask.bit_length() > origin:
            pivot = row.mask.bit_length() - 1
            if pivot not in pivots:
                pivots[pivot] = row
                break
            row ^= pivots[pivot]
        else:
            finished.append(row)
    for indices, constant in inferred:
        row = _Parity(sum(1 << index for index in indices), constant)
        while row.mask.bit_length() > origin:
            pivot = row.mask.bit_length() - 1
            if pivot not in pivots:
                pivots[pivot] = row
                break
            row ^= pivots[pivot]
    resolved, _ = _eliminate_aliases(pivots.values(), origin, output_count)
    return finished, [
        value ^ _Parity(1 << (origin + index)) for index, value in enumerate(resolved)
    ]


def _local_definition(
    definitions: list[circuit.CodeDefinition],
    source: str,
    contract: _LocalContract,
    invocation: Invocation,
) -> tuple[circuit.GadgetDefinition, int]:
    parsed = parse(source)
    gadget = cast(circuit.GadgetDefinition, parsed.definitions[-1])
    codes = {definition.name: definition for definition in definitions}
    inferred = resolve_gadget_checks(gadget, codes)
    finished, unfinished = _manual_checks(inferred.unfinished, contract)
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
    propagations = _complete_propagations(
        gadget, codes, _action_propagations(invocation)
    )
    _apply_authored_frames(contract, propagations)
    gadget.body.extend(propagations)
    logical_count = contract.gadget.implements.observe_count
    for row in contract.readouts[:logical_count]:
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
    input_count = sum(
        len(encoding.code.stabilizers) for encoding in contract.gadget.inputs
    )
    for row in finished:
        gadget.body.append(
            circuit.ReadoutStatement(
                targets=[
                    circuit.PhysicalMeasurementTarget(index - input_count)
                    for index in row.indices
                    if index >= input_count
                ],
                flip=row.constant,
            )
        )
    for (output, basis, logical), value in contract.frames.items():
        axis = "X" if basis == "z" else "Z"
        for index in value.indices:
            gadget.body.append(
                circuit.ConditionalStatement(
                    circuit.PhysicalMeasurementTarget(index),
                    [circuit.LogicalPauliTarget(axis, logical, "OUT", output)],
                )
            )
    return gadget, len(finished)


def _logical_targets(
    layout: PortColumnLayout,
) -> dict[int, circuit.LogicalPauliTarget]:
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
    inputs = [port for port in gadget.body if isinstance(port, circuit.InputPort)]
    outputs = [port for port in gadget.body if isinstance(port, circuit.OutputPort)]
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
    invocation: Invocation,
) -> list[circuit.PropagateStatement] | None:
    gadget = invocation.gadget
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
    program = prepare_actions(
        gadget.implements, len(inputs), len(outputs), invocation.call.arguments
    )
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
            # Other action forms retain deq's physical propagation relations.
            return None
    # Discard the global phase of each conjugated Pauli correction.
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
    contract: _LocalContract,
    statements: Sequence[circuit.PropagateStatement],
) -> None:
    for statement in statements:
        target = statement.target
        assert target.port_index is not None
        key = (target.port_index, "z" if target.pauli == "X" else "x", target.index)
        statement.flip = contract.frames.get(key, _Parity()).constant
        if key not in contract.specified:
            continue
        value = contract.signs[key]
        terms: list[circuit.PropagateTerm] = []
        for index in value.indices:
            if index < contract.width:
                terms.append(circuit.PhysicalMeasurementTarget(index))
            else:
                entry, input_basis, position = contract.inputs[index - contract.width]
                axis = "X" if input_basis == "z" else "Z"
                terms.append(
                    circuit.DestabilizerTarget(entry, position)
                    if input_basis == "stabilizers"
                    else circuit.LogicalPauliTarget(axis, position, "IN", entry)
                )
        statement.terms = terms
        statement.flip ^= value.constant


class _LocalLibraryBuilder:
    def __init__(self) -> None:
        self.codes: list[circuit.CodeDefinition] = []
        self.code_names: dict[str, str] = {}
        self.gadgets: list[circuit.GadgetDefinition] = []
        self.cached: dict[str, tuple[int, int, _LocalContract | None]] = {}

    def add_code(self, code: Code, width: int) -> str:
        key = _code_source(code, width, "Code")
        if key not in self.code_names:
            name = f"Code{len(self.code_names)}"
            self.code_names[key] = name
            self.codes.append(
                cast(
                    circuit.CodeDefinition,
                    parse(_code_source(code, width, name)).definitions[0],
                )
            )
        return self.code_names[key]

    def add_gadget(
        self, source: list[str], event: _Decode
    ) -> tuple[int, int, _LocalContract | None]:
        key = repr(
            (source, id(event.invocation.gadget), event.invocation.call.arguments)
        )
        if key not in self.cached:
            contract = _local_contract(event)
            definition, checks = _local_definition(
                self.codes,
                f"GADGET Gadget{len(self.gadgets)} {{\n" + "\n".join(source) + "\n}",
                contract,
                event.invocation,
            )
            self.gadgets.append(definition)
            self.cached[key] = len(self.gadgets), checks, contract
        return self.cached[key]

    def add_discard(self, name: str) -> int:
        key = f"discard {name}"
        if key not in self.cached:
            width = next(code.n for code in self.codes if code.name == name)
            definition = circuit.GadgetDefinition(
                f"Discard{len(self.gadgets)}",
                [circuit.InputPort(name, list(range(width)))],
            )
            self.gadgets.append(definition)
            self.cached[key] = len(self.gadgets), 0, None
        return self.cached[key][0]

    def build(self) -> tuple[circuit.DeqFile, JitLibraryArtifacts]:
        source = circuit.DeqFile(definitions=[*self.codes, *self.gadgets])
        artifacts = build_jit_library_artifacts(source, jobs=1)
        return source, artifacts


def _local_source(
    trace: CircuitTrace,
    before: _Before,
    event: _Decode,
    noise: NoiseConfig | None,
    input_codes: Sequence[str],
    output_codes: Sequence[str],
) -> list[str]:
    qubits = dict.fromkeys(
        target for targets in before.qubits.values() for target in targets
    )
    for instruction in trace.instructions[before.position : event.position]:
        operands = (
            instruction[1:2]
            if instruction[0] == QirInstructionId.MZ
            else instruction[1:]
        )
        qubits.update(dict.fromkeys(cast(tuple[int, ...], operands)))
    for targets in event.qubits.values():
        qubits.update(dict.fromkeys(targets))
    for _, target, _ in trace.frames[before.frame_position : event.frame_position]:
        qubits[target] = None
    local = {target: index for index, target in enumerate(qubits)}
    source = [
        f"INPUT {name} "
        + " ".join(str(local[target]) for target in before.qubits[block])
        for block, name in zip(event.invocation.inputs, input_codes)
    ]
    source.extend(_body_source(trace, before, event, local, noise))
    source.extend(
        f"OUTPUT {name} "
        + " ".join(str(local[target]) for target in event.qubits[block])
        for block, name in zip(event.invocation.outputs, output_codes)
    )
    return source


def _connected_library(
    trace: CircuitTrace,
    noise: NoiseConfig | None,
) -> tuple[circuit.DeqFile, JitLibraryArtifacts, tuple[_Gadget, ...]]:
    builder = _LocalLibraryBuilder()
    instances: list[_Gadget] = []
    producers: dict[BlockReference, tuple[int, int, str]] = {}

    def discard(block: BlockReference) -> None:
        producer = producers.pop(block, None)
        if producer is None:
            return
        instance, port, name = producer
        gtype = builder.add_discard(name)
        instances.append(_Gadget(gtype, None, 0, 0, range(0), 0, ((instance, port),)))

    before: _Before | None = None
    result_offset = 0
    for event in trace.events:
        if isinstance(event, _Before):
            before = event
            continue
        if isinstance(event, _Discard):
            for block in event.blocks:
                discard(block)
            continue
        if before is None or before.invocation.id != event.invocation.id:
            raise ExecutionUnresolved("Gadget trace has no matching input boundary")
        input_codes = []
        connectors = []
        for block in event.invocation.inputs:
            if block not in producers:
                raise ExecutionUnresolved("Gadget input has no producing output port")
            producer, output_port, name = producers.pop(block)
            connectors.append((producer, output_port))
            input_codes.append(name)
        output_codes = [
            builder.add_code(
                event.invocation.gadget.outputs[port].code, len(event.qubits[block])
            )
            for port, block in enumerate(event.invocation.outputs)
        ]
        source = _local_source(trace, before, event, noise, input_codes, output_codes)
        gtype, checks, contract = builder.add_gadget(source, event)
        instances.append(
            _Gadget(
                gtype,
                contract,
                event.start,
                event.width,
                range(
                    result_offset,
                    result_offset + event.invocation.gadget.implements.observe_count,
                ),
                checks,
                tuple(connectors),
            )
        )
        result_offset += len(event.invocation.gadget.readouts)
        for port, (block, name) in enumerate(
            zip(event.invocation.outputs, output_codes)
        ):
            producers[block] = (len(instances), port, name)
    for block in tuple(producers):
        discard(block)
    source, artifacts = builder.build()
    return source, artifacts, tuple(instances)


def _unpack_readouts(
    replies: Sequence[coordinator.Readouts], widths: Sequence[int]
) -> list[np.ndarray]:
    if len(replies) != len(widths):
        raise RuntimeError("deq returned the wrong number of gadget replies")
    unpacked = []
    for gid, (reply, size) in enumerate(zip(replies, widths), 1):
        if (
            reply.gid != gid
            or reply.readouts.size != size
            or len(reply.readouts.data) != (size + 7) // 8
        ):
            raise RuntimeError("deq returned an invalid gadget readout record")
        unpacked.append(
            np.unpackbits(np.frombuffer(reply.readouts.data, dtype=np.uint8))[:size]
        )
    return unpacked


@dataclass(frozen=True)
class DeqBatch:
    trace: CircuitTrace
    readout_plan: _ReadoutPlan
    library: jit.JitLibrary
    noise_key: tuple[float, ...]
    composites: tuple[_Composite, ...]

    def run(
        self,
        shots: int,
        noise: NoiseConfig | None,
        *,
        seed: int,
        on_shot_failure: Literal["raise", "discard"] = "discard",
    ) -> list[list[OutputRecordValue]]:
        if _noise_key(noise) != self.noise_key:
            raise ValueError("Noise changed; prepare a new circuit-level deq batch")
        physical = self.trace.sample(shots, noise, seed=seed)
        with ThreadPoolExecutor(max_workers=1, thread_name_prefix="qdk-deq") as worker:
            return worker.submit(
                asyncio.run, self._decode(physical, seed, on_shot_failure)
            ).result()

    async def _decode(
        self,
        physical: Sequence[Sequence[Result]],
        seed: int,
        policy: Literal["raise", "discard"],
    ) -> list[list[OutputRecordValue]]:
        if not self.composites:
            outputs = []
            for value in self.trace.outputs:
                if isinstance(value, _Readout):
                    raise ExecutionUnresolved("Circuit has no producer for a readout")
                outputs.append(value)
            return [list(outputs) for _ in physical]
        records, flags = self._prepare_records(physical, policy)
        shot_count = len(records)
        if not shot_count:
            return []
        readout_widths = [gadget.readout_count for gadget in self.composites]
        weights = {
            kind.base.gtype: _model_size(kind) for kind in self.library.gadget_types
        }
        weight_per_shot = sum(weights[chunk.gtype] for chunk in self.composites)
        batch_size = max(1, min(256, 262144 // weight_per_shot))
        results = []
        async with Runtime(
            decoder="black-box-relay-bp",
            decoder_config={"parallel": 1, "seed": seed},
            coordinator="window",
            coordinator_config={"buffer_radius": 1, "lookahead_radius": 1},
            controller="jit",
        ) as runtime:
            service = runtime.jit_controller
            await service.load_library(self.library)
            # Bound live coordinator state while amortizing native async crossings.
            for start in range(0, shot_count, batch_size):
                stop = min(start + batch_size, shot_count)
                instructions = self._batch_instructions(stop - start)
                assigned = await service.batch_execute(instructions)
                if assigned != list(range(1, len(instructions) + 1)):
                    raise RuntimeError("deq returned unexpected circuit identifiers")
                replies = await service.batch_decode(
                    self._batch_outcomes(records[start:stop])
                )
                unpacked = _unpack_readouts(replies, readout_widths * (stop - start))
                results.extend(
                    self._reconstruct_outputs(unpacked, flags[start:stop], policy)
                )
                if stop < shot_count:
                    await service.reset(
                        reset_library=False, reset_decoder_service=False
                    )
        return results

    def _prepare_records(
        self, physical: Sequence[Sequence[Result]], policy: Literal["raise", "discard"]
    ) -> tuple[np.ndarray, np.ndarray]:
        records = np.asarray(
            [[value == Result.One for value in shot] for shot in physical], dtype=bool
        ).reshape(len(physical), self.trace.num_measurements)
        flags = np.zeros((len(physical), self.readout_plan.count), dtype=bool)
        for index, parity in self.readout_plan.flags:
            flags[:, index] = (
                np.logical_xor.reduce(records[:, parity.indices], axis=1)
                ^ parity.constant
            )
        if policy == "discard" and self.readout_plan.flags:
            keep = [
                shot
                for shot, row in enumerate(flags)
                if all(
                    selection.accepts(tuple(bool(row[index]) for index in indices))
                    for selection, indices in self.readout_plan.selections
                )
            ]
            records, flags = records[keep], flags[keep]
        if not len(records):
            return records, flags
        static = _static_flips(self.trace.instructions, self.trace.frames)
        for index in range(self.trace.num_measurements):
            if static >> index & 1:
                records[:, index] ^= True
        return records, flags

    def _batch_instructions(self, shots: int) -> list[bytes | jit.JitInstruction]:
        count = len(self.composites)
        return [
            jit.JitInstruction(
                gadget=model.Gadget(
                    gtype=gadget.gtype,
                    gid=shot * count + index + 1,
                    connectors=[
                        model.Gadget.Connector(gid=shot * count + producer, port=port)
                        for producer, port in gadget.connectors
                    ],
                )
            )
            for shot in range(shots)
            for index, gadget in enumerate(self.composites)
        ]

    def _batch_outcomes(
        self, records: np.ndarray
    ) -> list[bytes | coordinator.Outcomes]:
        count = len(self.composites)
        return [
            coordinator.Outcomes(
                gid=shot * count + index + 1,
                outcomes=util.BitVector(
                    size=len(gadget.measurements),
                    data=np.packbits(row[list(gadget.measurements)]).tobytes(),
                ),
            )
            for shot, row in enumerate(records)
            for index, gadget in enumerate(self.composites)
        ]

    def _reconstruct_outputs(
        self,
        unpacked: Sequence[np.ndarray],
        flags: np.ndarray,
        policy: Literal["raise", "discard"],
    ) -> list[list[OutputRecordValue]]:
        decoded = np.concatenate(unpacked).reshape(len(flags), -1)
        plan = self.readout_plan
        bits = flags.copy()
        bits[:, plan.decoded_destinations] = decoded[:, plan.decoded_sources]
        failed = np.any(decoded[:, plan.check_indices], axis=1)
        results = []
        for row, unexplained in zip(bits, failed):
            try:
                if unexplained:
                    raise ExecutionUnresolved(
                        "deq could not explain the circuit syndrome"
                    )
                for selection, indices in plan.selections:
                    selection.require(tuple(bool(row[index]) for index in indices))
                outcomes = [
                    Result.One if row[index] else Result.Zero for index in plan.sources
                ]
                results.append(
                    [
                        outcomes[value.index] if isinstance(value, _Readout) else value
                        for value in self.trace.outputs
                    ]
                )
            except (ExecutionRejected, ExecutionUnresolved, InconsistentParity):
                if policy == "raise":
                    raise
        return results
