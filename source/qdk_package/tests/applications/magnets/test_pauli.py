# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for Pauli, Fermion, Majorana, and string utilities."""

import pytest

cirq = pytest.importorskip("cirq")

from qdk.applications.magnets import (
    edge_operator,
    Fermion,
    FermionAnnihilation,
    FermionCreation,
    FermionString,
    Majorana,
    MajoranaDualFermion,
    MajoranaFermion,
    MajoranaString,
    Pauli,
    PauliString,
    PauliX,
    PauliY,
    PauliZ,
    hopping_term,
    vertex_operator,
)


def test_majorana_init_from_int_and_string():
    """Test Majorana initialization from int and string labels."""
    majorana = Majorana(12, site=1)
    majorana_alias = Majorana("g", site=2)
    dual = Majorana(13, site=3)
    dual_alias = Majorana("G'", site=4)

    assert majorana.op == 12 and majorana.site == 1
    assert majorana_alias.op == 12 and majorana_alias.site == 2
    assert dual.op == 13 and dual.site == 3
    assert dual_alias.op == 13 and dual_alias.site == 4


@pytest.mark.parametrize("value", [11, 14, 42])
def test_majorana_invalid_int_raises(value: int):
    """Test invalid integer Majorana identifiers raise ValueError."""
    with pytest.raises(ValueError, match="Integer value must be 12 or 13"):
        Majorana(value)


def test_majorana_invalid_string_raises():
    """Test invalid string Majorana identifiers raise ValueError."""
    with pytest.raises(ValueError, match="String value must be one of"):
        Majorana("A")


def test_majorana_invalid_type_raises():
    """Test non-int/non-str Majorana identifiers raise ValueError."""
    with pytest.raises(ValueError, match="Expected int or str"):
        Majorana(1.5)  # type: ignore[arg-type]


def test_majorana_helpers_create_expected_operator():
    """Test MajoranaFermion/MajoranaDualFermion helper constructors."""
    assert MajoranaFermion(0) == Majorana("G", 0)
    assert MajoranaDualFermion(1) == Majorana("G'", 1)


def test_majorana_string_representation_uses_g_and_g_prime_notation():
    """Test Majorana string forms use G and G' as the canonical labels."""
    assert str(Majorana(12, site=2)) == "G(2)"
    assert repr(Majorana(13, site=3)) == "Majorana('G\'', site=3)"


def test_majorana_string_init_requires_majorana_instances():
    """Test MajoranaString initializer validates element types."""
    with pytest.raises(TypeError, match="Expected Majorana instance"):
        MajoranaString([MajoranaFermion(0), "G'"])  # type: ignore[list-item]


def test_majorana_string_from_sites_accepts_string_and_int_values():
    """Test MajoranaString.from_sites accepts both string and int identifiers."""
    from_string = MajoranaString.from_sites((0, 1), ["G", "G'"], coefficient=-1j)
    from_ints = MajoranaString.from_sites((0, 1), [12, 13], coefficient=-1j)

    assert from_string == from_ints
    assert len(from_string) == 2
    assert from_string.sites == (0, 1)
    assert from_string.majoranas == ("G", "G'")


def test_majorana_string_from_sites_length_mismatch_raises():
    """Test from_sites raises when site/value lengths differ."""
    with pytest.raises(ValueError, match="Length mismatch"):
        MajoranaString.from_sites((0, 1), ["G"])


def test_majorana_string_sequence_protocol_and_indexing():
    """Test MajoranaString iteration, len, and indexing behavior."""
    ms = MajoranaString([MajoranaFermion(0), MajoranaDualFermion(2)], coefficient=2.0)

    assert ms.sites == (0, 2)
    assert len(ms) == 2
    assert ms[0] == MajoranaFermion(0)
    assert list(ms) == [MajoranaFermion(0), MajoranaDualFermion(2)]


def test_majorana_string_equality_and_hash_include_coefficient():
    """Test equality/hash depend on Majorana terms and coefficient."""
    m1 = MajoranaString.from_sites((0, 1), ["G", "G'"], coefficient=1.0)
    m2 = MajoranaString.from_sites((0, 1), ["G", "G'"], coefficient=1.0)
    m3 = MajoranaString.from_sites((0, 1), ["G", "G'"], coefficient=-1.0)

    assert m1 == m2
    assert hash(m1) == hash(m2)
    assert m1 != m3


