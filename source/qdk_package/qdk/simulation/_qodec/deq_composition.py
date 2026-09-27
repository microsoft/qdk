"""Build bounded COMPOSE definitions over compiled primitive gadget models."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from dataclasses import dataclass
from functools import partial
from typing import TYPE_CHECKING, cast

from deq.circuit import model as circuit  # pyright: ignore[reportMissingImports]
from deq.circuit.parser import parse  # pyright: ignore[reportMissingImports]
from deq.proto import deq_jit_pb2 as jit  # pyright: ignore[reportMissingImports]
from deq.transpiler.compose_builder import (  # pyright: ignore[reportMissingImports]
    transpile_compose_jit_gadget_type,
)
from deq.transpiler.jit_library_builder import (  # pyright: ignore[reportMissingImports]
    JitLibraryArtifacts,
)
from deq.transpiler.loss.model_none import (  # pyright: ignore[reportMissingImports]
    NoLossModel,
)

if TYPE_CHECKING:
    from .deq_decoding import _Gadget


_COMPOSITE_SIZE = 1024


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


def _compose_source(
    library: jit.JitLibrary, gadgets: Sequence[_Gadget], indices: Sequence[int]
) -> tuple[str, list[tuple[int, int]], list[tuple[int, int]]]:
    types = {kind.base.gtype: kind.base for kind in library.gadget_types}
    codes = {port.base.ptype: port.base.name for port in library.port_types}
    members = {index + 1 for index in indices}
    inputs = [
        source
        for index in indices
        for source in gadgets[index].connectors
        if source[0] not in members
    ]
    if len(inputs) != len(set(inputs)):
        raise ValueError("A composite input port has multiple consumers")
    wires = {source: wire for wire, source in enumerate(inputs)}
    next_wire = len(wires)
    lines = [
        f"INPUT {codes[types[gadgets[producer - 1].gtype].outputs[port].ptype]} {wire}"
        for wire, (producer, port) in enumerate(inputs)
    ]
    for index in indices:
        gadget = gadgets[index]
        kind = types[gadget.gtype]
        operands = [wires.pop(source) for source in gadget.connectors]
        while len(operands) < len(kind.outputs):
            operands.append(next_wire)
            next_wire += 1
        lines.append(kind.name + " " + " ".join(map(str, operands)))
        for port, wire in enumerate(operands[: len(kind.outputs)]):
            wires[index + 1, port] = wire
    lines.extend(
        f"OUTPUT {codes[types[gadgets[producer - 1].gtype].outputs[port].ptype]} {wire}"
        for (producer, port), wire in wires.items()
    )
    return "\n".join(lines), inputs, list(wires)


def _compose_gadgets(
    source: circuit.DeqFile,
    artifacts: JitLibraryArtifacts,
    gadgets: Sequence[_Gadget],
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
    instances: list[_Composite] = []
    owners: dict[tuple[int, int], tuple[int, int]] = {}
    for indices in _groups(gadgets, types):
        body, inputs, outputs = _compose_source(library, gadgets, indices)
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
            index
            for original in indices
            for index in range(
                gadgets[original].start,
                gadgets[original].start + gadgets[original].width,
            )
        )
        readout_count = sum(
            len(gadgets[index].readouts) + gadgets[index].checks for index in indices
        )
        if len(kind.base.inputs) != len(inputs) or len(kind.base.outputs) != len(
            outputs
        ):
            raise ValueError("Composite changed its boundary port count")
        if (
            len(kind.base.measurements) != len(measurements)
            or len(kind.base.readouts) != readout_count
        ):
            raise ValueError("Composite changed its measurement or readout count")
        instances.append(
            _Composite(
                kind.base.gtype,
                tuple(owners.pop(source) for source in inputs),
                measurements,
                readout_count,
            )
        )
        for port, output in enumerate(outputs):
            owners[output] = len(instances), port
    return (
        jit.JitLibrary(
            port_types=library.port_types,
            gadget_types=list(cache.values()),
        ),
        tuple(instances),
    )
