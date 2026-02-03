# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false, reportOperatorIssue=false

"""Unit tests for the Model class."""

# To be updated after additional geometries are implemented

from __future__ import annotations

import pytest
from . import CIRQ_AVAILABLE, SKIP_REASON

if CIRQ_AVAILABLE:
    import cirq
    from cirq import LineQubit

    from qsharp.magnets.geometry import Hyperedge, Hypergraph
    from qsharp.magnets.models import Model


def make_chain(length: int) -> Hypergraph:
    """Create a simple chain hypergraph for testing."""
    edges = [Hyperedge([i, i + 1]) for i in range(length - 1)]
    return Hypergraph(edges)


# Model initialization tests


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_init_basic():
    """Test basic Model initialization."""
    geometry = Hypergraph([Hyperedge([0, 1]), Hyperedge([1, 2])])
    model = Model(geometry)
    assert model.geometry is geometry
    assert len(model.terms) == 0


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_init_creates_qubits():
    """Test that Model creates correct number of qubits."""
    geometry = Hypergraph([Hyperedge([0, 1]), Hyperedge([2, 3])])
    model = Model(geometry)
    assert len(model.qubit_list()) == 4


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_init_with_chain():
    """Test Model initialization with chain geometry."""
    geometry = make_chain(5)
    model = Model(geometry)
    assert len(model.qubit_list()) == 5


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_init_empty_geometry():
    """Test Model with empty geometry."""
    geometry = Hypergraph([])
    model = Model(geometry)
    assert len(model.qubit_list()) == 0
    assert len(model.terms) == 0


# Qubit access tests


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_q_returns_line_qubit():
    """Test that q() returns LineQubit instances."""
    geometry = make_chain(3)
    model = Model(geometry)
    qubit = model.q(0)
    assert isinstance(qubit, LineQubit)


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_q_returns_correct_qubit():
    """Test that q() returns qubit with correct index."""
    geometry = make_chain(4)
    model = Model(geometry)
    for i in range(4):
        assert model.q(i) == LineQubit(i)


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_qubit_list():
    """Test qubit_list() returns all qubits."""
    geometry = make_chain(3)
    model = Model(geometry)
    qubits = model.qubit_list()
    assert len(qubits) == 3
    assert qubits == [LineQubit(0), LineQubit(1), LineQubit(2)]


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_qubits_iterator():
    """Test qubits() returns an iterator."""
    geometry = make_chain(3)
    model = Model(geometry)
    qubit_iter = model.qubits()
    qubits = list(qubit_iter)
    assert len(qubits) == 3
    assert qubits == [LineQubit(0), LineQubit(1), LineQubit(2)]


# Term management tests


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_add_term_empty():
    """Test adding an empty term."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.add_term()
    assert len(model.terms) == 1


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_add_term_with_pauli_sum():
    """Test adding a PauliSum term."""
    geometry = make_chain(2)
    model = Model(geometry)
    q0, q1 = model.q(0), model.q(1)
    term = cirq.Z(q0) * cirq.Z(q1)
    model.add_term(cirq.PauliSum.from_pauli_strings([term]))
    assert len(model.terms) == 1


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_add_multiple_terms():
    """Test adding multiple terms."""
    geometry = make_chain(3)
    model = Model(geometry)
    model.add_term()
    model.add_term()
    model.add_term()
    assert len(model.terms) == 3


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_add_to_term():
    """Test adding a PauliString to an existing term."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.add_term()
    q0, q1 = model.q(0), model.q(1)
    pauli_string = cirq.Z(q0) * cirq.Z(q1)
    model.add_to_term(0, pauli_string)
    # Term should now contain the Pauli string
    assert len(model.terms[0]) == 1


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_add_to_term_multiple_strings():
    """Test adding multiple PauliStrings to the same term."""
    geometry = make_chain(3)
    model = Model(geometry)
    model.add_term()
    q0, q1, q2 = model.q(0), model.q(1), model.q(2)
    model.add_to_term(0, cirq.Z(q0) * cirq.Z(q1))
    model.add_to_term(0, cirq.Z(q1) * cirq.Z(q2))
    assert len(model.terms[0]) == 2


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_add_to_different_terms():
    """Test adding PauliStrings to different terms."""
    geometry = make_chain(3)
    model = Model(geometry)
    model.add_term()
    model.add_term()
    q0, q1, q2 = model.q(0), model.q(1), model.q(2)
    model.add_to_term(0, cirq.Z(q0) * cirq.Z(q1))
    model.add_to_term(1, cirq.Z(q1) * cirq.Z(q2))
    assert len(model.terms[0]) == 1
    assert len(model.terms[1]) == 1


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_add_to_term_with_coefficient():
    """Test adding a PauliString with a coefficient."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.add_term()
    q0, q1 = model.q(0), model.q(1)
    pauli_string = 0.5 * cirq.Z(q0) * cirq.Z(q1)
    model.add_to_term(0, pauli_string)
    assert len(model.terms[0]) == 1


# String representation tests


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_str():
    """Test string representation."""
    geometry = make_chain(4)
    model = Model(geometry)
    model.add_term()
    model.add_term()
    result = str(model)
    assert "2 terms" in result
    assert "4 qubits" in result


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_str_empty():
    """Test string representation with no terms."""
    geometry = make_chain(3)
    model = Model(geometry)
    result = str(model)
    assert "0 terms" in result
    assert "3 qubits" in result


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_repr():
    """Test repr representation."""
    geometry = make_chain(2)
    model = Model(geometry)
    assert repr(model) == str(model)


# Integration tests


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_build_simple_hamiltonian():
    """Test building a simple ZZ Hamiltonian on a chain."""
    geometry = make_chain(3)
    model = Model(geometry)
    model.add_term()  # Single term for all interactions

    for edge in geometry.edges():
        i, j = edge.vertices
        model.add_to_term(0, cirq.Z(model.q(i)) * cirq.Z(model.q(j)))

    # Should have 2 ZZ interactions: (0,1) and (1,2)
    assert len(model.terms[0]) == 2


@pytest.mark.skipif(not CIRQ_AVAILABLE, reason=SKIP_REASON)
def test_model_with_partitioned_terms():
    """Test building a model with partitioned terms for Trotterization."""
    geometry = make_chain(4)
    model = Model(geometry)

    # Add two terms for even/odd partitioning
    model.add_term()  # Even edges: (0,1), (2,3)
    model.add_term()  # Odd edges: (1,2)

    # Add even edges to term 0
    model.add_to_term(0, cirq.Z(model.q(0)) * cirq.Z(model.q(1)))
    model.add_to_term(0, cirq.Z(model.q(2)) * cirq.Z(model.q(3)))

    # Add odd edge to term 1
    model.add_to_term(1, cirq.Z(model.q(1)) * cirq.Z(model.q(2)))

    assert len(model.terms) == 2
    assert len(model.terms[0]) == 2
    assert len(model.terms[1]) == 1