def test_majorana_string_mul_scales_coefficient_and_preserves_terms():
    """Test MajoranaString.__mul__ returns scaled coefficient with same operators."""
    ms = MajoranaString.from_sites((0, 2), ["G", "G'"], coefficient=2.0)

    scaled = ms * (-0.25j)

    assert scaled.sites == ms.sites
    assert list(scaled) == list(ms)
    assert scaled.coefficient == -0.5j
    assert ms.coefficient == 2.0


def test_majorana_string_normalize_reorders_with_sign_flip():
    """Test normalize uses swap parity and the G[j] < G'[j] convention."""
    ms = MajoranaString([MajoranaDualFermion(1), MajoranaFermion(1)], coefficient=2.0)

    ms.normalize()

    assert ms.sites == (1, 1)
    assert ms.majoranas == ("G", "G'")
    assert list(ms) == [MajoranaFermion(1), MajoranaDualFermion(1)]
    assert ms.coefficient == -2.0


def test_majorana_string_normalize_cancels_adjacent_equal_terms():
    """Test normalize removes adjacent equal Majorana pairs after reordering."""
    ms = MajoranaString(
        [MajoranaFermion(1), MajoranaFermion(0), MajoranaFermion(0), MajoranaDualFermion(1)],
        coefficient=2.0,
    )

    ms.normalize()

    assert ms.sites == (1, 1)
    assert ms.majoranas == ("G", "G'")
    assert list(ms) == [MajoranaFermion(1), MajoranaDualFermion(1)]
    assert ms.coefficient == 2.0


def test_vertex_operator_returns_imaginary_majorana_pair():
    """Test vertex_operator constructs i * G[j] * G'[j]."""
    term = vertex_operator(3)

    assert term == MajoranaString.from_sites((3, 3), ["G", "G'"], coefficient=1j)
    assert term.coefficient == 1j
    assert term.sites == (3, 3)


def test_edge_operator_returns_imaginary_majorana_pair():
    """Test edge_operator constructs i * G[j] * G[k]."""
    term = edge_operator(1, 4)

    assert term == MajoranaString.from_sites((1, 4), ["G", "G"], coefficient=1j)
    assert term.coefficient == 1j
    assert term.sites == (1, 4)


def test_fermion_init_from_int_and_string():
    """Test Fermion initialization from int and string labels."""
    creation = Fermion(10, site=1)
    creation_alias = Fermion("A^", site=2)
    annihilation = Fermion(11, site=3)
    annihilation_alias = Fermion("a", site=4)

    assert creation.op == 10 and creation.site == 1
    assert creation_alias.op == 10 and creation_alias.site == 2
    assert annihilation.op == 11 and annihilation.site == 3
    assert annihilation_alias.op == 11 and annihilation_alias.site == 4


@pytest.mark.parametrize("value", [9, 12, 42])
def test_fermion_invalid_int_raises(value: int):
    """Test invalid integer fermion identifiers raise ValueError."""
    with pytest.raises(ValueError, match="Integer value must be 10 or 11"):
        Fermion(value)


def test_fermion_invalid_string_raises():
    """Test invalid string fermion identifiers raise ValueError."""
    with pytest.raises(ValueError, match="String value must be one of"):
        Fermion("Z")


def test_fermion_invalid_type_raises():
    """Test non-int/non-str fermion identifiers raise ValueError."""
    with pytest.raises(ValueError, match="Expected int or str"):
        Fermion(1.5)  # type: ignore[arg-type]


def test_fermion_helpers_create_expected_operator():
    """Test FermionCreation/FermionAnnihilation helper constructors."""
    assert FermionCreation(0) == Fermion("A^", 0)
    assert FermionAnnihilation(1) == Fermion("ANNIHILATION", 1)


def test_fermion_string_representation_uses_a_and_a_dagger_notation():
    """Test Fermion string forms use A and A^ as the canonical labels."""
    assert str(Fermion(10, site=2)) == "A^(2)"
    assert repr(Fermion(11, site=3)) == "Fermion('A', site=3)"


def test_fermion_string_init_requires_fermion_instances():
    """Test FermionString initializer validates element types."""
    with pytest.raises(TypeError, match="Expected Fermion instance"):
        FermionString([FermionCreation(0), "A"])  # type: ignore[list-item]


