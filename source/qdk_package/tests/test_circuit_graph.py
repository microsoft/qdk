# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for moving a circuit into the graph representation and back.

The lowering and the raise are tested in Rust. What these tests cover is the
part a Rust test cannot: that a Python user reaches both directions, that the
graph can be authored and read from Python, and that a refusal arrives naming
the construct rather than flattened into a generic failure.
"""

import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import (  # noqa: E402
    EdgeKind,
    EffectDomain,
    EffectEvent,
    EffectMode,
    GraphBuilder,
    InterfaceBinding,
    InterfaceRole,
    PortDirection,
    PortKind,
    PortStage,
    PortUsage,
    QuantumCircuit,
    QuantumGraph,
    RegionKind,
    SymbolId,
    ValueType,
)


def bell() -> QuantumCircuit:
    """Two entangled qubits, both measured."""
    circuit = QuantumCircuit(2, 2)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.measure(0, 0)
    circuit.measure(1, 1)
    return circuit


def foreign_graph(operation: str = "call") -> QuantumGraph:
    """A graph shaped like something other than a lowered circuit.

    One node under a control exit, with no capabilities and no values, which is
    what the compiler's own importer produces and what no circuit can express.
    Reading it as a circuit would have to invent every operand.
    """
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    node = builder.add_node(region, operation)
    builder.add_port(
        node,
        PortDirection.Output,
        PortKind.ControlExit,
        ValueType.unit(),
        PortStage.Residual,
        PortUsage.Affine,
    )
    return builder.finish()


def test_a_circuit_survives_the_crossing_into_the_graph_and_back() -> None:
    circuit = bell()
    restored = QuantumCircuit.from_graph(circuit.to_graph())
    assert restored.to_text() == circuit.to_text()


def test_a_parameterized_gate_survives_the_crossing() -> None:
    circuit = QuantumCircuit(1, 0)
    circuit.rx(0.25, 0)
    circuit.ry(-1.5, 0)
    restored = QuantumCircuit.from_graph(circuit.to_graph())
    assert restored.to_text() == circuit.to_text()


def test_a_lowered_circuit_satisfies_the_graph_s_own_rules() -> None:
    assert bell().to_graph().diagnostics() == []


def test_graph_binary_snapshot_restores_identity_and_program() -> None:
    graph = bell().to_graph()
    data = graph.to_bytes()
    assert isinstance(data, bytes) and data.startswith(b"QDKQHDG!\x01\n")
    back = QuantumGraph.from_bytes(data)
    assert back.to_bytes() == data
    assert back.diagnostics() == []
    assert [(node.id, node.operation) for node in back.nodes] == [
        (node.id, node.operation) for node in graph.nodes
    ]


def test_graph_text_snapshot_has_the_same_binary_model() -> None:
    graph = bell().to_graph()
    text = graph.to_text()
    assert text.startswith("QDKQHDG!(\n(qdk 1 ")
    assert QuantumGraph.from_text(text).to_bytes() == graph.to_bytes()


def test_graph_snapshot_tampering_is_rejected() -> None:
    graph = bell().to_graph()
    text = graph.to_text()
    changed = text.replace('"quantum.h"', '"quantum.x"')
    assert changed != text
    with pytest.raises(ValueError, match="digest"):
        QuantumGraph.from_text(changed)


def test_graph_import_rejects_a_circuit_snapshot() -> None:
    with pytest.raises(ValueError, match="magic"):
        QuantumGraph.from_bytes(bell().to_bytes())


def test_the_lowered_graph_names_the_circuit_s_operations() -> None:
    operations = [node.operation for node in bell().to_graph().nodes]
    assert "quantum.h" in operations
    assert "quantum.cx" in operations
    assert operations.count("quantum.allocate") == 2
    assert operations.count("classical.allocate") == 2


def test_a_qubit_becomes_a_capability_flowing_along_an_edge() -> None:
    graph = bell().to_graph()
    ports = {port.id: port for port in graph.ports}
    capability_edges = [
        edge for edge in graph.edges if edge.kind == EdgeKind.AffineCapability
    ]
    assert capability_edges, "a circuit with qubits should carry capabilities"
    for edge in capability_edges:
        assert ports[edge.source].kind == PortKind.Capability
        assert ports[edge.source].direction == PortDirection.Output
        assert ports[edge.target].direction == PortDirection.Input


def test_a_conditional_reaches_the_graph_and_comes_back() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)
    graph = circuit.to_graph()
    branches = [region for region in graph.regions if region.kind == RegionKind.Gamma]
    assert len(branches) == 1, "the branch is a region rather than inline code"
    assert (
        branches[0].owner is not None
    ), "a branch is the body of the node that selects it"
    # The raise recovers the conditional but not the surface's spelling of its
    # condition: the graph carries a read of the classical bit, because that is
    # what comparing a one-bit result against one means.
    assert QuantumCircuit.from_graph(graph)[1].name == "if_else"


def test_a_construct_the_graph_has_no_form_for_is_named() -> None:
    circuit = QuantumCircuit(1, 1)
    circuit.measure(0, 0)
    with circuit.while_loop(0, 1):
        circuit.x(0)
    with pytest.raises(ValueError, match="operation 'while' has no graph form"):
        circuit.to_graph()


def test_an_unbound_parameter_reaches_the_graph() -> None:
    circuit = QuantumCircuit(1, 0)
    circuit.rx(circuit.parameter("theta"), 0)
    graph = circuit.to_graph()
    angles = [node.angle for node in graph.nodes if node.angle is not None]
    assert [angle.parameters for angle in angles] == [["theta"]]
    assert angles[0].value is None
    assert QuantumCircuit.from_graph(graph).to_text() == circuit.to_text()


def test_a_construct_the_circuit_has_no_form_for_is_named() -> None:
    with pytest.raises(ValueError, match="operation 'call' has no circuit form"):
        QuantumCircuit.from_graph(foreign_graph())


def test_the_refusal_names_the_operation_it_found() -> None:
    with pytest.raises(ValueError, match="magic_state_distill"):
        QuantumCircuit.from_graph(foreign_graph("quantum.magic_state_distill"))


def test_lowering_while_a_block_is_open_is_refused() -> None:
    circuit = QuantumCircuit(1, 1)
    circuit.measure(0, 0)
    block = circuit.if_test(0, 1)
    block.__enter__()
    try:
        with pytest.raises(ValueError, match="cannot lower a circuit"):
            circuit.to_graph()
    finally:
        block.__exit__(None, None, None)


def test_a_graph_authored_by_hand_reads_back_as_it_was_built() -> None:
    builder = GraphBuilder()
    outer = builder.add_region(RegionKind.ModuleOmega)
    inner = builder.add_region(RegionKind.Gamma, outer)
    source = builder.add_node(outer, "quantum.allocate")
    produced = builder.add_port(
        source,
        PortDirection.Output,
        PortKind.Capability,
        ValueType.qubit(),
        PortStage.Residual,
        PortUsage.Affine,
    )
    sink = builder.add_node(outer, "quantum.release")
    consumed = builder.add_port(
        sink,
        PortDirection.Input,
        PortKind.Capability,
        ValueType.qubit(),
        PortStage.Residual,
        PortUsage.Affine,
    )
    builder.connect(produced, consumed, EdgeKind.AffineCapability)
    graph = builder.finish()

    assert [(region.id, region.parent, region.kind) for region in graph.regions] == [
        (outer, None, RegionKind.ModuleOmega),
        (inner, outer, RegionKind.Gamma),
    ]
    assert [node.operation for node in graph.nodes] == [
        "quantum.allocate",
        "quantum.release",
    ]
    assert [node.outputs for node in graph.nodes] == [[produced], []]
    assert [node.inputs for node in graph.nodes] == [[], [consumed]]
    assert [(edge.source, edge.target, edge.kind) for edge in graph.edges] == [
        (produced, consumed, EdgeKind.AffineCapability)
    ]
    port = next(port for port in graph.ports if port.id == produced)
    assert port.node == source
    assert port.region == outer
    assert port.value_type == ValueType.qubit()
    assert port.stage == PortStage.Residual
    assert port.usage == PortUsage.Affine


def test_an_edge_between_mismatched_ports_is_refused_where_it_is_written() -> None:
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    source = builder.add_node(region, "quantum.allocate")
    produced = builder.add_port(
        source,
        PortDirection.Output,
        PortKind.Capability,
        ValueType.qubit(),
        PortStage.Residual,
        PortUsage.Affine,
    )
    sink = builder.add_node(region, "classical.store")
    consumed = builder.add_port(
        sink,
        PortDirection.Input,
        PortKind.Value,
        ValueType.bool(),
        PortStage.Residual,
        PortUsage.Copyable,
    )
    with pytest.raises(ValueError, match="does not match|do not match"):
        builder.connect(produced, consumed, EdgeKind.AffineCapability)


def test_a_qubit_inside_an_array_cannot_be_declared_copyable() -> None:
    # A qubit is no more copyable for being inside an array, so the declaration
    # is refused where it is written. The builder is otherwise low-level and
    # this is the one combination it will not express.
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    node = builder.add_node(region, "quantum.allocate_array")
    with pytest.raises(ValueError, match="carries a qubit cannot be copyable"):
        builder.add_port(
            node,
            PortDirection.Output,
            PortKind.Value,
            ValueType.array(ValueType.qubit()),
            PortStage.Generation,
            PortUsage.Copyable,
        )


def test_a_qubit_inside_an_array_cannot_reach_two_consumers() -> None:
    # The same array declared affine is expressible, and fanning it out is what
    # no-cloning forbids. The rule reads the type rather than the port kind, so
    # declaring the array as a value does not move it outside the rule.
    array = ValueType.array(ValueType.qubit())
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    producer = builder.add_node(region, "quantum.allocate_array")
    source = builder.add_port(
        producer,
        PortDirection.Output,
        PortKind.Value,
        array,
        PortStage.Generation,
        PortUsage.Affine,
    )
    for operation in ("quantum.apply_all", "quantum.release_array"):
        consumer = builder.add_node(region, operation)
        target = builder.add_port(
            consumer,
            PortDirection.Input,
            PortKind.Value,
            array,
            PortStage.Generation,
            PortUsage.Affine,
        )
        builder.connect(source, target, EdgeKind.CopyableValue)

    assert builder.finish().diagnostics() == [
        "QHDG-OWNERSHIP-FANOUT-001: affine capability has multiple uses"
    ]


def test_a_node_that_does_not_exist_is_refused() -> None:
    builder = GraphBuilder()
    builder.add_region(RegionKind.ModuleOmega)
    with pytest.raises(ValueError, match="unknown region, node, or port"):
        builder.add_port(
            404,
            PortDirection.Output,
            PortKind.Value,
            ValueType.int(),
            PortStage.Residual,
            PortUsage.Copyable,
        )


def test_a_builder_produces_one_graph_and_no_more() -> None:
    builder = GraphBuilder()
    builder.add_region(RegionKind.ModuleOmega)
    builder.finish()
    with pytest.raises(ValueError, match="already produced a graph"):
        builder.finish()


def test_an_effect_survives_being_read_back() -> None:
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    builder.add_node(
        region,
        "quantum.measure",
        [
            EffectEvent(
                EffectDomain.Quantum,
                EffectMode.Irreversible,
                resource="q0",
            )
        ],
    )
    (node,) = builder.finish().nodes
    (effect,) = node.effects
    assert effect.domain == EffectDomain.Quantum
    assert effect.mode == EffectMode.Irreversible
    assert effect.resource == "q0"
    assert effect.observable is True


def test_an_effect_on_an_unnamed_resource_reads_back_as_absent() -> None:
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    builder.add_node(
        region,
        "host.call",
        [EffectEvent(EffectDomain.Host, EffectMode.Unknown, observable=False)],
    )
    (node,) = builder.finish().nodes
    (effect,) = node.effects
    assert effect.resource is None
    assert effect.observable is False


def test_a_declared_symbol_reads_back_and_an_undeclared_one_is_absent() -> None:
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    named = builder.add_node(region, "quantum.h")
    builder.add_node(region, "quantum.x")
    builder.declare_symbol(named, SymbolId("Std.Intrinsic", "H"))
    first, second = builder.finish().nodes
    assert first.symbol is not None
    assert (first.symbol.package, first.symbol.item) == ("Std.Intrinsic", "H")
    assert second.symbol is None


def test_an_interface_binding_that_crosses_no_boundary_is_refused() -> None:
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    node = builder.add_node(region, "quantum.h")
    port = builder.add_port(
        node,
        PortDirection.Output,
        PortKind.Value,
        ValueType.int(),
        PortStage.Residual,
        PortUsage.Copyable,
    )
    with pytest.raises(ValueError, match="does not cross the declared parent"):
        builder.bind_interface(
            region, InterfaceBinding(port, port, InterfaceRole.Argument)
        )


def test_a_nested_type_is_carried_through_the_binding() -> None:
    nested = ValueType.array(ValueType.tuple([ValueType.int(), ValueType.qubit()]))
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    node = builder.add_node(region, "quantum.h")
    builder.add_port(
        node,
        PortDirection.Output,
        PortKind.Value,
        nested,
        PortStage.Generation,
        # Affine rather than copyable: the tuple reaches a qubit, and the
        # builder refuses to call a type that reaches a qubit copyable.
        PortUsage.Affine,
    )
    (port,) = builder.finish().ports
    assert port.value_type == nested
    assert port.value_type != ValueType.array(ValueType.int())


def test_the_graph_hands_out_nothing_that_borrows_it() -> None:
    # The graph's transaction type carries a lifetime tied to the graph it
    # mutates, so it could never be held across a call from Python. The two
    # pass contracts are the case that matters: they call back into Python and
    # still appear here, because they hand out owned matches rather than views.
    graph = bell().to_graph()
    exposed = {name for name in dir(graph) if not name.startswith("_")}
    assert exposed == {
        "regions",
        "nodes",
        "ports",
        "edges",
        "diagnostics",
        "to_bytes",
        "from_bytes",
        "to_text",
        "from_text",
        "to_qir",
        "to_qasm",
        "bind_parameters",
        "rewrite",
        "expand",
    }
