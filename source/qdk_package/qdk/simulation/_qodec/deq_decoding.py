"""Connect compiled deq gadgets and decode native physical samples.

Gadget conversion owns equations, noise models, and frame propagation rules.
This module owns instances, sample/result routing, and bounded deq execution.
"""

from __future__ import annotations

import asyncio
from collections.abc import Callable, Iterator, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, replace
from functools import partial
from typing import Literal, cast

import numpy as np
from qodec import Layer
from deq.circuit import model as circuit  # pyright: ignore[reportMissingImports]
from deq.circuit.parser import parse  # pyright: ignore[reportMissingImports]
from deq.proto import (  # pyright: ignore[reportMissingImports]
    coordinator_pb2 as coordinator,
    deq_bin_pb2 as model,
    deq_jit_pb2 as jit,
    util_pb2 as util,
)
from deq.runtime import Runtime  # pyright: ignore[reportMissingImports]
from deq.transpiler.compose_builder import (  # pyright: ignore[reportMissingImports]
    transpile_compose_jit_gadget_type,
)
from deq.transpiler.jit_library_builder import (  # pyright: ignore[reportMissingImports]
    JitLibraryArtifacts,
)
from deq.transpiler.loss.model_none import (  # pyright: ignore[reportMissingImports]
    NoLossModel,
)

from qdk import Result
from .. import NoiseConfig
from ..._adaptive_pass import AdaptiveProgram
from ._interpreter import OutputRecordValue
from .deq_conversion import _GadgetModel, _LibraryBuilder, _noise_key
from .native_batch import (
    CircuitTrace,
    _CircuitCall,
    _CircuitRecorder,
    _Discard,
    _Readout,
    _trace_circuit,
)
from .executor import ExecutionPipelineFactory
from .layer_runtime import LayerPlan
from .protocols import (
    BlockReference,
    DecoderSession,
    ExecutionRejected,
    ExecutionUnresolved,
)
from .selection import Selection

_COMPOSITE_SIZE = 1024


def _default_runtime(seed: int, *, forced_gap: bool = False) -> Runtime:
    coordinator_config = {"buffer_radius": 1, "lookahead_radius": 1}
    if forced_gap:
        coordinator_config["forced_gap"] = True
    return Runtime(
        decoder="black-box-relay-bp",
        decoder_config={"parallel": 1, "seed": seed},
        coordinator="window",
        coordinator_config=coordinator_config,
        controller="jit",
    )


@dataclass(frozen=True)
class _Instance:
    """Map one compiled gadget to physical records and QDK readout slots."""

    model: _GadgetModel
    measurements: range
    destinations: range
    connectors: tuple[tuple[int, int], ...]


@dataclass(frozen=True)
class _Flag:
    destination: int
    measurements: tuple[int, ...]
    constant: bool


@dataclass(frozen=True)
class _ReadoutPlan:
    """Route decoded bits and local flags directly to program outputs."""

    count: int
    flags: tuple[_Flag, ...]
    selections: tuple[tuple[Selection, tuple[int, ...]], ...]
    outputs: tuple[OutputRecordValue | _Readout, ...]
    decoded_destinations: tuple[int, ...]

    @property
    def returned_logical_positions(self) -> tuple[int, ...]:
        returned = {
            value.index for value in self.outputs if isinstance(value, _Readout)
        }
        return tuple(
            index
            for index, destination in enumerate(self.decoded_destinations)
            if destination in returned
        )


class DeqModel:
    def __init__(
        self,
        layer: Layer,
        *,
        runtime_factory: Callable[[int], Runtime] | None = None,
        max_readout_score: float | None = None,
    ) -> None:
        self.layer_plan = LayerPlan(layer)
        self.runtime_factory = (
            partial(_default_runtime, forced_gap=max_readout_score is not None)
            if runtime_factory is None
            else runtime_factory
        )
        self.max_readout_score = max_readout_score

    def record_circuit(
        self,
        program: AdaptiveProgram,
        factory: ExecutionPipelineFactory[AdaptiveProgram, list[OutputRecordValue]],
    ) -> CircuitTrace | None:
        return _trace_circuit(
            program, factory, self.layer_plan, recorder=_CircuitRecorder
        )

    def __call__(self, seed: int | None = None) -> DecoderSession:
        raise NotImplementedError("deq requires a complete native trace")

    def prepare_circuit(
        self, trace: CircuitTrace, noise: NoiseConfig | None, /
    ) -> DeqBatch:
        builder = _LibraryBuilder(self.layer_plan, noise)
        instances, readouts = _connect_gadgets(trace, builder)
        if (
            self.max_readout_score is not None
            and not readouts.returned_logical_positions
        ):
            raise ValueError("max_readout_score requires a returned logical readout")
        source, artifacts = builder.build()
        library, composites = _compose_gadgets(source, artifacts, instances)
        samples = replace(trace, events=(), sources=(), outputs=())
        return DeqBatch(
            samples,
            readouts,
            library,
            _noise_key(noise),
            composites,
            self.runtime_factory,
            self.max_readout_score,
        )


