# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false

"""Unit tests for the Model classes."""

from __future__ import annotations

import pytest

cirq = pytest.importorskip("cirq")

from qsharp.applications.magnets import (
    HeisenbergModel,
    Hyperedge,
    Hypergraph,
    IsingModel,
    Model,
    PauliString,
)


def make_chain(length: int) -> Hypergraph:
    edges = [Hyperedge([i, i + 1]) for i in range(length - 1)]
    return Hypergraph(edges)


def make_chain_with_vertices(length: int) -> Hypergraph:
    edges = [Hyperedge([i, i + 1]) for i in range(length - 1)]
    edges.extend([Hyperedge([i]) for i in range(length)])
    return Hypergraph(edges)


class CountingColoringHypergraph(Hypergraph):
    def __init__(self, edges: list[Hyperedge]):
        super().__init__(edges)
        self.edge_coloring_calls = 0

    def edge_coloring(self):
        self.edge_coloring_calls += 1
        return super().edge_coloring()


def test_model_init_basic():
    geometry = Hypergraph([Hyperedge([0, 1]), Hyperedge([1, 2])])
    model = Model(geometry)
    assert model.geometry is geometry
    assert model.nqubits == 3
    assert model.nterms == 0
    assert model._ops == []
    assert model._terms == {}


def test_model_init_empty_geometry():
    model = Model(Hypergraph([]))
    assert model.nqubits == 0
    assert model.nterms == 0


def test_model_add_interaction_basic():
    edge = Hyperedge([0, 1])
    model = Model(Hypergraph([edge]))
    model.add_interaction(edge, "ZZ", -1.5)

    assert len(model._ops) == 1
    assert model._ops[0] == PauliString.from_qubits((0, 1), "ZZ", -1.5)
    assert model.nterms == 0


def test_model_add_interaction_with_term():
    edge = Hyperedge([0, 1])
    model = Model(Hypergraph([edge]))
    model.add_interaction(edge, "ZZ", -2.0, term=3)

    assert model.nterms == 1
    assert 3 in model._terms
    assert model._terms[3] == {0: [0]}


def test_model_term_color_query_methods():
    edge = Hyperedge([0, 1])
    model = Model(Hypergraph([edge]))
    model.add_interaction(edge, "ZZ", -1.0, term=1, color=2)
    model.add_interaction(edge, "XX", -0.5, term=1, color=2)
    model.add_interaction(edge, "YY", -0.25, term=1, color=3)

    assert model.terms == [1]
    assert model.ncolors(1) == 2
    assert set(model.colors(1)) == {2, 3}
    assert model.nops(1, 2) == 2
    assert model.nops(1, 3) == 1
    assert model.ops(1, 2) == [
        PauliString.from_qubits((0, 1), "ZZ", -1.0),
        PauliString.from_qubits((0, 1), "XX", -0.5),
    ]
    assert model.ops(1, 3) == [PauliString.from_qubits((0, 1), "YY", -0.25)]


def test_model_query_methods_raise_for_missing_term_and_color():
    edge = Hyperedge([0, 1])
    model = Model(Hypergraph([edge]))
    model.add_interaction(edge, "ZZ", -1.0, term=0, color=0)

    with pytest.raises(ValueError, match="Term 99 does not exist in the model"):
        model.ncolors(99)
    with pytest.raises(ValueError, match="Term 99 does not exist in the model"):
        model.colors(99)
    with pytest.raises(ValueError, match="Term 99 does not exist in the model"):
        model.nops(99, 0)
    with pytest.raises(ValueError, match="Term 99 does not exist in the model"):
        model.ops(99, 0)

    with pytest.raises(ValueError, match="Color 7 does not exist in term 0"):
        model.nops(0, 7)
    with pytest.raises(ValueError, match="Color 7 does not exist in term 0"):
        model.ops(0, 7)


def test_model_add_interaction_rejects_edge_not_in_geometry():
    model = Model(Hypergraph([Hyperedge([0, 1])]))
    with pytest.raises(ValueError, match="Edge is not part of the model geometry"):
        model.add_interaction(Hyperedge([1, 2]), "ZZ", -1.0)


def test_model_str_and_repr():
    model = Model(make_chain(3))
    assert "0 terms" in str(model)
    assert "3 qubits" in str(model)
    assert repr(model) == str(model)


def test_ising_model_basic():
    geometry = make_chain_with_vertices(3)
    model = IsingModel(geometry, h=1.0, J=1.0)

    assert isinstance(model, Model)
    assert model.geometry is geometry
    assert model.nterms == 2
    assert set(model._terms.keys()) == {0, 1}


