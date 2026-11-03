# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the pass pipeline.

Two things are pinned here that the pieces underneath could not claim for
themselves. That every published anchor is a place a pass can actually run,
shown by registering one at each and watching its change reach the output. And
that a pass the compiler ships and a pass a caller writes are the same kind of
thing: registered the same way, listed together, and run in one declared order.
"""

import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import (  # noqa: E402
    PassAnchor,
    PassPatch,
    PassPipeline,
    QuantumCircuit,
)

BUILT_IN = "cancel-inverse-pairs"
COLLAPSE = "retire-diagonal-before-z-collapse"
BUILT_INS = [BUILT_IN, COLLAPSE]


def cancellable() -> QuantumCircuit:
    """A circuit with an adjacent inverse pair and one gate that survives it."""
    circuit = QuantumCircuit(1)
    circuit.h(0)
    circuit.h(0)
    circuit.x(0)
    return circuit


def exported(gates: list[str]) -> str:
    circuit = QuantumCircuit(1)
    for gate in gates:
        getattr(circuit, gate)(0)
    return circuit.to_qasm()


def test_the_compilers_own_pass_is_listed_at_a_named_anchor():
    listing = PassPipeline().listing()

    assert [entry.name for entry in listing] == BUILT_INS
    assert all(
        entry.anchor == PassAnchor.GraphRewritten and entry.built_in
        for entry in listing
    )


def test_the_compilers_own_pass_changes_the_program():
    assert PassPipeline().run(cancellable()).circuit.to_qasm() == exported(["x"])
    assert PassPipeline(built_ins=False).run(
        cancellable()
    ).circuit.to_qasm() == exported(["h", "h", "x"])


def test_every_anchor_runs_a_pass_and_every_change_reaches_the_output():
    pipeline = PassPipeline()

    def append_y(circuit):
        circuit.y(0)
        return None

    def append_z(circuit):
        circuit.z(0)
        return circuit

    pipeline.add_circuit_pass(PassAnchor.CircuitBuilt, "append-y", append_y)
    pipeline.add_rewrite_pass(
        PassAnchor.GraphLowered,
        "x-becomes-s",
        ["quantum.x"],
        lambda match: PassPatch.replace("quantum.s"),
    )
    pipeline.add_rewrite_pass(
        PassAnchor.GraphRewritten,
        "s-becomes-t",
        ["quantum.s"],
        lambda match: PassPatch.replace("quantum.t"),
    )
    pipeline.add_circuit_pass(PassAnchor.CircuitRaised, "append-z", append_z)

    result = pipeline.run(cancellable())

    assert [(entry.anchor.name, entry.name) for entry in result.executed] == [
        ("circuit_built", "append-y"),
        ("graph_lowered", "x-becomes-s"),
        ("graph_rewritten", BUILT_IN),
        ("graph_rewritten", COLLAPSE),
        ("graph_rewritten", "s-becomes-t"),
        ("circuit_raised", "append-z"),
    ]
    assert [entry.name for entry in result.executed] == [
        entry.name for entry in pipeline.listing()
    ]
    assert sorted({entry.anchor.name for entry in result.executed}) == sorted(
        anchor.name for anchor in PassAnchor.all()
    ), "every published anchor ran a pass"
    assert result.circuit.to_qasm() == exported(["t", "y", "z"])


def test_a_caller_pass_at_the_same_anchor_sees_what_the_built_in_left():
    seen = []
    pipeline = PassPipeline()
    pipeline.add_rewrite_pass(
        PassAnchor.GraphRewritten,
        "observe",
        ["quantum.h", "quantum.x"],
        lambda match: seen.append(match.operation) or None,
    )
    pipeline.run(cancellable())

    assert seen == ["quantum.x"], "the built-in ran first, so the pair is already gone"


def test_a_pass_cannot_be_registered_where_its_program_does_not_exist():
    pipeline = PassPipeline()

    with pytest.raises(ValueError, match="circuit_built.*circuit anchor.*graph pass"):
        pipeline.add_rewrite_pass(
            PassAnchor.CircuitBuilt, "nowhere", ["quantum.x"], lambda match: None
        )
    with pytest.raises(ValueError, match="graph_rewritten.*graph anchor.*circuit pass"):
        pipeline.add_circuit_pass(
            PassAnchor.GraphRewritten, "nowhere", lambda circuit: None
        )

    assert [entry.name for entry in pipeline.listing()] == BUILT_INS


def test_the_pipeline_consults_a_pass_once_per_match_however_large_the_graph_is():
    crossings = []
    sizes = []
    for padding in (2, 20, 200):
        circuit = QuantumCircuit(1)
        for _ in range(padding):
            circuit.t(0)
        circuit.x(0)
        circuit.x(0)

        pipeline = PassPipeline(built_ins=False)
        pipeline.add_rewrite_pass(
            PassAnchor.GraphRewritten, "count", ["quantum.x"], lambda match: None
        )
        result = pipeline.run(circuit)
        crossings.append(result.crossings)
        sizes.append(len(result.graph.nodes))

    assert sizes[2] > sizes[0] * 10, f"the graph should actually grow: {sizes}"
    assert crossings == [2, 2, 2]


def test_a_pass_that_raises_reaches_the_caller_as_itself():
    class Rejected(Exception):
        pass

    def refuse(match):
        raise Rejected("not this one")

    pipeline = PassPipeline()
    pipeline.add_rewrite_pass(
        PassAnchor.GraphRewritten, "refuse", ["quantum.x"], refuse
    )

    with pytest.raises(Rejected, match="not this one"):
        pipeline.run(cancellable())


def test_the_circuit_handed_in_is_not_the_circuit_that_comes_out():
    circuit = cancellable()
    before = circuit.to_qasm()

    pipeline = PassPipeline()
    pipeline.add_circuit_pass(
        PassAnchor.CircuitBuilt, "append-y", lambda handed: handed.y(0)
    )
    result = pipeline.run(circuit)

    assert circuit.to_qasm() == before, "the pipeline does not edit its input"
    assert result.circuit.to_qasm() != before


def test_a_fragment_returning_pass_runs_in_the_pipeline():
    pipeline = PassPipeline(built_ins=False)
    pipeline.add_expand_pass(
        PassAnchor.GraphRewritten,
        "x-becomes-hzh",
        ["quantum.x"],
        lambda match: ["quantum.h", "quantum.z", "quantum.h"],
    )
    result = pipeline.run(cancellable())

    assert result.circuit.to_qasm() == exported(["h", "h", "h", "z", "h"])


def test_a_pipeline_can_only_be_added_to():
    pipeline = PassPipeline()

    # Qiskit's staged manager is mutable at stage granularity and immutable at
    # pass granularity. This is the same shape: an anchor accepts more work, and
    # nothing reorders, replaces, or removes what is already registered.
    for absent in ("remove", "replace", "insert", "clear", "reorder", "pop"):
        assert not hasattr(pipeline, absent)

    assert sorted(name for name in dir(pipeline) if not name.startswith("_")) == [
        "add_circuit_pass",
        "add_expand_pass",
        "add_rewrite_pass",
        "listing",
        "run",
    ]


def test_a_graph_the_raise_cannot_express_is_refused_by_name():
    pipeline = PassPipeline(built_ins=False)
    pipeline.add_rewrite_pass(
        PassAnchor.GraphLowered,
        "leave-the-vocabulary",
        ["quantum.h"],
        lambda match: PassPatch.replace("vendor.mystery"),
    )

    with pytest.raises(ValueError, match="vendor.mystery"):
        pipeline.run(cancellable())


PERMUTED = [
    ("cx", (0, 1), (1, 0)),
    ("cy", (0, 1), (1, 0)),
    ("ccx", (0, 1, 2), (0, 2, 1)),
    ("cswap", (0, 1, 2), (1, 0, 2)),
]


def paired(gate: str, first: tuple, second: tuple) -> QuantumCircuit:
    circuit = QuantumCircuit(3)
    for operands in (first, second):
        getattr(circuit, gate)(*operands)
    return circuit


@pytest.mark.parametrize(("gate", "first", "second"), PERMUTED)
def test_the_built_in_leaves_a_permuted_pair_alone(gate, first, second):
    # The two halves hold the same qubits in a different order, which is not the
    # identity. Cancelling it would change what the program computes with no
    # diagnostic, which is the worst failure this surface can have.
    result = PassPipeline().run(paired(gate, first, second))

    assert result.circuit.to_qasm() == paired(gate, first, second).to_qasm()


@pytest.mark.parametrize(("gate", "first", "_second"), PERMUTED)
def test_the_built_in_still_cancels_an_aligned_pair(gate, first, _second):
    result = PassPipeline().run(paired(gate, first, first))

    assert result.circuit.to_qasm() == QuantumCircuit(3).to_qasm()


def test_every_pass_argument_can_be_named():
    # The parameter cannot be called `pass`, which is a Python keyword, so a
    # caller who writes it out has to be given a name that parses.
    pipeline = PassPipeline(built_ins=False)
    pipeline.add_circuit_pass(
        anchor=PassAnchor.CircuitBuilt, name="a", pass_=lambda circuit: None
    )
    pipeline.add_rewrite_pass(
        anchor=PassAnchor.GraphRewritten,
        name="b",
        operations=["quantum.x"],
        pass_=lambda match: None,
    )
    pipeline.add_expand_pass(
        anchor=PassAnchor.GraphLowered,
        name="c",
        operations=["quantum.h"],
        pass_=lambda match: None,
    )

    assert [entry.name for entry in pipeline.listing()] == ["a", "c", "b"]
    assert pipeline.run(cancellable()).circuit.to_qasm() == exported(["h", "h", "x"])


def test_a_run_reports_which_passes_changed_the_program():
    # The pipeline never compares two programs to find this out. Each pass says
    # so, which is what makes an iteration group's stopping condition writable.
    result = PassPipeline().run(cancellable())
    assert result.changed == [BUILT_IN]
    assert result.circuit.to_qasm() == exported(["x"])

    unchanged = PassPipeline().run(QuantumCircuit(1))
    assert unchanged.changed == []


def test_a_diagnostic_from_a_pass_reaches_the_caller():
    circuit = QuantumCircuit(1)
    circuit.t(0)
    circuit.t(0)
    result = PassPipeline().run(circuit)

    assert result.diagnostics == [
        (BUILT_IN, "no operation in this graph is self-inverse")
    ]
    assert result.changed == []


def test_the_optimization_level_reaches_every_pass_from_the_pipeline():
    # Configuration used to be able to enter a pass only through its own
    # constructor, so two passes could not agree about it.
    declined = PassPipeline(optimization_level="none").run(cancellable())
    assert declined.changed == []
    assert declined.circuit.to_qasm() == exported(["h", "h", "x"])

    assert PassPipeline(optimization_level="aggressive").run(
        cancellable()
    ).circuit.to_qasm() == exported(["x"])

    with pytest.raises(ValueError, match="unknown optimization level"):
        PassPipeline(optimization_level="turbo")