def _connect_gadgets(
    trace: CircuitTrace, builder: _LibraryBuilder
) -> tuple[tuple[_Instance, ...], _ReadoutPlan]:
    instances = []
    producers: dict[BlockReference, tuple[int, int, str]] = {}
    offsets = {}
    flags = []
    selections = []
    count = 0

    def discard(block: BlockReference) -> None:
        producer = producers.pop(block, None)
        if producer is not None:
            instance, port, name = producer
            instances.append(
                _Instance(
                    builder.add_discard(name), range(0), range(0), ((instance, port),)
                )
            )

    for event_index, event in enumerate(trace.events):
        if isinstance(event, _Discard):
            for block in event.blocks:
                discard(block)
        elif isinstance(event, _CircuitCall):
            invocation = event.invocation
            compiled = builder.add_gadget(
                invocation.call.mnemonic, invocation.call.arguments
            )
            if event.width != compiled.measurement_count:
                raise ExecutionUnresolved(
                    "Gadget model and samples have different measurement counts"
                )
            connectors = []
            for block, code in zip(invocation.inputs, compiled.inputs, strict=True):
                if block not in producers:
                    raise ExecutionUnresolved(
                        "Gadget input has no producing output port"
                    )
                instance, port, produced_code = producers.pop(block)
                if produced_code != code:
                    raise ExecutionUnresolved("Connected gadget codes do not match")
                connectors.append((instance, port))
            destinations = range(count, count + compiled.readout_count)
            instances.append(
                _Instance(
                    compiled,
                    range(event.start, event.start + event.width),
                    destinations,
                    tuple(connectors),
                )
            )
            for port, (block, code) in enumerate(
                zip(invocation.outputs, compiled.outputs, strict=True)
            ):
                producers[block] = len(instances), port, code
            indices = tuple(
                range(destinations.stop, destinations.stop + len(compiled.flags))
            )
            flags.extend(
                _Flag(
                    index,
                    tuple(event.start + bit for bit in flag.indices),
                    flag.constant,
                )
                for index, flag in zip(indices, compiled.flags)
            )
            selections.append((event.selection, indices))
            offsets[event_index] = count
            count += compiled.readout_count + len(compiled.flags)
    for block in tuple(producers):
        discard(block)
    sources = [offsets[event] + index for event, index in trace.sources]
    return tuple(instances), _ReadoutPlan(
        count,
        tuple(flags),
        tuple(selections),
        tuple(
            _Readout(sources[value.index]) if isinstance(value, _Readout) else value
            for value in trace.outputs
        ),
        tuple(index for instance in instances for index in instance.destinations),
    )


@dataclass(frozen=True)
class _Composite:
    gtype: int
    connectors: tuple[tuple[int, int], ...]
    measurements: tuple[int, ...]
    readout_count: int


def _model_size(kind: jit.JitGadgetType) -> int:
    return (
        1
        + len(kind.base.measurements)
        + len(kind.finished_checks)
        + len(kind.unfinished_checks)
        + len(kind.errors)
    )


def _groups(
    instances: Sequence[_Instance], types: dict[int, jit.JitGadgetType]
) -> Iterator[list[int]]:
    group: list[int] = []
    size = 0
    for index, instance in enumerate(instances):
        weight = _model_size(types[instance.model.gtype])
        if group and size + weight > _COMPOSITE_SIZE:
            yield group
            group, size = [], 0
        group.append(index)
        size += weight
    if group:
        yield group


