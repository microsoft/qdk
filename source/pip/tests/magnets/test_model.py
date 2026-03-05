# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false

"""Unit tests for the Model classes."""

from __future__ import annotations

import pytest

from qsharp.magnets.models import IsingModel, Model
from qsharp.magnets.utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
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
    assert model._terms[3] == [0]


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
    assert isinstance(model.coloring, HypergraphEdgeColoring)


def test_ising_model_coloring_matches_geometry_coloring():
    geometry = make_chain_with_vertices(4)
    model = IsingModel(geometry, h=1.0, J=1.0)
    geometry_coloring = geometry.edge_coloring()

    for edge in geometry.edges():
        assert model.coloring.color(edge) == geometry_coloring.color(edge)


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

    assert isinstance(model.coloring, HypergraphEdgeColoring)
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
    assert all(len(model._ops[index].qubits) == 1 for index in model._terms[0])
    assert all(len(model._ops[index].qubits) == 2 for index in model._terms[1])