def test_fermion_string_from_sites_accepts_string_and_int_values():
    """Test FermionString.from_sites accepts both string and int identifiers."""
    from_string = FermionString.from_sites((0, 1), ["A^", "A"], coefficient=-1j)
    from_ints = FermionString.from_sites((0, 1), [10, 11], coefficient=-1j)

    assert from_string == from_ints
    assert len(from_string) == 2
    assert from_string.sites == (0, 1)
    assert from_string.fermions == ("A^", "A")


def test_fermion_string_from_sites_length_mismatch_raises():
    """Test from_sites raises when site/value lengths differ."""
    with pytest.raises(ValueError, match="Length mismatch"):
        FermionString.from_sites((0, 1), ["A^"])


def test_fermion_string_sequence_protocol_and_indexing():
    """Test FermionString iteration, len, and indexing behavior."""
    fs = FermionString([FermionCreation(0), FermionAnnihilation(2)], coefficient=2.0)

    assert fs.sites == (0, 2)
    assert len(fs) == 2
    assert fs[0] == FermionCreation(0)
    assert list(fs) == [FermionCreation(0), FermionAnnihilation(2)]


def test_fermion_string_equality_and_hash_include_coefficient():
    """Test equality/hash depend on Fermion terms and coefficient."""
    f1 = FermionString.from_sites((0, 1), ["A^", "A"], coefficient=1.0)
    f2 = FermionString.from_sites((0, 1), ["A^", "A"], coefficient=1.0)
    f3 = FermionString.from_sites((0, 1), ["A^", "A"], coefficient=-1.0)

    assert f1 == f2
    assert hash(f1) == hash(f2)
    assert f1 != f3


def test_fermion_string_mul_scales_coefficient_and_preserves_terms():
    """Test FermionString.__mul__ returns scaled coefficient with same operators."""
    fs = FermionString.from_sites((0, 2), ["A^", "A"], coefficient=2.0)

    scaled = fs * (-0.25j)

    assert scaled.sites == fs.sites
    assert list(scaled) == list(fs)
    assert scaled.coefficient == -0.5j
    assert fs.coefficient == 2.0


def test_fermion_string_hermitian_conjugate_reverses_and_conjugates():
    """Test Hermitian conjugation reverses order, swaps ops, and conjugates coefficient."""
    fs = FermionString.from_sites((0, 2), ["A^", "A"], coefficient=1 + 2j)

    conjugated = fs.hermitian_conjugate()

    assert conjugated == FermionString.from_sites((2, 0), ["A^", "A"], coefficient=1 - 2j)
    assert fs.coefficient == 1 + 2j


def test_hopping_term_returns_creation_then_annihilation_string():
    """Test hopping_term constructs A^[j] A[k] with unit coefficient."""
    term = hopping_term(1, 3)

    assert term == FermionString.from_sites((1, 3), ["A^", "A"])
    assert term.coefficient == 1.0


def test_hopping_term_with_equal_indices_gives_number_operator():
    """Test hopping_term(j, j) gives the fermionic number operator form."""
    term = hopping_term(2, 2)

    assert term.sites == (2, 2)
    assert term.fermions == ("A^", "A")


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


def test_pauli_string_normalize_reorders_simplifies_and_removes_identity():
    """Test normalize sorts qubits, multiplies repeated qubits, and drops identities."""
    ps = PauliString([PauliX(2), PauliX(0), PauliZ(2), PauliX(0)], coefficient=2.0)

    ps.normalize()

    assert ps.qubits == (2,)
    assert ps.paulis == "Y"
    assert list(ps) == [PauliY(2)]
    assert ps.coefficient == -2j


def test_pauli_string_cirq_property_preserves_terms_and_coefficient():
    """Test PauliString.cirq conversion with coefficient."""
    ps = PauliString.from_qubits((0, 2), "XZ", coefficient=-0.5j)

    expected = cirq.PauliString(
        {cirq.LineQubit(0): cirq.X, cirq.LineQubit(2): cirq.Z},
        coefficient=-0.5j,
    )

    assert ps.cirq == expected


def test_pauli_string_cirq_property_normalizes_duplicate_qubits():
    """Test PauliString.cirq simplifies repeated qubits before conversion."""
    ps = PauliString([PauliX(2), PauliX(0), PauliZ(2), PauliX(0)], coefficient=2.0)

    expected = cirq.PauliString(
        {cirq.LineQubit(2): cirq.Y},
        coefficient=-2j,
    )

    assert ps.cirq == expected
    assert ps.qubits == (2, 0, 2, 0)