def _compose_source(
    types: dict[int, jit.JitGadgetType],
    instances: Sequence[_Instance],
    indices: Sequence[int],
) -> tuple[str, list[tuple[int, int]], list[tuple[int, int]]]:
    members = {index + 1 for index in indices}
    inputs = [
        source
        for index in indices
        for source in instances[index].connectors
        if source[0] not in members
    ]
    if len(inputs) != len(set(inputs)):
        raise ValueError("A composite input port has multiple consumers")
    wires = {source: wire for wire, source in enumerate(inputs)}
    next_wire = len(wires)
    lines = [
        f"INPUT {instances[producer - 1].model.outputs[port]} {wire}"
        for wire, (producer, port) in enumerate(inputs)
    ]
    for index in indices:
        instance = instances[index]
        kind = types[instance.model.gtype].base
        operands = [wires.pop(source) for source in instance.connectors]
        while len(operands) < len(kind.outputs):
            operands.append(next_wire)
            next_wire += 1
        lines.append(kind.name + " " + " ".join(map(str, operands)))
        for port, wire in enumerate(operands[: len(kind.outputs)]):
            wires[index + 1, port] = wire
    lines.extend(
        f"OUTPUT {instances[producer - 1].model.outputs[port]} {wire}"
        for (producer, port), wire in wires.items()
    )
    return "\n".join(lines), inputs, list(wires)


def _compose_gadgets(
    source: circuit.DeqFile,
    artifacts: JitLibraryArtifacts,
    instances: Sequence[_Instance],
) -> tuple[jit.JitLibrary, tuple[_Composite, ...]]:
    library = artifacts.jit_library
    types = {kind.base.gtype: kind for kind in library.gadget_types}
    compile_composite = partial(
        transpile_compose_jit_gadget_type,
        gadget_definitions={
            item.name: item
            for item in source.definitions
            if isinstance(item, circuit.GadgetDefinition)
        },
        compose_definitions={},
        jit_gadget_artifacts_by_name=artifacts.gadget_artifacts_by_name,
        codes={
            item.name: item
            for item in source.definitions
            if isinstance(item, circuit.CodeDefinition)
        },
        ptype_of_code={port.base.name: port.base.ptype for port in library.port_types},
        port_types=list(library.port_types),
        library_has_loss=False,
        loss_model=NoLossModel(),
    )
    cache: dict[str, jit.JitGadgetType] = {}
    composites = []
    owners: dict[tuple[int, int], tuple[int, int]] = {}
    for indices in _groups(instances, types):
        body, inputs, outputs = _compose_source(types, instances, indices)
        if body not in cache:
            gtype = len(cache) + 1
            compose = cast(
                circuit.ComposeDefinition,
                parse(f"COMPOSE QodecComposite{gtype} {{\n{body}\n}}").definitions[0],
            )
            kind = compile_composite(compose, gtype=gtype).jit_type
            # A measurement-free component must still have a window leader.
            kind.base.is_free_hop = False
            cache[body] = kind
        kind = cache[body]
        measurements = tuple(
            index for original in indices for index in instances[original].measurements
        )
        readout_count = sum(instances[index].model.readout_count for index in indices)
        if len(kind.base.inputs) != len(inputs) or len(kind.base.outputs) != len(
            outputs
        ):
            raise ValueError("Composite changed its boundary port count")
        if (
            len(kind.base.measurements) != len(measurements)
            or len(kind.base.readouts) != readout_count
        ):
            raise ValueError("Composite changed its measurement or readout count")
        composites.append(
            _Composite(
                kind.base.gtype,
                tuple(owners.pop(source) for source in inputs),
                measurements,
                readout_count,
            )
        )
        for port, output in enumerate(outputs):
            owners[output] = len(composites), port
    return jit.JitLibrary(
        port_types=library.port_types, gadget_types=list(cache.values())
    ), tuple(composites)


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


def _unpack_scores(
    replies: Sequence[coordinator.Readouts], widths: Sequence[int]
) -> np.ndarray:
    scores = []
    for reply, size in zip(replies, widths, strict=True):
        values = np.asarray(reply.probabilities, dtype=float)
        if len(values) != size:
            raise RuntimeError(
                "deq must return one score per logical readout; "
                'enable coordinator_config={"forced_gap": True} in runtime_factory'
            )
        if not np.all(np.isfinite(values) & (values >= 0) & (values <= 1)):
            raise RuntimeError(
                "deq returned invalid readout scores; expected finite values in [0, 1]"
            )
        scores.append(values)
    return np.concatenate(scores)