def test_ising_model_str_and_repr():
    geometry = make_chain_with_vertices(3)
    model = IsingModel(geometry, h=0.5, J=2.0)

    assert str(model) == "Ising model with 2 terms on 3 qubits (h=0.5, J=2.0)."
    assert repr(model) == "IsingModel(nqubits=3, nterms=2, h=0.5, J=2.0)"


def test_heisenberg_model_str_and_repr():
    geometry = make_chain(3)
    model = HeisenbergModel(geometry, J=1.5)

    assert str(model) == "Heisenberg model with 3 terms on 3 qubits (J=1.5)."
    assert repr(model) == "HeisenbergModel(nqubits=3, nterms=3, J=1.5)"


def test_ising_model_coloring_matches_geometry_coloring():
    geometry = make_chain_with_vertices(4)
    model = IsingModel(geometry, h=1.0, J=1.0)
    geometry_coloring = geometry.edge_coloring()

    for color, indices in model._terms[1].items():
        for index in indices:
            op = model._ops[index]
            edge_vertices = tuple(sorted(op.qubits))
            assert geometry_coloring.color(edge_vertices) == color


def test_ising_model_initialization_calls_geometry_edge_coloring_once():
    geometry = CountingColoringHypergraph(
        [
            Hyperedge([0, 1]),
            Hyperedge([1, 2]),
            Hyperedge([0]),
            Hyperedge([1]),
            Hyperedge([2]),
        ]
    )

    model = IsingModel(geometry, h=1.0, J=1.0)

    assert isinstance(model, IsingModel)
    assert geometry.edge_coloring_calls == 1


def test_ising_model_coefficients_and_paulis():
    geometry = make_chain_with_vertices(3)
    model = IsingModel(geometry, h=0.5, J=2.0)

    ops_by_qubits = {tuple(sorted(op.qubits)): op for op in model._ops}

    assert ops_by_qubits[(0, 1)] == PauliString.from_qubits((0, 1), "ZZ", -2.0)
    assert ops_by_qubits[(1, 2)] == PauliString.from_qubits((1, 2), "ZZ", -2.0)
    assert ops_by_qubits[(0,)] == PauliString.from_qubits((0,), "X", -0.5)
    assert ops_by_qubits[(1,)] == PauliString.from_qubits((1,), "X", -0.5)
    assert ops_by_qubits[(2,)] == PauliString.from_qubits((2,), "X", -0.5)


def test_ising_model_term_grouping_indices():
    geometry = make_chain_with_vertices(4)
    model = IsingModel(geometry, h=1.0, J=1.0)

    assert set(model._terms.keys()) == {0, 1}
    assert all(
        len(model._ops[index].qubits) == 1
        for indices in model._terms[0].values()
        for index in indices
    )
    assert all(
        len(model._ops[index].qubits) == 2
        for indices in model._terms[1].values()
        for index in indices
    )


def test_heisenberg_model_basic():
    geometry = make_chain(3)
    model = HeisenbergModel(geometry, J=1.0)

    assert isinstance(model, Model)
    assert model.geometry is geometry
    assert model.nterms == 3
    assert set(model._terms.keys()) == {0, 1, 2}


def test_heisenberg_model_coefficients_and_paulis():
    geometry = make_chain(3)
    model = HeisenbergModel(geometry, J=2.5)

    expected = [
        PauliString.from_qubits((0, 1), "XX", -2.5),
        PauliString.from_qubits((1, 2), "XX", -2.5),
        PauliString.from_qubits((0, 1), "YY", -2.5),
        PauliString.from_qubits((1, 2), "YY", -2.5),
        PauliString.from_qubits((0, 1), "ZZ", -2.5),
        PauliString.from_qubits((1, 2), "ZZ", -2.5),
    ]
    for pauli in expected:
        assert pauli in model._ops


def test_heisenberg_model_term_grouping_colors_and_paulis():
    geometry = make_chain(4)
    model = HeisenbergModel(geometry, J=1.0)

    paulis_by_term = {0: "XX", 1: "YY", 2: "ZZ"}
    for term, pauli in paulis_by_term.items():
        for color, indices in model._terms[term].items():
            for index in indices:
                op = model._ops[index]
                expected = PauliString.from_qubits(
                    tuple(sorted(op.qubits)), pauli, -1.0
                )
                assert op == expected
                assert model.coloring.color(tuple(sorted(op.qubits))) == color
