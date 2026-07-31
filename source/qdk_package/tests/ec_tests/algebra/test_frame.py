"""Tests for the provenance-carrying simulation frame group.

These cover the outcome-frame machinery ``FrameGroup`` exposes for the
readout-discovery path: factoring a target in the group and XOR-ing its
factors' frames, plus the per-generator ``relabel`` / ``restrict_to`` /
``complex_conjugated`` transforms and the support-based ``partition``.
"""
from __future__ import annotations

import pytest

from qdk.ec.profile.propagation.frames import FrameGroup, PauliFrame
from qdk.ec.profile.propagation.pauli import Pauli, identity


def _z(qubit: int) -> Pauli:
    return Pauli({qubit: "Z"})


def _x(qubit: int) -> Pauli:
    return Pauli({qubit: "X"})


def _group(pairs: list[tuple[Pauli, set[int]]]) -> FrameGroup:
    return FrameGroup(PauliFrame(pauli, frozenset(frame)) for pauli, frame in pairs)


# ── unframed ────────────────────────────────────────────────────────────────


def test_unframed_exposes_underlying_pauli_group() -> None:
    group = _group([(_z(0), set()), (_x(3), set())])
    plain = group.unframed
    assert plain.generators == [_z(0), _x(3)]
    assert set(plain.support) == {0, 3}


# ── factorization_of ────────────────────────────────────────────────────────


def test_factorization_of_single_generator_returns_its_frame() -> None:
    group = _group([(_z(0), {0}), (_x(1), {1})])
    factors = group.factorization_of(_z(0))
    assert factors is not None
    assert len(factors) == 1
    assert factors[0].pauli == _z(0)
    assert factors[0].frame == frozenset({0})


def test_factorization_of_product_returns_per_factor_frames() -> None:
    group = _group([(_z(0), {0}), (_z(1), {0, 1}), (_z(2), {2})])
    factors = group.factorization_of(_z(0) * _z(1) * _z(2))
    assert factors is not None
    by_pauli = {f.pauli: f.frame for f in factors}
    assert by_pauli == {
        _z(0): frozenset({0}),
        _z(1): frozenset({0, 1}),
        _z(2): frozenset({2}),
    }


def test_factorization_of_target_not_in_group_returns_none() -> None:
    group = _group([(_z(0), {0})])
    assert group.factorization_of(_x(5)) is None


def test_factorization_of_identity_returns_empty_list() -> None:
    group = _group([(_z(0), {0}), (_z(1), {1})])
    assert group.factorization_of(Pauli.identity()) == []


# ── frame_of ────────────────────────────────────────────────────────────────


def test_frame_of_xors_factor_frames() -> None:
    # {0} XOR {0, 1} XOR {2} = {1, 2}
    group = _group([(_z(0), {0}), (_z(1), {0, 1}), (_z(2), {2})])
    assert group.frame_of(_z(0) * _z(1) * _z(2)) == frozenset({1, 2})


def test_frame_of_identity_is_empty() -> None:
    group = _group([(_z(0), {0})])
    assert group.frame_of(Pauli.identity()) == frozenset()


def test_frame_of_raises_when_target_not_in_group() -> None:
    group = _group([(_z(0), set())])
    with pytest.raises(ValueError):
        group.frame_of(_x(9))


# ── __or__ ──────────────────────────────────────────────────────────────────


def test_or_concatenates_generators_and_frames() -> None:
    union = _group([(_z(0), {0})]) | _group([(_x(1), {1}), (_z(2), {2})])
    assert union.unframed.generators == [_z(0), _x(1), _z(2)]
    assert [g.frame for g in union.generators] == [
        frozenset({0}),
        frozenset({1}),
        frozenset({2}),
    ]


# ── relabel ─────────────────────────────────────────────────────────────────


def test_relabel_remaps_qubit_indices_keeping_frames() -> None:
    group = _group([(_z(0), {1}), (_x(1), {2})])
    remapped = group.relabel({0: 10, 1: 11})
    assert remapped.unframed.generators == [Pauli({10: "Z"}), Pauli({11: "X"})]
    assert [g.frame for g in remapped.generators] == [frozenset({1}), frozenset({2})]


def test_relabel_passes_unmapped_qubits_through() -> None:
    remapped = _group([(Pauli({0: "Z", 5: "X"}), {0})]).relabel({0: 100})
    assert remapped.unframed.generators == [Pauli({100: "Z", 5: "X"})]


# ── restrict_to ─────────────────────────────────────────────────────────────


def test_restrict_to_drops_characters_outside_support() -> None:
    group = _group([(Pauli({0: "Z", 1: "X", 2: "Y"}), {0, 1})])
    restricted = group.restrict_to({0, 2})
    assert restricted.unframed.generators == [Pauli({0: "Z", 2: "Y"})]
    assert [g.frame for g in restricted.generators] == [frozenset({0, 1})]


def test_restrict_to_preserves_phase() -> None:
    group = _group([(Pauli({0: "Z"}) * identity(-1), set())])
    restricted = group.restrict_to({0})
    assert restricted.unframed.generators[0].phase == -1


# ── complex_conjugated ──────────────────────────────────────────────────────


def test_complex_conjugated_flips_sign_on_odd_y_weight() -> None:
    group = _group(
        [(_z(0), set()), (Pauli({0: "Y"}), set()), (Pauli({0: "Y", 1: "Y"}), set())]
    )
    gens = group.complex_conjugated().unframed.generators
    assert gens[0] == _z(0)  # Y-weight 0 -> unchanged
    assert gens[1] == Pauli({0: "Y"}) * identity(-1)  # Y-weight 1 -> flipped
    assert gens[2] == Pauli({0: "Y", 1: "Y"})  # Y-weight 2 -> unchanged


def test_complex_conjugated_keeps_frames() -> None:
    group = _group([(Pauli({0: "Y"}), {1, 2})])
    assert [g.frame for g in group.complex_conjugated().generators] == [
        frozenset({1, 2})
    ]


# ── partition ───────────────────────────────────────────────────────────────


def test_partition_separates_supported_from_complement() -> None:
    group = _group([(_z(0), {0}), (_z(1), {1}), (_z(2), {2})])
    over, complement, _cross = group.partition(over={0, 1})
    over_paulis = set(over.unframed.generators)
    complement_paulis = set(complement.unframed.generators)
    assert _z(0) in over_paulis or _z(1) in over_paulis
    assert _z(2) in complement_paulis