@dataclass(frozen=True)
class DeqBatch:
    trace: CircuitTrace
    readout_plan: _ReadoutPlan
    library: jit.JitLibrary
    noise_key: tuple[float, ...]
    composites: tuple[_Composite, ...]
    runtime_factory: Callable[[int], Runtime] = _default_runtime
    max_readout_score: float | None = None

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
            for value in self.readout_plan.outputs:
                if isinstance(value, _Readout):
                    raise ExecutionUnresolved("Circuit has no producer for a readout")
                outputs.append(value)
            return [list(outputs) for _ in physical]
        records, readouts = self._prepare_records(physical, policy)
        if not len(records):
            return []
        widths = [gadget.readout_count for gadget in self.composites]
        weights = {
            kind.base.gtype: _model_size(kind) for kind in self.library.gadget_types
        }
        weight_per_shot = sum(weights[chunk.gtype] for chunk in self.composites)
        batch_size = max(1, min(256, 262144 // weight_per_shot))
        results = []
        async with self.runtime_factory(seed) as runtime:
            service = runtime.jit_controller
            await service.load_library(self.library)
            for start in range(0, len(records), batch_size):
                stop = min(start + batch_size, len(records))
                instructions = self._batch_instructions(stop - start)
                assigned = await service.batch_execute(instructions)
                if assigned != list(range(1, len(instructions) + 1)):
                    raise RuntimeError("deq returned unexpected circuit identifiers")
                replies = await service.batch_decode(
                    self._batch_outcomes(records[start:stop])
                )
                unpacked = _unpack_readouts(replies, widths * (stop - start))
                scores = (
                    _unpack_scores(replies, widths * (stop - start))
                    if self.max_readout_score is not None
                    else None
                )
                results.extend(
                    self._reconstruct_outputs(
                        unpacked, readouts[start:stop], policy, scores=scores
                    )
                )
                if stop < len(records):
                    await service.reset(
                        reset_library=False, reset_decoder_service=False
                    )
        return results

    def _prepare_records(
        self, physical: Sequence[Sequence[Result]], policy: Literal["raise", "discard"]
    ) -> tuple[np.ndarray, np.ndarray]:
        """Fill QDK flag slots now; deq supplies the logical readouts later."""
        records = np.asarray(
            [[value == Result.One for value in shot] for shot in physical], dtype=bool
        ).reshape(len(physical), self.trace.num_measurements)
        readouts = np.zeros((len(physical), self.readout_plan.count), dtype=bool)
        for flag in self.readout_plan.flags:
            readouts[:, flag.destination] = (
                np.logical_xor.reduce(records[:, flag.measurements], axis=1)
                ^ flag.constant
            )
        if policy == "discard" and self.readout_plan.flags:
            keep = [
                shot
                for shot, row in enumerate(readouts)
                if all(
                    selection.accepts(tuple(bool(row[index]) for index in indices))
                    for selection, indices in self.readout_plan.selections
                )
            ]
            records, readouts = records[keep], readouts[keep]
        return records, readouts

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
        readouts: np.ndarray,
        policy: Literal["raise", "discard"],
        *,
        scores: np.ndarray | None = None,
    ) -> list[list[OutputRecordValue]]:
        decoded = np.concatenate(unpacked).reshape(len(readouts), -1)
        plan = self.readout_plan
        readouts = readouts.copy()
        readouts[:, plan.decoded_destinations] = decoded
        if self.max_readout_score is not None:
            if scores is None:
                raise RuntimeError("deq returned no readout scores")
            scores = scores.reshape(len(readouts), -1)[
                :, plan.returned_logical_positions
            ]
        results = []
        for shot, row in enumerate(readouts):
            # Discard-mode selections were applied before decoding.
            if policy == "raise":
                for selection, indices in plan.selections:
                    selection.require(tuple(bool(row[index]) for index in indices))
            if scores is not None and self.max_readout_score is not None:
                worst_position = int(np.argmax(scores[shot]))
                worst = float(scores[shot, worst_position])
                if worst > self.max_readout_score:
                    if policy == "raise":
                        destination = plan.decoded_destinations[
                            plan.returned_logical_positions[worst_position]
                        ]
                        output = plan.outputs.index(_Readout(destination))
                        raise ExecutionRejected(
                            f"Logical readout at output position {output} has score {worst:g}, which exceeds "
                            f"max_readout_score={self.max_readout_score:g}"
                        )
                    continue
            results.append(
                [
                    (
                        (Result.One if row[value.index] else Result.Zero)
                        if isinstance(value, _Readout)
                        else value
                    )
                    for value in plan.outputs
                ]
            )
        return results
