# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false

"""Unit tests for the Model class."""

from __future__ import annotations

import pytest

from qsharp.magnets.geometry import Hyperedge, Hypergraph
from qsharp.magnets.models import Model
from qsharp.magnets.utilities import PauliString


def make_chain(length: int) -> Hypergraph:
    """Create a simple chain hypergraph for testing."""
    edges = [Hyperedge([i, i + 1]) for i in range(length - 1)]
    return Hypergraph(edges)


def make_chain_with_vertices(length: int) -> Hypergraph:
    """Create a chain hypergraph with single-vertex (field) edges for testing."""
    edges = [Hyperedge([i, i + 1]) for i in range(length - 1)]
    # Add single-vertex edges for field terms
    edges.extend([Hyperedge([i]) for i in range(length)])
    return Hypergraph(edges)


# Model initialization tests


def test_model_init_basic():
    """Test basic Model initialization."""
    geometry = Hypergraph([Hyperedge([0, 1]), Hyperedge([1, 2])])
    model = Model(geometry)
    assert model.geometry is geometry
    assert len(model.terms()) == 0


def test_model_init_with_chain():
    """Test Model initialization with chain geometry."""
    geometry = make_chain(5)
    model = Model(geometry)
    assert len(model._qubits) == 5


def test_model_init_empty_geometry():
    """Test Model with empty geometry."""
    geometry = Hypergraph([])
    model = Model(geometry)
    assert len(model._qubits) == 0
    assert len(model.terms()) == 0


def test_model_init_coefficients_zero():
    """Test that coefficients are initialized to zero."""
    geometry = make_chain(3)  # edges: (0,1), (1,2)
    model = Model(geometry)
    assert model.get_coefficient((0, 1)) == 0.0
    assert model.get_coefficient((1, 2)) == 0.0


def test_model_init_pauli_strings_identity():
    """Test that PauliStrings are initialized to identity."""
    geometry = make_chain(3)  # edges: (0,1), (1,2)
    model = Model(geometry)
    assert model.get_pauli_string((0, 1)) == PauliString.from_qubits((0, 1), "II")
    assert model.get_pauli_string((1, 2)) == PauliString.from_qubits((1, 2), "II")


# Coefficient tests


def test_model_set_coefficient():
    """Test setting coefficient for an edge."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_coefficient((0, 1), 1.5)
    assert model.get_coefficient((0, 1)) == 1.5


def test_model_set_coefficient_overwrite():
    """Test overwriting an existing coefficient."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_coefficient((0, 1), 1.5)
    model.set_coefficient((0, 1), 2.5)
    assert model.get_coefficient((0, 1)) == 2.5


def test_model_set_coefficient_invalid_edge():
    """Test setting coefficient for non-existent edge raises error."""
    geometry = make_chain(2)
    model = Model(geometry)
    with pytest.raises(KeyError):
        model.set_coefficient((0, 2), 1.0)


def test_model_get_coefficient_invalid_edge():
    """Test getting coefficient for non-existent edge raises error."""
    geometry = make_chain(2)
    model = Model(geometry)
    with pytest.raises(KeyError):
        model.get_coefficient((0, 2))


def test_model_get_coefficient_sorted():
    """Test that get_coefficient sorts vertices so order doesn't matter."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_coefficient((0, 1), 3.0)
    assert model.get_coefficient((1, 0)) == 3.0


def test_model_set_coefficient_sorted():
    """Test that set_coefficient sorts vertices so order doesn't matter."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_coefficient((1, 0), 4.0)
    assert model.get_coefficient((0, 1)) == 4.0


def test_model_set_coefficient_preserves_pauli_string():
    """Test that set_coefficient does not change the PauliString."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_pauli_string((0, 1), PauliString.from_qubits((0, 1), "ZZ"))
    model.set_coefficient((0, 1), 3.0)
    assert model.get_pauli_string((0, 1)) == PauliString.from_qubits((0, 1), "ZZ")


# PauliString tests


def test_model_set_pauli_string():
    """Test setting PauliString for an edge."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_pauli_string((0, 1), PauliString.from_qubits((0, 1), "ZZ"))
    assert model.get_pauli_string((0, 1)) == PauliString.from_qubits((0, 1), "ZZ")


