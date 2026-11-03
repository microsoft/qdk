# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for compiling a graph rather than the circuit it came from.

The lowering and its agreement with the direct path are proven in Rust. What
these tests cover is the part a Rust test cannot: that a Python user reaches
the graph path, that it names the same profiles the circuit surface names, and
that a graph the path does not accept is refused with the operation named.
"""

import warnings

import pytest

from qdk import TargetProfile

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import (  # noqa: E402
    EdgeKind,
    GraphBuilder,
    PortDirection,
    PortKind,
    PortStage,
    PortUsage,
    QuantumCircuit,
    QuantumGraph,
    RegionKind,
    ValueType,
)

PROFILES = [TargetProfile.Base, TargetProfile.Adaptive_RI]


def bell() -> QuantumCircuit:
    """Two entangled qubits, both measured."""
    circuit = QuantumCircuit(2, 2)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.measure(0, 0)
    circuit.measure(1, 1)
    return circuit


def one_qubit_graph(operation: str) -> QuantumGraph:
    """A graph holding one operation between an allocate and a release.

    Built by hand so the operation can be one the circuit lowering would never
    emit, while the shape around it stays what the reader expects.
    """
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)

    def capability(node: int, direction: PortDirection) -> int:
        return builder.add_port(
            node,
            direction,
            PortKind.Capability,
            ValueType.qubit(),
            PortStage.Residual,
            PortUsage.Affine,
        )

    allocate = builder.add_node(region, "quantum.allocate")
    allocated = capability(allocate, PortDirection.Output)
    node = builder.add_node(region, operation)
    taken = capability(node, PortDirection.Input)
    given = capability(node, PortDirection.Output)
    release = builder.add_node(region, "quantum.release")
    released = capability(release, PortDirection.Input)

    builder.connect(allocated, taken, EdgeKind.AffineCapability)
    builder.connect(given, released, EdgeKind.AffineCapability)
    return builder.finish()


def foreign_graph() -> QuantumGraph:
    """A graph from the vocabulary the compiler's own importer produces."""
    builder = GraphBuilder()
    region = builder.add_region(RegionKind.ModuleOmega)
    node = builder.add_node(region, "call")
    builder.add_port(
        node,
        PortDirection.Output,
        PortKind.ControlExit,
        ValueType.unit(),
        PortStage.Residual,
        PortUsage.Affine,
    )
    return builder.finish()


@pytest.mark.parametrize("profile", PROFILES)
def test_a_graph_compiles_to_qir_for_a_named_profile(profile: TargetProfile) -> None:
    qir = bell().to_graph().to_qir(profile)

    assert "define i64 @ENTRYPOINT__main()" in qir
    assert "__quantum__qis__h__body" in qir
    assert "__quantum__qis__m__body" in qir


@pytest.mark.parametrize("profile", PROFILES)
def test_compiling_the_graph_agrees_with_compiling_the_circuit(
    profile: TargetProfile,
) -> None:
    circuit = bell()

    assert circuit.to_graph().to_qir(profile) == circuit.to_qir(profile)


def test_a_conditional_agrees_across_the_two_paths() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)

    profile = TargetProfile.Adaptive_RI
    assert circuit.to_graph().to_qir(profile) == circuit.to_qir(profile)


def test_a_conditional_needs_a_profile_that_can_branch() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)

    with pytest.raises(ValueError, match="requires the Adaptive capability"):
        circuit.to_graph().to_qir(TargetProfile.Base)


def test_a_gate_carrying_an_angle_agrees_across_the_two_paths() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.rx(0.25, 0)
    circuit.crz(-1.5, 0, 1)
    circuit.measure(1, 0)

    assert circuit.to_graph().to_qir(TargetProfile.Base) == circuit.to_qir(
        TargetProfile.Base
    )


def test_a_graph_from_another_producer_is_refused_by_name() -> None:
    with pytest.raises(ValueError, match="operation 'call'"):
        foreign_graph().to_qir(TargetProfile.Base)


def test_an_unbound_parameter_is_refused_by_name() -> None:
    # The graph carries symbolic angles so passes can run on a variational
    # program. A target cannot, so the boundary refuses rather than emitting a
    # zero.
    circuit = QuantumCircuit(1, 0)
    circuit.rx(circuit.parameter("theta"), 0)
    with pytest.raises(ValueError, match="parameter 'theta' is unbound"):
        circuit.to_graph().to_qir(TargetProfile.Base)


def test_a_rebound_graph_compiles_to_what_the_bound_circuit_compiles_to() -> None:
    circuit = QuantumCircuit(1, 0)
    circuit.rx(circuit.parameter("theta"), 0)
    graph = circuit.to_graph()

    bound = QuantumCircuit(1, 0)
    bound.rx(0.375, 0)

    assert graph.bind_parameters({"theta": 0.375}).to_qir(
        TargetProfile.Base
    ) == bound.to_qir(TargetProfile.Base)
    # The symbolic program survives the binding, so the next set of values binds
    # the same graph rather than a rebuilt one.
    with pytest.raises(ValueError, match="parameter 'theta' is unbound"):
        graph.to_qir(TargetProfile.Base)


def test_a_rebind_changes_the_value_and_nothing_structural() -> None:
    circuit = QuantumCircuit(1, 0)
    circuit.rx(circuit.parameter("theta"), 0)
    graph = circuit.to_graph()
    rebound = graph.bind_parameters({"theta": 0.5})

    def shape(target: QuantumGraph) -> tuple:
        return (
            [(node.id, node.operation) for node in target.nodes],
            [(port.id, port.node) for port in target.ports],
            [(edge.source, edge.target) for edge in target.edges],
        )

    assert shape(rebound) == shape(graph)
    assert [node.angle for node in graph.nodes if node.angle is not None]
    assert not [node.angle for node in rebound.nodes if node.angle is not None]
    assert 0.5 in [node.number for node in rebound.nodes]


def test_an_operation_with_no_runtime_form_is_named() -> None:
    graph = one_qubit_graph("quantum.magic_state_distill")

    with pytest.raises(ValueError, match="quantum.magic_state_distill"):
        graph.to_qir(TargetProfile.Base)


def test_openqasm_from_a_graph_is_what_the_raised_circuit_exports() -> None:
    circuit = bell()
    graph = circuit.to_graph()

    assert graph.to_qasm() == QuantumCircuit.from_graph(graph).to_qasm()
    assert graph.to_qasm().startswith("OPENQASM 3.0;")


def test_openqasm_from_a_graph_outside_the_subset_is_refused_by_name() -> None:
    graph = one_qubit_graph("quantum.magic_state_distill")

    with pytest.raises(ValueError, match="quantum.magic_state_distill"):
        graph.to_qasm()
