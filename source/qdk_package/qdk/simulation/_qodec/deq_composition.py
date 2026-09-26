"""Compose bounded groups of connected gadgets without closing their code ports."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING

from deq.compiler.jit_compiler import (  # pyright: ignore[reportMissingImports]
    static_jit_compiler,
)
from deq.proto import (  # pyright: ignore[reportMissingImports]
    deq_bin_pb2 as model,
    deq_jit_pb2 as jit,
)
from deq.spec.canonical import merge  # pyright: ignore[reportMissingImports]
from deq.transpiler.compose_builder import (  # pyright: ignore[reportMissingImports]
    _mk_input_mock,
    _mk_output_mock,
)

if TYPE_CHECKING:
    from .deq_circuit import _Gadget


_COMPOSITE_SIZE = 1024


@dataclass(frozen=True)
class _Composite:
    gtype: int
    connectors: tuple[tuple[int, int], ...]
    measurements: tuple[int, ...]
    readouts: tuple[tuple[int, tuple[int, ...]], ...]


@dataclass(frozen=True)
class _CompositeType:
    kind: jit.JitGadgetType
    measurements: tuple[tuple[int, int], ...]
    readouts: tuple[tuple[int, ...], ...]


def _model_size(kind: jit.JitGadgetType) -> int:
    return (
        1
        + len(kind.base.measurements)
        + len(kind.finished_checks)
        + len(kind.unfinished_checks)
        + len(kind.errors)
    )


def _groups(
    gadgets: Sequence[_Gadget], types: dict[int, jit.JitGadgetType]
) -> Iterator[list[int]]:
    group: list[int] = []
    size = 0
    for index, gadget in enumerate(gadgets):
        weight = _model_size(types[gadget.gtype])
        if group and size + weight > _COMPOSITE_SIZE:
            yield group
            group, size = [], 0
        group.append(index)
        size += weight
    if group:
        yield group


def _chunk_library(
    library: jit.JitLibrary, gadgets: Sequence[_Gadget], indices: Sequence[int]
) -> tuple[jit.JitLibrary, list[int], list[tuple[int, int]], list[tuple[int, int]]]:
    types = {kind.base.gtype: kind for kind in library.gadget_types}
    ports = {port.base.ptype: port for port in library.port_types}
    members = {index + 1 for index in indices}
    inputs = [
        source
        for index in indices
        for source in gadgets[index].connectors
        if source[0] not in members
    ]
    if len(inputs) != len(set(inputs)):
        raise ValueError("A composite input port has multiple consumers")
    input_sources = {source: (index + 1, 0) for index, source in enumerate(inputs)}
    live_outputs: dict[tuple[int, int], tuple[int, int]] = {}
    real_gids: list[int] = []
    instructions: list[jit.JitInstruction] = []
    mock_types: list[jit.JitGadgetType] = []
    next_type = max(types) + 1
    # Like deq COMPOSE, use one mock per input port: merge indexes peers by gid.
    for source, (gid, _) in input_sources.items():
        producer, port_index = source
        ptype = types[gadgets[producer - 1].gtype].base.outputs[port_index].ptype
        port = ports[ptype]
        gtype = next_type + len(mock_types)
        mock_types.append(
            _mk_input_mock(
                gtype, [ptype], [len(port.stabilizers)], len(port.base.observables)
            )
        )
        instructions.append(
            jit.JitInstruction(gadget=model.Gadget(gtype=gtype, gid=gid))
        )
    for index in indices:
        source = gadgets[index]
        gid = len(instructions) + 1
        real_gids.append(gid)
        connectors = []
        for origin in source.connectors:
            producer, port_index = (
                input_sources[origin]
                if origin in input_sources
                else live_outputs.pop(origin)
            )
            connectors.append(model.Gadget.Connector(gid=producer, port=port_index))
        instructions.append(
            jit.JitInstruction(
                gadget=model.Gadget(gtype=source.gtype, gid=gid, connectors=connectors)
            )
        )
        for port_index in range(len(types[source.gtype].base.outputs)):
            live_outputs[index + 1, port_index] = gid, port_index
    outputs = list(live_outputs)
    output_ptypes = [
        types[gadgets[producer - 1].gtype].base.outputs[port_index].ptype
        for producer, port_index in outputs
    ]
    output_type = next_type + len(mock_types)
    mock_types.append(
        _mk_output_mock(
            output_type,
            output_ptypes,
            [len(ports[ptype].stabilizers) for ptype in output_ptypes],
            sum(len(ports[ptype].base.observables) for ptype in output_ptypes),
        )
    )
    instructions.append(
        jit.JitInstruction(
            gadget=model.Gadget(
                gtype=output_type,
                gid=len(instructions) + 1,
                connectors=[
                    model.Gadget.Connector(gid=gid, port=port)
                    for gid, port in live_outputs.values()
                ],
            )
        )
    )
    return (
        jit.JitLibrary(
            port_types=library.port_types,
            gadget_types=[*types.values(), *mock_types],
            program=instructions,
        ),
        real_gids,
        inputs,
        outputs,
    )


def _merge_program(
    library: jit.JitLibrary, real_gids: Sequence[int], gtype: int
) -> _CompositeType:
    merged = merge(static_jit_compiler(library), real_gids)
    kind = merged.to_jit_gadget_type(gtype=gtype, name=f"QodecComposite{gtype}")
    # A measurement-free component must still have a window leader.
    kind.base.is_free_hop = False
    local_indices = {gid: offset for offset, gid in enumerate(real_gids)}
    measurements = {
        target.measurement_index: (
            local_indices[source.gid],
            source.measurement_index,
        )
        for source, target in merged.measurement_map.atob.items()
    }
    readouts = {
        (source.gid, source.readout_index): target.readout_index
        for source, target in merged.readout_map.atob.items()
    }
    widths = {kind.base.gtype: len(kind.base.readouts) for kind in library.gadget_types}
    return _CompositeType(
        kind,
        tuple(measurements[index] for index in range(len(kind.base.measurements))),
        tuple(
            tuple(
                readouts[gid, index]
                for index in range(widths[library.program[gid - 1].gadget.gtype])
            )
            for gid in real_gids
        ),
    )


def _compose_gadgets(
    library: jit.JitLibrary, gadgets: Sequence[_Gadget]
) -> tuple[jit.JitLibrary, tuple[_Composite, ...]]:
    types = {kind.base.gtype: kind for kind in library.gadget_types}
    cache: dict[tuple[bytes, ...], _CompositeType] = {}
    instances: list[_Composite] = []
    owners: dict[tuple[int, int], tuple[int, int]] = {}
    for indices in _groups(gadgets, types):
        local, real_gids, inputs, outputs = _chunk_library(library, gadgets, indices)
        key = tuple(
            instruction.SerializeToString(deterministic=True)
            for instruction in local.program
        )
        if key not in cache:
            cache[key] = _merge_program(local, real_gids, len(cache) + 1)
        composed = cache[key]
        if len(composed.kind.base.inputs) != len(inputs) or len(
            composed.kind.base.outputs
        ) != len(outputs):
            raise ValueError("Composite changed its boundary port count")
        instances.append(
            _Composite(
                composed.kind.base.gtype,
                tuple(owners.pop(source) for source in inputs),
                tuple(
                    gadgets[indices[offset]].start + measurement
                    for offset, measurement in composed.measurements
                ),
                tuple(zip(indices, composed.readouts)),
            )
        )
        for port, source in enumerate(outputs):
            owners[source] = len(instances), port
    return (
        jit.JitLibrary(
            port_types=library.port_types,
            gadget_types=[composed.kind for composed in cache.values()],
        ),
        tuple(instances),
    )