def test_model_set_pauli_string_overwrite():
    """Test overwriting an existing PauliString."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_pauli_string((0, 1), PauliString.from_qubits((0, 1), "ZZ"))
    model.set_pauli_string((0, 1), PauliString.from_qubits((0, 1), "XX"))
    assert model.get_pauli_string((0, 1)) == PauliString.from_qubits((0, 1), "XX")


def test_model_set_pauli_string_invalid_edge():
    """Test setting PauliString for non-existent edge raises error."""
    geometry = make_chain(2)
    model = Model(geometry)
    with pytest.raises(KeyError):
        model.set_pauli_string((0, 2), PauliString.from_qubits((0, 2), "ZZ"))


def test_model_get_pauli_string_invalid_edge():
    """Test getting PauliString for non-existent edge raises error."""
    geometry = make_chain(2)
    model = Model(geometry)
    with pytest.raises(KeyError):
        model.get_pauli_string((0, 2))


def test_model_set_pauli_string_preserves_coefficient():
    """Test that set_pauli_string does not change the coefficient."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_coefficient((0, 1), 5.0)
    model.set_pauli_string((0, 1), PauliString.from_qubits((0, 1), "ZZ"))
    assert model.get_coefficient((0, 1)) == 5.0


def test_model_set_pauli_string_sorted():
    """Test that set_pauli_string sorts vertices so order doesn't matter."""
    geometry = make_chain(2)
    model = Model(geometry)
    model.set_pauli_string((1, 0), PauliString.from_qubits((1, 0), "XZ"))
    assert model.get_pauli_string((0, 1)) == PauliString.from_qubits((1, 0), "XZ")


# has_interaction_term tests


def test_model_has_interaction_term_true():
    """Test has_interaction_term returns True for existing edge."""
    geometry = make_chain(3)
    model = Model(geometry)
    assert model.has_interaction_term((0, 1)) is True
    assert model.has_interaction_term((1, 2)) is True


def test_model_has_interaction_term_false():
    """Test has_interaction_term returns False for non-existent edge."""
    geometry = make_chain(3)
    model = Model(geometry)
    assert model.has_interaction_term((0, 2)) is False
    assert model.has_interaction_term((5, 6)) is False


def test_model_has_interaction_term_sorted():
    """Test has_interaction_term sorts vertices so order doesn't matter."""
    geometry = make_chain(2)
    model = Model(geometry)
    assert model.has_interaction_term((1, 0)) is True


# Term management tests


def test_model_add_term():
    """Test adding a term with edges."""
    geometry = make_chain(3)
    model = Model(geometry)
    edge1 = Hyperedge([0, 1])
    edge2 = Hyperedge([1, 2])
    model.add_term([edge1, edge2])
    assert len(model.terms()) == 1
    assert len(model.terms()[0]) == 2


def test_model_add_multiple_terms():
    """Test adding multiple terms."""
    geometry = make_chain(4)
    model = Model(geometry)
    model.add_term([Hyperedge([0, 1])])
    model.add_term([Hyperedge([1, 2]), Hyperedge([2, 3])])
    assert len(model.terms()) == 2


# String representation tests


def test_model_str():
    """Test string representation."""
    geometry = make_chain(4)
    model = Model(geometry)
    model.add_term([Hyperedge([0, 1])])
    model.add_term([Hyperedge([1, 2])])
    result = str(model)
    assert "2 terms" in result
    assert "4 qubits" in result


def test_model_str_empty():
    """Test string representation with no terms."""
    geometry = make_chain(3)
    model = Model(geometry)
    result = str(model)
    assert "0 terms" in result
    assert "3 qubits" in result


def test_model_repr():
    """Test repr representation."""
    geometry = make_chain(2)
    model = Model(geometry)
    assert repr(model) == str(model)


# Integration tests


def test_model_build_simple_hamiltonian():
    """Test building a simple ZZ Hamiltonian on a chain."""
    geometry = make_chain(3)
    model = Model(geometry)

    # Set coefficients for all edges
    for edge in geometry.edges():
        model.set_coefficient(edge.vertices, 1.0)

    # Verify coefficients
    assert model.get_coefficient((0, 1)) == 1.0
    assert model.get_coefficient((1, 2)) == 1.0


def test_model_with_partitioned_terms():
    """Test building a model with partitioned terms for Trotterization."""
    geometry = make_chain(4)
    model = Model(geometry)

    # Add two terms for even/odd partitioning
    even_edges = [Hyperedge([0, 1]), Hyperedge([2, 3])]
    odd_edges = [Hyperedge([1, 2])]
    model.add_term(even_edges)
    model.add_term(odd_edges)

    assert len(model.terms()) == 2
    assert len(model.terms()[0]) == 2
    assert len(model.terms()[1]) == 1


