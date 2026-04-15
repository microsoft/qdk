# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for Pauli and PauliString utilities."""

import pytest
from qsharp.qre.application.magnets import Pauli, PauliString, PauliX, PauliY, PauliZ

cirq = pytest.importorskip("cirq")


def test_pauli_init_from_int_and_string():
    """Test Pauli initialization from int and case-insensitive string labels."""
    p_i = Pauli(0, qubit=1)
    p_x = Pauli("x", qubit=2)
    p_z = Pauli(2, qubit=3)
    p_y = Pauli("Y", qubit=4)

    assert p_i.op == 0 and p_i.qubit == 1
    assert p_x.op == 1 and p_x.qubit == 2
    assert p_z.op == 2 and p_z.qubit == 3
    assert p_y.op == 3 and p_y.qubit == 4


@pytest.mark.parametrize("value", [-1, 4, 42])
def test_pauli_invalid_int_raises(value: int):
    """Test invalid integer Pauli identifiers raise ValueError."""
    with pytest.raises(ValueError, match="Integer value must be 0-3"):
        Pauli(value)


def test_pauli_invalid_string_raises():
    """Test invalid string Pauli identifiers raise ValueError."""
    with pytest.raises(ValueError, match="String value must be one of"):
        Pauli("A")


def test_pauli_invalid_type_raises():
    """Test non-int/non-str Pauli identifiers raise ValueError."""
    with pytest.raises(ValueError, match="Expected int or str"):
        Pauli(1.5)  # type: ignore


def test_pauli_helpers_create_expected_operator():
    """Test PauliX/PauliY/PauliZ helper constructors."""
    assert PauliX(0) == Pauli("X", 0)
    assert PauliY(1) == Pauli("Y", 1)
    assert PauliZ(2) == Pauli("Z", 2)


def test_pauli_cirq_property_returns_operation_on_line_qubit():
    """Test Pauli.cirq returns a Cirq operation on the target qubit."""
    q = cirq.LineQubit(3)
    assert Pauli("I", 3).cirq == cirq.I.on(q)
    assert Pauli("X", 3).cirq == cirq.X.on(q)
    assert Pauli("Y", 3).cirq == cirq.Y.on(q)
    assert Pauli("Z", 3).cirq == cirq.Z.on(q)


def test_pauli_string_init_requires_pauli_instances():
    """Test PauliString initializer validates element types."""
    with pytest.raises(TypeError, match="Expected Pauli instance"):
        PauliString([PauliX(0), "Z"])  # type: ignore


def test_pauli_string_from_qubits_accepts_string_and_int_values():
    """Test PauliString.from_qubits accepts both string and int identifiers."""
    from_string = PauliString.from_qubits((0, 1, 2), "XZY", coefficient=-1j)
    from_ints = PauliString.from_qubits((0, 1, 2), [1, 2, 3], coefficient=-1j)

    assert from_string == from_ints
    assert len(from_string) == 3
    assert from_string.qubits == (0, 1, 2)


def test_pauli_string_from_qubits_length_mismatch_raises():
    """Test from_qubits raises when qubit/value lengths differ."""
    with pytest.raises(ValueError, match="Length mismatch"):
        PauliString.from_qubits((0, 1), "XYZ")


def test_pauli_string_sequence_protocol_and_indexing():
    """Test iteration, len, and indexing behavior."""
    ps = PauliString([PauliX(0), PauliZ(2)], coefficient=2.0)

    assert ps.qubits == (0, 2)
    assert len(ps) == 2
    assert ps[0] == PauliX(0)
    assert list(ps) == [PauliX(0), PauliZ(2)]


def test_pauli_string_equality_and_hash_include_coefficient():
    """Test equality/hash depend on Pauli terms and coefficient."""
    p1 = PauliString.from_qubits((0, 1), "XZ", coefficient=1.0)
    p2 = PauliString.from_qubits((0, 1), "XZ", coefficient=1.0)
    p3 = PauliString.from_qubits((0, 1), "XZ", coefficient=-1.0)

    assert p1 == p2
    assert hash(p1) == hash(p2)
    assert p1 != p3


def test_pauli_string_mul_scales_coefficient_and_preserves_terms():
    """Test PauliString.__mul__ returns scaled coefficient with same operators."""
    ps = PauliString.from_qubits((0, 2), "XZ", coefficient=2.0)

    scaled = ps * (-0.25j)

    assert scaled.qubits == ps.qubits
    assert list(scaled) == list(ps)
    assert scaled.coefficient == -0.5j
    assert ps.coefficient == 2.0


def test_pauli_string_cirq_property_preserves_terms_and_coefficient():
    """Test PauliString.cirq conversion with coefficient."""
    ps = PauliString.from_qubits((0, 2), "XZ", coefficient=-0.5j)

    expected = cirq.PauliString(
        {cirq.LineQubit(0): cirq.X, cirq.LineQubit(2): cirq.Z},
        coefficient=-0.5j,
    )

    assert ps.cirq == expected
