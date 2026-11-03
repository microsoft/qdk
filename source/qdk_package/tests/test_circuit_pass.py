# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the two pass contracts over the graph.

The load-bearing test is the counting harness. A contract that let the pass
read the graph would satisfy every other test here and be unusable at scale,
because its cost would follow the size of the program rather than the size of
the change.
"""

import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk import TargetProfile  # noqa: E402
from qdk.circuit import PassPatch, QuantumCircuit  # noqa: E402

BASE = TargetProfile.Base


def padded(padding: int) -> QuantumCircuit:
    """A circuit with two matching gates and `padding` that do not match."""
    circuit = QuantumCircuit(1)
    for _ in range(padding):
        circuit.t(0)
    circuit.x(0)
    circuit.x(0)
    return circuit


def test_the_pass_is_consulted_once_per_match_however_large_the_graph_is():
    crossings = []
    sizes = []
    for padding in (2, 20, 200):
        graph = padded(padding).to_graph()
        sizes.append(len(graph.nodes))

        seen = []
        result = graph.rewrite(["quantum.x"], lambda match: seen.append(match) or None)
        crossings.append(len(seen))
        assert result.crossings == len(seen)

    assert sizes[2] > sizes[0] * 10, f"the graph should actually grow: {sizes}"
    assert crossings == [2, 2, 2]


def test_a_match_says_what_it_is_without_handing_over_the_graph():
    seen = []
    padded(1).to_graph().rewrite(
        ["quantum.x"], lambda match: seen.append(match) or None
    )

    assert [match.operation for match in seen] == ["quantum.x", "quantum.x"]
    assert [match.operands for match in seen] == [1, 1]
    assert len({match.node for match in seen}) == 2
    assert not hasattr(seen[0], "graph")


def test_a_pass_that_replaces_reaches_the_generated_code():
    graph = padded(0).to_graph()
    result = graph.rewrite(["quantum.x"], lambda match: PassPatch.replace("quantum.y"))

    assert result.outcome == "committed"
    assert result.applications == 2

    expected = QuantumCircuit(1)
    expected.y(0)
    expected.y(0)
    assert result.graph.to_qir(BASE) == expected.to_graph().to_qir(BASE)


def test_a_pass_that_reduces_a_known_graph_leaves_a_graph_that_verifies():
    circuit = QuantumCircuit(1)
    circuit.x(0)
    circuit.h(0)
    circuit.x(0)
    result = circuit.to_graph().rewrite(["quantum.x"], lambda match: PassPatch.remove())

    assert result.outcome == "committed"
    assert result.applications == 2
    assert result.graph.diagnostics() == []

    remaining = QuantumCircuit(1)
    remaining.h(0)
    assert result.graph.to_qir(BASE) == remaining.to_graph().to_qir(BASE)


def test_a_pass_may_decline_a_match():
    graph = padded(0).to_graph()
    decided = []

    def once(match):
        decided.append(match)
        return PassPatch.replace("quantum.y") if len(decided) == 1 else None

    result = graph.rewrite(["quantum.x"], once)
    assert result.crossings == 2
    assert result.applications == 1


def test_a_budget_too_small_reports_what_it_did_not_reach():
    graph = padded(0).to_graph()
    result = graph.rewrite(
        ["quantum.x"], lambda match: PassPatch.replace("quantum.y"), budget=1
    )
    assert result.outcome == "budget_exhausted"
    assert result.applications == 1


def test_a_fragment_puts_a_chain_where_one_node_was():
    circuit = QuantumCircuit(1)
    circuit.x(0)
    circuit.t(0)
    result = circuit.to_graph().expand(
        ["quantum.x"], lambda match: ["quantum.h", "quantum.z"]
    )

    assert result.outcome == "committed"
    assert result.applications == 1
    assert result.graph.diagnostics() == []

    expected = QuantumCircuit(1)
    expected.h(0)
    expected.z(0)
    expected.t(0)
    assert result.graph.to_qir(BASE) == expected.to_graph().to_qir(BASE)


def test_a_patch_cannot_grow_the_graph_which_is_why_there_are_two_contracts():
    # The published patch surface replaces, retires, disconnects, and connects.
    # It has no way to state a node, so a decomposition needs `expand`.
    assert not hasattr(PassPatch, "insert")
    assert not hasattr(PassPatch, "expand")


def test_the_original_graph_is_untouched_by_a_pass():
    graph = padded(0).to_graph()
    before = graph.to_qir(BASE)
    graph.rewrite(["quantum.x"], lambda match: PassPatch.remove())
    assert graph.to_qir(BASE) == before


def test_removing_a_measurement_is_refused_by_name():
    circuit = QuantumCircuit(1, 1)
    circuit.measure(0, 0)
    graph = circuit.to_graph()

    with pytest.raises(ValueError, match="quantum.measure_z"):
        graph.rewrite(["quantum.measure_z"], lambda match: PassPatch.remove())


def test_an_empty_fragment_is_refused_rather_than_treated_as_a_removal():
    graph = padded(0).to_graph()
    with pytest.raises(ValueError, match="fragment cannot be empty"):
        graph.expand(["quantum.x"], lambda match: [])


def test_renaming_a_node_that_carries_a_value_is_refused_by_name():
    # The operation name is a label; the angle lives on the node's payload and
    # is what the lowering reads. A patch cannot state a new payload, so a
    # rename that was accepted would emit the old angle under a name spelling a
    # different one.
    circuit = QuantumCircuit(1)
    circuit.rx(0.25, 0)
    graph = circuit.to_graph()

    with pytest.raises(ValueError, match="constant:0.25"):
        graph.rewrite(
            ["constant:0.25"], lambda match: PassPatch.replace("constant:0.5")
        )


def test_a_pass_that_raises_does_not_leave_a_half_applied_graph():
    graph = padded(0).to_graph()
    before = graph.to_qir(BASE)

    def broken(match):
        raise RuntimeError("the pass failed")

    with pytest.raises(RuntimeError, match="the pass failed"):
        graph.rewrite(["quantum.x"], broken)
    assert graph.to_qir(BASE) == before


def test_both_contracts_can_be_called_with_named_arguments():
    # The parameter cannot be called `pass`, which is a Python keyword, so the
    # published name has to be one a caller can actually write.
    graph = padded(0).to_graph()

    rewritten = graph.rewrite(
        operations=["quantum.x"], pass_=lambda match: PassPatch.replace("quantum.y")
    )
    expanded = graph.expand(
        operations=["quantum.x"], pass_=lambda match: ["quantum.y", "quantum.y"]
    )

    assert rewritten.applications == 2
    assert expanded.applications == 2


if __name__ == "__main__":
    pytest.main([__file__])
