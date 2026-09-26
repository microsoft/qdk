"""Compile a closed Clifford shot into a deq measurement/check/error model.

Encoding signs denote Pauli-frame corrections, not logical measurement values.
Input signs use the zero correction frame. Output sign equations determine
linear Pauli corrections, including random preparation syndromes. These and
authored frames are substituted into subsequent record expressions once, during
compilation. deq infers fault-induced flips across the entire physical trace.
Rejection flags are not error-corrected.
"""

from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from contextlib import closing
from dataclasses import dataclass
from itertools import product
from math import expm1, log1p
from typing import cast, Literal

import numpy as np
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
from .native_batch import (
    ReplayBatch,
    _Decode,
    _FrameMasks,
    _GATES,
    _Readout,
    _TABLE_WIDTHS,
    _native_noise,
    _static_flips,
)
from .protocols import DecoderSession, ExecutionRejected, ExecutionUnresolved
from .readout_equations import InconsistentParity, Parity, expression, prepare_frames
from .selection import Selection


@dataclass(frozen=True)
class _Parity:
    mask: int = 0
    constant: bool = False

    def __xor__(self, other: _Parity) -> _Parity:
        return _Parity(self.mask ^ other.mask, self.constant ^ other.constant)

    def flipped(self, fault: int) -> bool:
        return bool((self.mask & fault).bit_count() % 2)

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
        equations = _compile(trace)
        return CircuitDeqBatch(
            trace, equations, _library(trace, equations, noise), _noise_key(noise)
        )


@dataclass(frozen=True)
class _Equations:
    checks: tuple[_Parity, ...]
    readouts: tuple[_Parity, ...]
    flags: frozenset[int]
    selections: tuple[tuple[Selection, tuple[int, ...]], ...]
    sources: tuple[int, ...]


