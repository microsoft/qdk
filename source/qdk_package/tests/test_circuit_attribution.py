# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for asking a raised program where each of its operations came from.

The Rust side proves that a node keeps its origin through a real rewrite. What
these tests cover is the part a Rust test cannot: that a Python user can ask,
that the answer survives a transformation the user drives from Python, and that
a graph which did not originate as a circuit reports nothing rather than a
guess.
"""

import warnings

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import (  # noqa: E402
    CircuitOrigin,
    GraphBuilder,
    OriginKind,
    PortKind,
    QuantumCircuit,
    QuantumGraph,
    SymbolId,
)


def instruction(index: int) -> CircuitOrigin:
    return CircuitOrigin(OriginKind.Instruction, index)


def cancellable() -> QuantumCircuit:
    """Two adjacent inverses followed by a gate that survives them."""
    circuit = QuantumCircuit(1, 0)
    circuit.h(0)
    circuit.h(0)
    circuit.x(0)
    return circuit


def rebuild(
    graph: QuantumGraph,
    drop: tuple[str, ...] = (),
    keep_origins: bool = True,
    symbol_package: str | None = None,
) -> QuantumGraph:
    """Rebuilds a graph, optionally without the nodes naming an operation.

    A dropped node's qubit capabilities are spliced past it, which is what a
    cancellation leaves behind. Only a node whose operands are all capabilities
    can be dropped this way, which is every gate that neither measures nor
    carries a parameter.

    Dropping the origins is how a graph that came from some other producer looks
    from here. The values are carried either way, because a pass that lost them
    would produce a graph that no longer raises.
    """
    nodes = {node.id: node for node in graph.nodes}
    ports = {port.id: port for port in graph.ports}
    source_of = {edge.target: edge.source for edge in graph.edges}
    dropped = {node.id for node in graph.nodes if node.operation in drop}

    def capabilities(port_ids: list[int]) -> list[int]:
        return [
            port_id
            for port_id in port_ids
            if ports[port_id].kind == PortKind.Capability
        ]

    def resolve(port_id: int) -> int:
        while ports[port_id].node in dropped:
            node = nodes[ports[port_id].node]
            taken = capabilities(node.inputs)
            given = capabilities(node.outputs)
            port_id = source_of[taken[given.index(port_id)]]
        return port_id

    builder = GraphBuilder()
    regions = {}
    for region in graph.regions:
        parent = None if region.parent is None else regions[region.parent]
        regions[region.id] = builder.add_region(region.kind, parent)

    new_ports = {}
    for node in graph.nodes:
        if node.id in dropped:
            continue
        rebuilt = builder.add_node(regions[node.region], node.operation, node.effects)
        builder.declare_payload(
            rebuilt,
            number=node.number,
            classical_bit=node.classical_bit,
            angle=node.angle,
            origin=node.origin if keep_origins else None,
        )
        if symbol_package is not None:
            builder.declare_symbol(rebuilt, SymbolId(symbol_package, "item"))
        for port_id in list(node.inputs) + list(node.outputs):
            port = ports[port_id]
            new_ports[port_id] = builder.add_port(
                rebuilt,
                port.direction,
                port.kind,
                port.value_type,
                port.stage,
                port.usage,
            )

    for edge in graph.edges:
        if ports[edge.target].node in dropped:
            continue
        builder.connect(
            new_ports[resolve(edge.source)], new_ports[edge.target], edge.kind
        )
    return builder.finish()


def test_each_raised_instruction_names_the_instruction_it_came_from() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.measure(1, 0)
    raised = QuantumCircuit.from_graph_with_origins(circuit.to_graph())
    assert raised.circuit.to_bytes() == circuit.to_bytes()
    assert raised.origins == [instruction(0), instruction(1), instruction(2)]


def test_a_parameter_does_not_shift_what_an_instruction_is_attributed_to() -> None:
    circuit = QuantumCircuit(1, 0)
    circuit.rx(0.25, 0)
    circuit.ry(-1.5, 0)
    raised = QuantumCircuit.from_graph_with_origins(circuit.to_graph())
    assert raised.origins == [instruction(0), instruction(1)]


def test_attribution_survives_a_transformation_that_removes_operations() -> None:
    cancelled = rebuild(cancellable().to_graph(), drop=("quantum.h",))
    raised = QuantumCircuit.from_graph_with_origins(cancelled)

    assert len(raised.circuit) == 1
    # The survivor was the third instruction written and is now the first one
    # raised. Numbering the raised program positionally would say instruction
    # zero, which is one of the two the transformation removed.
    assert raised.origins == [instruction(2)]


def test_a_removed_operation_is_absent_rather_than_reassigned() -> None:
    cancelled = rebuild(cancellable().to_graph(), drop=("quantum.h",))
    raised = QuantumCircuit.from_graph_with_origins(cancelled)
    assert instruction(0) not in raised.origins
    assert instruction(1) not in raised.origins


def test_a_graph_that_did_not_come_from_a_circuit_reports_no_attribution() -> None:
    plain = rebuild(cancellable().to_graph(), keep_origins=False)
    raised = QuantumCircuit.from_graph_with_origins(plain)
    assert len(raised.circuit) == 3
    assert raised.origins == [None, None, None]


def test_a_source_symbol_is_not_read_as_attribution() -> None:
    # A symbol names a source-level item and an origin names a position in a
    # lowering. Filling one must not answer for the other.
    foreign = rebuild(
        cancellable().to_graph(), keep_origins=False, symbol_package="qsharp"
    )
    raised = QuantumCircuit.from_graph_with_origins(foreign)
    assert raised.origins == [None, None, None]


def test_an_origin_carries_the_kind_and_the_index_separately() -> None:
    origin = QuantumCircuit.from_graph_with_origins(cancellable().to_graph()).origins[0]
    assert origin is not None
    assert origin.kind == OriginKind.Instruction
    assert origin.index == 0
    assert CircuitOrigin(OriginKind.GlobalPhase).index is None


def test_the_attributed_raise_recovers_the_same_circuit_as_the_plain_one() -> None:
    circuit = cancellable()
    raised = QuantumCircuit.from_graph_with_origins(circuit.to_graph())
    plain = QuantumCircuit.from_graph(circuit.to_graph())
    assert raised.circuit.to_bytes() == plain.to_bytes()
