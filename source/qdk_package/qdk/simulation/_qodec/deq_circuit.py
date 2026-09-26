"""Decode bounded groups of connected Clifford gadgets with deq windows.

Encoding signs denote Pauli-frame corrections, not logical measurement values.
Qodec signs name observables; deq logical targets name correction Paulis.
deq owns code-port connections, circuit fault propagation, and decoding.
Raw rejection flags are compiled separately: unlike logical readouts, they must
include authored frame changes but must not receive inferred error corrections.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from contextlib import closing
from dataclasses import dataclass, replace
from itertools import product
from math import expm1, log1p
from typing import cast, Literal

import numpy as np
from qodec import Code
from deq.circuit import model as circuit  # pyright: ignore[reportMissingImports]
from deq.circuit.parser import parse  # pyright: ignore[reportMissingImports]
from deq.transpiler.check_plugins import (  # pyright: ignore[reportMissingImports]
    resolve_gadget_checks,
)
from deq.transpiler.jit_library_builder import (  # pyright: ignore[reportMissingImports]
    build_jit_library,
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
from ..._native import QirInstruction, QirInstructionId, run_clifford
from ._interpreter import OutputRecordValue
from .clifford_semantics import pauli
from .deq_decoding import _DeqTransport
from .deq_composition import _Composite, _compose_gadgets, _model_size
from .native_batch import (
    ReplayBatch,
    _Before,
    _Decode,
    _Discard,
    _FrameMasks,
    _GATES,
    _Readout,
    _TABLE_WIDTHS,
    _native_noise,
    _static_flips,
)
from .protocols import (
    BlockReference,
    DecoderSession,
    ExecutionRejected,
    ExecutionUnresolved,
)
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


class CircuitDeqModel:
    def __call__(self, seed: int | None = None) -> DecoderSession:
        raise NotImplementedError("Circuit-level deq requires a complete native trace")

    def prepare_circuit(
        self, trace: ReplayBatch, noise: NoiseConfig | None, /
    ) -> CircuitDeqBatch:
        readouts = _readout_plan(trace)
        library, gadgets = _connected_library(trace, noise)
        library, composites = _compose_gadgets(library, gadgets)
        return CircuitDeqBatch(
            trace, readouts, library, _noise_key(noise), gadgets, composites
        )


@dataclass(frozen=True)
class _ReadoutPlan:
    count: int
    flags: tuple[tuple[int, _Parity], ...]
    selections: tuple[tuple[Selection, tuple[int, ...]], ...]
    sources: tuple[int, ...]


def _raw(
    parity: Parity,
    event: _Decode,
    records: Sequence[_Parity],
    alias_origin: int,
    output_signs: Mapping[tuple[int, str, int], int],
    input_signs: Mapping[tuple[int, str, int], int] | None = None,
) -> _Parity:
    result = _Parity(constant=parity.constant)
    gadget = event.invocation.gadget
    for variable in parity.variables:
        kind, boundary, entry, basis, index = cast(
            tuple[str, str, int, str, int], variable
        )
        if kind == "circuit_readout":
            if not 0 <= index < event.width:
                raise ExecutionUnresolved("Circuit readout reference is unavailable")
            result ^= records[event.start + index]
        elif kind == "readout":
            if not 0 <= index < len(gadget.readouts):
                raise ExecutionUnresolved("Gadget readout reference is unavailable")
            result ^= _Parity(1 << (alias_origin + index))
        else:
            encodings = gadget.inputs if boundary == "in" else gadget.outputs
            if not 0 <= entry < len(encodings) or not 0 <= index < len(
                getattr(encodings[entry].code, basis)
            ):
                raise ExecutionUnresolved("Encoding sign reference is unavailable")
            if boundary == "out":
                result ^= _Parity(1 << output_signs[entry, basis, index])
            elif input_signs is not None:
                result ^= _Parity(1 << input_signs[entry, basis, index])
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


def _gadget_equations(
    event: _Decode,
    records: Sequence[_Parity],
    input_signs: Mapping[tuple[int, str, int], int] | None = None,
) -> tuple[list[_Parity], list[_Parity], dict[tuple[int, str, int], _Parity]]:
    gadget = event.invocation.gadget
    keys = [
        (entry, basis, index)
        for entry, encoding in enumerate(gadget.outputs)
        for basis in ("stabilizers", "x", "z")
        for index in range(len(getattr(encoding.code, basis)))
    ]
    origin = len(records)
    signs = {key: origin + index for index, key in enumerate(keys)}
    alias_origin = origin + len(keys)
    equations = [
        _Parity(1 << (alias_origin + index))
        ^ _raw(
            expression(readout.equation),
            event,
            records,
            alias_origin,
            signs,
            input_signs,
        )
        for index, readout in enumerate(gadget.readouts)
    ]
    equations.extend(
        _raw(expression(check), event, records, alias_origin, signs, input_signs)
        for check in gadget.checks
    )
    resolved, checks = _eliminate_aliases(
        equations, origin, len(keys) + len(gadget.readouts), free_count=len(keys)
    )
    return resolved[len(keys) :], checks, dict(zip(keys, resolved[: len(keys)]))


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


def _flag_parities(trace: ReplayBatch) -> tuple[tuple[int, _Parity], ...]:
    masks = _FrameMasks(trace.instructions)
    static = _static_flips(trace.instructions, trace.frames, masks)
    records = [
        _Parity(1 << index, bool(static >> index & 1))
        for index in range(trace.num_measurements)
    ]
    flags = []
    offset = 0
    for event in trace.events:
        if not isinstance(event, _Decode):
            continue
        gadget = event.invocation.gadget
        count = len(gadget.readouts)
        observed, _, signs = _gadget_equations(event, records)
        flags.extend(
            (offset + index, observed[index])
            for index in range(gadget.implements.observe_count, count)
        )
        offset += count
        corrections, _ = _output_frames(event, signs, len(records))
        for target, axis, value in corrections:
            _apply_frame(records, masks.mask(event.position, target, axis), value)
        for frame in prepare_frames(gadget):
            value = _raw(frame.parity, event, records, len(records), {})
            block = event.invocation.outputs[frame.output]
            qubits = event.qubits[block]
            code = gadget.outputs[frame.output].code
            operator = pauli(
                getattr(code, "x" if frame.basis == "z" else "z")[frame.logical],
                len(qubits),
            )
            affected = 0
            for target in operator.support:
                affected ^= masks.mask(
                    event.position, qubits[target], operator[target].lower()
                )
            _apply_frame(records, affected, value)
    return tuple(flags)


def _readout_plan(trace: ReplayBatch) -> _ReadoutPlan:
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
    return _ReadoutPlan(
        count,
        _flag_parities(trace) if any(indices for _, indices in selections) else (),
        tuple(selections),
        tuple(offsets[event] + index for event, index in trace.sources),
    )


def _channel(noise: NoiseConfig, name: str, width: int) -> list[tuple[str, float]]:
    table = getattr(noise, name)
    if any(
        getattr(table, "".join(axes))
        for axes in product("IXYZL", repeat=width)
        if "L" in axes
    ):
        raise NotImplementedError("Circuit-level deq does not support loss")
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
        f"Circuit-level deq requires a single Pauli mechanism or representable "
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
    event: int | None
    start: int
    width: int
    readouts: int
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
    trace: ReplayBatch,
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
    index: int, event: _Decode
) -> (
    circuit.InputVirtualTarget
    | circuit.PhysicalMeasurementTarget
    | circuit.OutputVirtualTarget
):
    for port, encoding in enumerate(event.invocation.gadget.inputs):
        width = len(encoding.code.stabilizers)
        if index < width:
            return circuit.InputVirtualTarget(port, index)
        index -= width
    if index < event.width:
        return circuit.PhysicalMeasurementTarget(index)
    index -= event.width
    for port, encoding in enumerate(event.invocation.gadget.outputs):
        width = len(encoding.code.stabilizers)
        if index < width:
            return circuit.OutputVirtualTarget(port, index)
        index -= width
    raise ExecutionUnresolved("Encoding sign reference is unavailable")


@dataclass(frozen=True)
class _LocalContract:
    readouts: list[_Parity]
    checks: list[_Parity]
    signs: dict[tuple[int, str, int], _Parity]
    inputs: tuple[tuple[int, str, int], ...]
    specified: frozenset[tuple[int, str, int]]


def _local_contract(event: _Decode) -> _LocalContract:
    gadget = event.invocation.gadget
    inputs = tuple(
        (port, basis, index)
        for port, encoding in enumerate(gadget.inputs)
        for basis in ("stabilizers", "x", "z")
        for index in range(len(getattr(encoding.code, basis)))
    )
    readouts, checks, signs = _gadget_equations(
        replace(event, start=0),
        [_Parity(1 << index) for index in range(event.width + len(inputs))],
        {key: event.width + index for index, key in enumerate(inputs)},
    )
    specified = set()
    aliases = set()
    pending = [expression(check) for check in gadget.checks]
    while pending:
        for variable in pending.pop().variables:
            kind, boundary, entry, basis, index = cast(
                tuple[str, str, int, str, int], variable
            )
            if kind == "encoding" and boundary == "out":
                specified.add((entry, basis, index))
            elif kind == "readout" and index not in aliases:
                aliases.add(index)
                pending.append(expression(gadget.readouts[index].equation))
    return _LocalContract(readouts, checks, signs, inputs, frozenset(specified))


def _check_row(row: _Parity, event: _Decode, contract: _LocalContract) -> _Parity:
    inputs = event.invocation.gadget.inputs
    input_count = sum(len(encoding.code.stabilizers) for encoding in inputs)
    result = _Parity(constant=row.constant)
    for index in row.indices:
        if index < event.width:
            result ^= _Parity(1 << (input_count + index))
        else:
            port, basis, position = contract.inputs[index - event.width]
            if basis != "stabilizers":
                raise NotImplementedError(
                    "Circuit-level deq cannot use an input logical sign as a detection check"
                )
            offset = sum(len(encoding.code.stabilizers) for encoding in inputs[:port])
            result ^= _Parity(1 << (offset + position))
    return result


def _manual_checks(
    event: _Decode,
    inferred: Sequence[tuple[frozenset[int], bool]],
    contract: _LocalContract,
) -> tuple[list[_Parity], list[_Parity]]:
    """Complete only missing port propagation; never audit authored checks."""
    gadget = event.invocation.gadget
    input_count = sum(len(encoding.code.stabilizers) for encoding in gadget.inputs)
    origin = input_count + event.width
    output_count = sum(len(encoding.code.stabilizers) for encoding in gadget.outputs)
    authored = [_check_row(row, event, contract) for row in contract.checks]
    for (port, basis, index), value in contract.signs.items():
        if basis != "stabilizers" or (port, basis, index) not in contract.specified:
            continue
        offset = sum(
            len(encoding.code.stabilizers) for encoding in gadget.outputs[:port]
        )
        authored.append(
            _Parity(1 << (origin + offset + index)) ^ _check_row(value, event, contract)
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
    event: _Decode,
) -> tuple[circuit.GadgetDefinition, int, list[tuple[int, int]]]:
    parsed = parse(source)
    gadget = cast(circuit.GadgetDefinition, parsed.definitions[-1])
    codes = {definition.name: definition for definition in definitions}
    inferred = resolve_gadget_checks(gadget, codes)
    contract = _local_contract(event)
    finished, unfinished = _manual_checks(event, inferred.unfinished, contract)
    gadget.decorators.extend(
        cast(
            circuit.GadgetDefinition,
            parse('@CHECKS("manual", verify=0)\nGADGET Checks {}').definitions[0],
        ).decorators
    )
    for row in (*finished, *unfinished):
        gadget.body.append(
            circuit.CheckStatement(
                targets=[_measurement_target(index, event) for index in row.indices],
                flip=row.constant,
            )
        )
    gadget.body.extend(_logical_statements(event, contract))
    logical_count = event.invocation.gadget.implements.observe_count
    for row in contract.readouts[:logical_count]:
        gadget.body.append(
            circuit.ReadoutStatement(
                targets=[
                    circuit.PhysicalMeasurementTarget(index)
                    for index in row.indices
                    if index < event.width
                ],
                flip=row.constant,
            )
        )
    input_count = sum(
        len(encoding.code.stabilizers) for encoding in event.invocation.gadget.inputs
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
    constants = []
    offset = 0
    for entry, encoding in enumerate(event.invocation.gadget.outputs):
        for basis in ("x", "z"):
            for index in range(len(getattr(encoding.code, basis))):
                key = (entry, basis, index)
                value = contract.signs[key]
                if value.constant:
                    constants.append((offset + 2 * index + (basis == "z"), 1))
        offset += 2 * len(encoding.code.x) + len(encoding.code.stabilizers)
    for frame in prepare_frames(event.invocation.gadget):
        axis = "X" if frame.basis == "z" else "Z"
        for variable in frame.parity.variables:
            kind, _, _, _, index = cast(tuple[str, str, int, str, int], variable)
            if kind != "circuit_readout":
                raise ExecutionUnresolved("Frame reference is unavailable")
            statement = parse(
                f"GADGET Frame {{ CONDITIONAL M{index} OUT{frame.output}.L{axis}{frame.logical} }}"
            )
            gadget.body.extend(
                cast(circuit.GadgetDefinition, statement.definitions[0]).body
            )
        if frame.parity.constant:
            offset = sum(
                2 * len(encoding.code.x) + len(encoding.code.stabilizers)
                for encoding in event.invocation.gadget.outputs[: frame.output]
            )
            constants.append((offset + 2 * frame.logical + (frame.basis == "z"), 1))
    return gadget, len(finished), constants


def _logical_statements(
    event: _Decode, contract: _LocalContract
) -> list[circuit.GadgetStatement]:
    statements = []
    for target, value in contract.signs.items():
        port, basis, logical = target
        if target not in contract.specified or basis == "stabilizers":
            continue
        terms = []
        for index in value.indices:
            if index < event.width:
                terms.append(f"M{index}")
            else:
                entry, input_basis, position = contract.inputs[index - event.width]
                axis = "X" if input_basis == "z" else "Z"
                terms.append(
                    f"IN{entry}.DS{position}"
                    if input_basis == "stabilizers"
                    else f"IN{entry}.L{axis}{position}"
                )
        if value.constant:
            terms.append("FLIP")
        axis = "X" if basis == "z" else "Z"
        statement = parse(
            f"GADGET Frame {{ PROPAGATE OUT{port}.L{axis}{logical} FROM "
            + " ".join(terms)
            + " }"
        )
        statements.extend(cast(circuit.GadgetDefinition, statement.definitions[0]).body)
    return statements


def _connected_library(
    trace: ReplayBatch, noise: NoiseConfig | None
) -> tuple[jit.JitLibrary, tuple[_Gadget, ...]]:
    definitions: list[circuit.CodeDefinition] = []
    code_names: dict[str, str] = {}
    gadgets: list[circuit.GadgetDefinition] = []
    instances: list[_Gadget] = []
    constants: list[list[tuple[int, int]]] = []
    producers: dict[BlockReference, tuple[int, int, str]] = {}
    cached: dict[str, tuple[int, int]] = {}

    def code_name(code: Code, width: int) -> str:
        key = _code_source(code, width, "Code")
        if key not in code_names:
            name = f"Code{len(code_names)}"
            code_names[key] = name
            definitions.append(
                cast(
                    circuit.CodeDefinition,
                    parse(_code_source(code, width, name)).definitions[0],
                )
            )
        return code_names[key]

    def discard(block: BlockReference) -> None:
        producer = producers.pop(block, None)
        if producer is None:
            return
        instance, port, name = producer
        key = f"discard {name}"
        if key not in cached:
            width = next(code.n for code in definitions if code.name == name)
            definition = cast(
                circuit.GadgetDefinition,
                parse(
                    f"GADGET Discard{len(gadgets)} {{ INPUT {name} "
                    + " ".join(map(str, range(width)))
                    + " }"
                ).definitions[0],
            )
            gadgets.append(definition)
            constants.append([])
            cached[key] = len(gadgets), 0
        gtype, _ = cached[key]
        instances.append(_Gadget(gtype, None, 0, 0, 0, 0, ((instance, port),)))

    before: _Before | None = None
    for event_index, event in enumerate(trace.events):
        if isinstance(event, _Before):
            before = event
            continue
        if isinstance(event, _Discard):
            for block in event.blocks:
                discard(block)
            continue
        if before is None or before.invocation.id != event.invocation.id:
            raise ExecutionUnresolved("Gadget trace has no matching input boundary")
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
        source = []
        connectors = []
        for port, block in enumerate(event.invocation.inputs):
            if block not in producers:
                raise ExecutionUnresolved("Gadget input has no producing output port")
            producer, output_port, name = producers.pop(block)
            connectors.append((producer, output_port))
            source.append(
                f"INPUT {name} "
                + " ".join(str(local[target]) for target in before.qubits[block])
            )
        source.extend(_body_source(trace, before, event, local, noise))
        output_codes = []
        for port, block in enumerate(event.invocation.outputs):
            encoding = event.invocation.gadget.outputs[port]
            name = code_name(encoding.code, len(event.qubits[block]))
            output_codes.append(name)
            source.append(
                f"OUTPUT {name} "
                + " ".join(str(local[target]) for target in event.qubits[block])
            )
        key = repr((source, id(event.invocation.gadget)))
        if key not in cached:
            definition, checks, flips = _local_definition(
                definitions,
                f"GADGET Gadget{len(gadgets)} {{\n" + "\n".join(source) + "\n}",
                event,
            )
            gadgets.append(definition)
            constants.append(flips)
            cached[key] = len(gadgets), checks
        gtype, checks = cached[key]
        instances.append(
            _Gadget(
                gtype,
                event_index,
                event.start,
                event.width,
                event.invocation.gadget.implements.observe_count,
                checks,
                tuple(connectors),
            )
        )
        for port, (block, name) in enumerate(
            zip(event.invocation.outputs, output_codes)
        ):
            producers[block] = (len(instances), port, name)
    for block in tuple(producers):
        discard(block)
    library = build_jit_library(
        circuit.DeqFile(definitions=[*definitions, *gadgets]), jobs=1
    )
    ports = {port.base.ptype: port for port in library.port_types}
    for gadget, flips in zip(library.gadget_types, constants):
        logical_rows = []
        offset = 0
        for output in gadget.base.outputs:
            port = ports[output.ptype]
            logical_rows.extend(range(offset, offset + 2 * port.k))
            offset += len(port.base.observables)
        # Qodec executes intended logical Paulis; they are not corrections.
        _set_affine(gadget.base.correction_propagation, [], clear=logical_rows)
        _set_affine(gadget.base.correction_propagation, [row for row, _ in flips])
        retained = []
        for error in gadget.errors:
            if error.base.probability > 0.5:
                _set_affine(gadget.base.correction_propagation, error.base.residual)
                _set_affine(gadget.base.readout_propagation, error.base.readout_flips)
                for index in error.finished_checks:
                    check = gadget.finished_checks[index].base
                    check.naturally_flipped ^= True
                for index in error.unfinished_checks:
                    check = gadget.unfinished_checks[index].base
                    check.naturally_flipped ^= True
                error.base.probability = 1 - error.base.probability
            if error.base.probability:
                retained.append(error)
        del gadget.errors[:]
        gadget.errors.extend(retained)
    return library, tuple(instances)


def _set_affine(
    matrix: util.BitMatrix, flips: Iterable[int], *, clear: Iterable[int] = ()
) -> None:
    entries = set(zip(matrix.i, matrix.j))
    column = matrix.cols - 1
    entries.difference_update((row, column) for row in clear)
    for row in flips:
        entries.symmetric_difference_update({(row, column)})
    del matrix.i[:]
    del matrix.j[:]
    for row, column in sorted(entries):
        matrix.i.append(row)
        matrix.j.append(column)


@dataclass(frozen=True)
class CircuitDeqBatch:
    trace: ReplayBatch
    readout_plan: _ReadoutPlan
    library: jit.JitLibrary
    noise_key: tuple[float, ...]
    gadgets: tuple[_Gadget, ...]
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
        physical = cast(
            list[list[Result]],
            run_clifford(
                cast(list[QirInstruction], list(self.trace.instructions)),
                self.trace.num_qubits,
                self.trace.num_measurements,
                shots,
                _native_noise(noise),
                seed,
            ),
        )
        with closing(_DeqTransport()) as transport:
            return transport.run(
                self._decode(self.library, physical, seed, on_shot_failure)
            )

    async def _decode(
        self,
        library: jit.JitLibrary,
        physical: Sequence[Sequence[Result]],
        seed: int,
        policy: Literal["raise", "discard"],
    ) -> list[list[OutputRecordValue]]:
        if not self.gadgets:
            outputs = []
            for value in self.trace.outputs:
                if isinstance(value, _Readout):
                    raise ExecutionUnresolved("Circuit has no producer for a readout")
                outputs.append(value)
            return [list(outputs) for _ in physical]
        results = []
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
        shot_count = len(records)
        if not shot_count:
            return []
        static = _static_flips(self.trace.instructions, self.trace.frames)
        for index in range(self.trace.num_measurements):
            if static >> index & 1:
                records[:, index] ^= True
        count = len(self.composites)
        widths = {
            kind.base.gtype: len(kind.base.readouts) for kind in library.gadget_types
        }
        weights = {kind.base.gtype: _model_size(kind) for kind in library.gadget_types}
        weight_per_shot = sum(weights[chunk.gtype] for chunk in self.composites)
        batch_size = max(1, min(256, 262144 // weight_per_shot))
        async with Runtime(
            decoder="black-box-relay-bp",
            decoder_config={"parallel": 1, "seed": seed},
            coordinator="window",
            coordinator_config={"buffer_radius": 1, "lookahead_radius": 1},
            controller="jit",
        ) as runtime:
            service = runtime.jit_controller
            await service.load_library(library)
            # Bound live coordinator state while amortizing native async crossings.
            for start in range(0, shot_count, batch_size):
                stop = min(start + batch_size, shot_count)
                gids = list(range(1, (stop - start) * count + 1))
                assigned = await service.batch_execute(
                    [
                        jit.JitInstruction(
                            gadget=model.Gadget(
                                gtype=gadget.gtype,
                                gid=shot * count + index + 1,
                                connectors=[
                                    model.Gadget.Connector(
                                        gid=shot * count + producer, port=port
                                    )
                                    for producer, port in gadget.connectors
                                ],
                            )
                        )
                        for shot in range(stop - start)
                        for index, gadget in enumerate(self.composites)
                    ]
                )
                if assigned != gids:
                    raise RuntimeError("deq returned unexpected circuit identifiers")
                replies = await service.batch_decode(
                    [
                        coordinator.Outcomes(
                            gid=(shot - start) * count + index + 1,
                            outcomes=util.BitVector(
                                size=len(gadget.measurements),
                                data=np.packbits(
                                    records[shot, list(gadget.measurements)]
                                ).tobytes(),
                            ),
                        )
                        for shot in range(start, stop)
                        for index, gadget in enumerate(self.composites)
                    ]
                )
                if len(replies) != len(gids):
                    raise RuntimeError(
                        "deq returned the wrong number of gadget replies"
                    )
                unpacked = []
                for index, (gid, reply) in enumerate(zip(gids, replies)):
                    size = widths[self.composites[index % count].gtype]
                    if (
                        reply.gid != gid
                        or reply.readouts.size != size
                        or len(reply.readouts.data) != (size + 7) // 8
                    ):
                        raise RuntimeError(
                            "deq returned an invalid gadget readout record"
                        )
                    unpacked.append(
                        np.unpackbits(
                            np.frombuffer(reply.readouts.data, dtype=np.uint8)
                        )[:size]
                    )
                for shot in range(start, stop):
                    offset = (shot - start) * count
                    original_readouts = {
                        original: unpacked[offset + index][list(order)]
                        for index, gadget in enumerate(self.composites)
                        for original, order in gadget.readouts
                    }
                    try:
                        results.append(
                            self._readouts(
                                [
                                    original_readouts[index]
                                    for index in range(len(self.gadgets))
                                ],
                                flags[shot],
                            )
                        )
                    except (ExecutionRejected, ExecutionUnresolved, InconsistentParity):
                        if policy == "raise":
                            raise
                if stop < shot_count:
                    await service.reset(
                        reset_library=False, reset_decoder_service=False
                    )
        return results

    def _readouts(
        self, replies: Sequence[np.ndarray], flags: np.ndarray
    ) -> list[OutputRecordValue]:
        bits = flags.copy()
        offset = 0
        for gadget, reply in zip(self.gadgets, replies):
            if any(reply[gadget.readouts :]):
                raise ExecutionUnresolved("deq could not explain the circuit syndrome")
            if gadget.event is not None:
                bits[offset : offset + gadget.readouts] = reply[: gadget.readouts]
                event = cast(_Decode, self.trace.events[gadget.event])
                offset += len(event.invocation.gadget.readouts)
        for selection, indices in self.readout_plan.selections:
            selection.require(tuple(bool(bits[index]) for index in indices))
        outcomes = [
            Result.One if bits[index] else Result.Zero
            for index in self.readout_plan.sources
        ]
        return [
            outcomes[value.index] if isinstance(value, _Readout) else value
            for value in self.trace.outputs
        ]