# translation_invariant_ising_model tests


def test_translation_invariant_ising_model_basic():
    """Test basic creation of Ising model."""
    from qsharp.magnets.models import translation_invariant_ising_model

    geometry = make_chain_with_vertices(3)
    model = translation_invariant_ising_model(geometry, h=1.0, J=1.0)

    assert isinstance(model, Model)
    assert model.geometry is geometry


def test_translation_invariant_ising_model_zz_coefficients():
    """Test that ZZ interaction coefficients are correctly set."""
    from qsharp.magnets.models import translation_invariant_ising_model

    geometry = make_chain_with_vertices(4)  # 3 two-body edges: (0,1), (1,2), (2,3)
    J = 2.0
    model = translation_invariant_ising_model(geometry, h=0.5, J=J)

    # All two-body edge coefficients should be -J
    assert model.get_coefficient((0, 1)) == -J
    assert model.get_coefficient((1, 2)) == -J
    assert model.get_coefficient((2, 3)) == -J


def test_translation_invariant_ising_model_x_coefficients():
    """Test that X field coefficients are correctly set."""
    from qsharp.magnets.models import translation_invariant_ising_model

    geometry = make_chain_with_vertices(4)  # 4 single-vertex edges
    h = 0.5
    model = translation_invariant_ising_model(geometry, h=h, J=2.0)

    # All single-vertex edge coefficients should be -h
    for v in range(4):
        assert model.get_coefficient((v,)) == -h


def test_translation_invariant_ising_model_coefficients():
    """Test that coefficients are correctly applied."""
    from qsharp.magnets.models import translation_invariant_ising_model

    # Geometry with one two-body edge and two single-vertex edges
    geometry = Hypergraph([Hyperedge([0, 1]), Hyperedge([0]), Hyperedge([1])])
    h, J = 0.3, 0.7
    model = translation_invariant_ising_model(geometry, h=h, J=J)

    # Check ZZ coefficient is -J
    assert model.get_coefficient((0, 1)) == -J

    # Check X coefficients are -h
    assert model.get_coefficient((0,)) == -h
    assert model.get_coefficient((1,)) == -h


def test_translation_invariant_ising_model_zero_field():
    """Test Ising model with zero transverse field."""
    from qsharp.magnets.models import translation_invariant_ising_model

    geometry = make_chain_with_vertices(3)
    model = translation_invariant_ising_model(geometry, h=0.0, J=1.0)

    # X coefficients (single-vertex edges) should all be zero
    for v in range(3):
        assert model.get_coefficient((v,)) == 0.0


def test_translation_invariant_ising_model_zero_coupling():
    """Test Ising model with zero coupling."""
    from qsharp.magnets.models import translation_invariant_ising_model

    geometry = make_chain_with_vertices(3)
    model = translation_invariant_ising_model(geometry, h=1.0, J=0.0)

    # ZZ coefficients (two-body edges) should all be zero
    assert model.get_coefficient((0, 1)) == 0.0
    assert model.get_coefficient((1, 2)) == 0.0


def test_translation_invariant_ising_model_term_grouping():
    """Test that Ising model has correct term grouping by color."""
    from qsharp.magnets.models import translation_invariant_ising_model

    geometry = make_chain_with_vertices(4)
    model = translation_invariant_ising_model(geometry, h=1.0, J=1.0)

    # Number of terms should be ncolors + 1
    assert len(model.terms()) == geometry.ncolors + 1


def test_translation_invariant_ising_model_pauli_strings():
    """Test that Ising model sets correct PauliStrings."""
    from qsharp.magnets.models import translation_invariant_ising_model

    geometry = make_chain_with_vertices(3)
    model = translation_invariant_ising_model(geometry, h=1.0, J=1.0)

    # Two-body edges should have ZZ PauliString
    assert model.get_pauli_string((0, 1)) == PauliString.from_qubits((0, 1), "ZZ")
    assert model.get_pauli_string((1, 2)) == PauliString.from_qubits((1, 2), "ZZ")

    # Single-vertex edges should have X PauliString
    for v in range(3):
        assert model.get_pauli_string((v,)) == PauliString.from_qubits((v,), "X")