def _raw(
    parity: Parity,
    event: _Decode,
    records: Sequence[_Parity],
    alias_origin: int,
    output_signs: Mapping[tuple[int, str, int], int],
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
    event: _Decode, records: Sequence[_Parity]
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
        ^ _raw(expression(readout.equation), event, records, alias_origin, signs)
        for index, readout in enumerate(gadget.readouts)
    ]
    equations.extend(
        _raw(expression(check), event, records, alias_origin, signs)
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


def _compile(trace: ReplayBatch) -> _Equations:
    masks = _FrameMasks(trace.instructions)
    static = _static_flips(trace.instructions, trace.frames, masks)
    records = [
        _Parity(1 << index, bool(static >> index & 1))
        for index in range(trace.num_measurements)
    ]
    checks: list[_Parity] = []
    readouts: list[_Parity] = []
    flags: set[int] = set()
    selections = []
    offsets = {}
    for event_index, event in enumerate(trace.events):
        if not isinstance(event, _Decode):
            continue
        gadget = event.invocation.gadget
        count = len(gadget.readouts)
        observed, constraints, signs = _gadget_equations(event, records)
        offsets[event_index] = len(readouts)
        flag_indices = tuple(
            range(
                len(readouts) + gadget.implements.observe_count, len(readouts) + count
            )
        )
        if count != gadget.implements.observe_count + len(gadget.implements.flags):
            raise ExecutionUnresolved("Gadget readout equations have the wrong shape")
        flags.update(flag_indices)
        selections.append((event.selection, flag_indices))
        readouts.extend(observed)
        checks.extend(constraints)
        corrections, constraints = _output_frames(event, signs, len(records))
        checks.extend(constraints)
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
    return _Equations(
        tuple(dict.fromkeys(check for check in checks if check.mask or check.constant)),
        tuple(readouts),
        frozenset(flags),
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


def _faults(
    trace: ReplayBatch, noise: NoiseConfig | None
) -> Iterable[tuple[int, float]]:
    if noise is None:
        return
    masks = _FrameMasks(trace.instructions)
    channels: dict[tuple[str, int], list[tuple[str, float]]] = {}
    for position, instruction in enumerate(trace.instructions):
        opcode, *operands = instruction
        if opcode in (QirInstructionId.MZ, QirInstructionId.RESET):
            name, targets = "mresetz", cast(list[int], operands[:1])
        else:
            name = next(
                name for name, operation in _GATES.items() if operation == opcode
            )
            targets = cast(list[int], operands)
        key = (name, len(targets))
        if key not in channels:
            channels[key] = _channel(noise, *key)
        for axes, probability in channels[key]:
            effect = 0
            for target, axis in zip(targets, axes):
                if axis != "I":
                    effect ^= masks.mask(position + 1, target, axis.lower())
            if effect:
                yield effect, probability


def _noise_key(noise: NoiseConfig | None) -> tuple[float, ...]:
    return tuple(
        0.0 if noise is None else float(getattr(getattr(noise, name), "".join(axes)))
        for name, width in (*_TABLE_WIDTHS.items(), ("mresetz", 1))
        for axes in product("IXYZL", repeat=width)
        if axes != ("I",) * width
    )


def _library(
    trace: ReplayBatch, equations: _Equations, noise: NoiseConfig | None
) -> jit.JitLibrary:
    readout_count = len(equations.readouts)
    rows = (*equations.readouts, *equations.checks)
    constants = [row.constant for row in rows]
    errors = []
    for fault, probability in _faults(trace, noise):
        flipped = [
            index
            for index, row in enumerate(rows)
            if index not in equations.flags and row.flipped(fault)
        ]
        if probability > 0.5:
            for index in flipped:
                constants[index] ^= True
            probability = 1 - probability
        if probability and flipped:
            errors.append(
                jit.JitGadgetType.Error(
                    base=model.ErrorModelType.Error(
                        probability=probability,
                        readout_flips=flipped,
                    ),
                    finished_checks=[
                        index - readout_count
                        for index in flipped
                        if index >= readout_count
                    ],
                )
            )
    return jit.JitLibrary(
        gadget_types=[
            jit.JitGadgetType(
                base=model.GadgetType(
                    gtype=1,
                    name="qodec_circuit",
                    measurements=[
                        model.GadgetType.Measurement()
                        for _ in range(trace.num_measurements)
                    ],
                    readouts=[
                        model.GadgetType.Readout(measurement_indices=row.indices)
                        for row in rows
                    ],
                    correction_propagation=util.BitMatrix(rows=0, cols=1),
                    readout_propagation=util.BitMatrix(
                        rows=len(rows),
                        cols=1,
                        i=[index for index, value in enumerate(constants) if value],
                        j=[0 for value in constants if value],
                    ),
                    logical_correction=util.BitMatrix(rows=0, cols=len(rows)),
                    physical_correction=util.BitMatrix(
                        rows=0, cols=trace.num_measurements
                    ),
                ),
                finished_checks=[
                    jit.JitGadgetType.Check(
                        base=model.CheckModelType.Check(
                            naturally_flipped=constants[readout_count + position],
                        ),
                        measurements=[
                            jit.JitGadgetType.PresentMeasurement(
                                measurement_index=index
                            )
                            for index in check.indices
                        ],
                    )
                    for position, check in enumerate(equations.checks)
                ],
                errors=errors,
            )
        ]
    )


@dataclass(frozen=True)
class CircuitDeqBatch:
    trace: ReplayBatch
    equations: _Equations
    library: jit.JitLibrary
    noise_key: tuple[float, ...]

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
        results = []
        async with Runtime(
            decoder="black-box-relay-bp",
            decoder_config={"parallel": 1, "seed": seed},
            coordinator="monolithic",
            controller="jit",
        ) as runtime:
            service = runtime.jit_controller
            await service.load_library(library)
            # Bound live coordinator state while amortizing native async crossings.
            for start in range(0, len(physical), 256):
                chunk = physical[start : start + 256]
                gids = list(range(start + 1, start + len(chunk) + 1))
                assigned = await service.batch_execute(
                    [
                        jit.JitInstruction(gadget=model.Gadget(gtype=1, gid=gid))
                        for gid in gids
                    ]
                )
                if assigned != gids:
                    raise RuntimeError("deq returned unexpected circuit identifiers")
                replies = await service.batch_decode(
                    [
                        coordinator.Outcomes(
                            gid=gid,
                            outcomes=util.BitVector(
                                size=len(records),
                                data=np.packbits(
                                    np.asarray(
                                        [value == Result.One for value in records],
                                        dtype=bool,
                                    )
                                ).tobytes(),
                            ),
                        )
                        for gid, records in zip(gids, chunk)
                    ]
                )
                if len(replies) != len(chunk):
                    raise RuntimeError("deq returned the wrong number of shots")
                for gid, reply in zip(gids, replies):
                    try:
                        results.append(self._readouts(reply, gid))
                    except (ExecutionRejected, ExecutionUnresolved, InconsistentParity):
                        if policy == "raise":
                            raise
        return results

    def _readouts(
        self, reply: coordinator.Readouts, gid: int
    ) -> list[OutputRecordValue]:
        count = len(self.equations.readouts)
        size = count + len(self.equations.checks)
        if (
            reply.gid != gid
            or reply.readouts.size != size
            or len(reply.readouts.data) != (size + 7) // 8
        ):
            raise RuntimeError("deq returned an invalid circuit readout record")
        bits = np.unpackbits(np.frombuffer(reply.readouts.data, dtype=np.uint8))[:size]
        if any(bits[count:]):
            raise ExecutionUnresolved("deq could not explain the circuit syndrome")
        for selection, indices in self.equations.selections:
            selection.require(tuple(bool(bits[index]) for index in indices))
        outcomes = [
            Result.One if bits[index] else Result.Zero
            for index in self.equations.sources
        ]
        return [
            outcomes[value.index] if isinstance(value, _Readout) else value
            for value in self.trace.outputs
        ]
