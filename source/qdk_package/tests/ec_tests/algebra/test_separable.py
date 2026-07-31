from itertools import zip_longest, chain
from typing import Any
import pytest
from hypothesis import given, strategies, settings
from multiset import Multiset
from qdk.ec.profile.propagation.pauli import (
    Pauli,
    PauliEnumerator,
    characters_of,
)
from qdk.ec.profile.separable_code import SeparableCode
from qdk.ec.profile.stabilizer_code import StabilizerCode
from ec_tests.algebra.test_stabilizer_codes import stabilizer_codes as _stabilizer_codes


def stabilizer_codes() -> strategies.SearchStrategy[StabilizerCode]:
    return strategies.sampled_from(_stabilizer_codes)


@given(strategies.lists(stabilizer_codes(), max_size=5))
def test_blocks_match_codes(codes: list[StabilizerCode]) -> None:
    tensor = SeparableCode.by_stacking(*codes)
    for code, block in zip_longest(codes, tensor.blocks):
        assert code.length == block.length
        assert code.logical_qubit_count == block.logical_qubit_count
        for code_gen, block_gen in zip_longest(code.stabilizers, block.stabilizers):
            assert _weight_profile_of(code_gen) == _weight_profile_of(block_gen)


@settings(deadline=1000)
@given(strategies.lists(stabilizer_codes(), max_size=5))
def test_bulk_properties_are_internally_consistent(codes: list[StabilizerCode]) -> None:
    tensor = SeparableCode.by_stacking(*codes)
    assert tuple(tensor.stabilizers) == tuple(
        chain(*(block.stabilizers for block in tensor.blocks))
    )
    assert tuple(tensor.logical_basis) == tuple(
        chain(*(block.logical_basis for block in tensor.blocks))
    )

    fused = StabilizerCode(tensor.stabilizers, logical_basis=tensor.logical_basis)
    assert tensor.support == fused.support
    assert tensor.length == fused.length
    assert tensor.logical_qubit_count == fused.logical_qubit_count
    assert tuple(tensor.stabilizers) == tuple(fused.stabilizers)
    assert tuple(tensor.logical_basis) == tuple(fused.logical_basis)


@settings(deadline=1000)
@given(
    strategies.lists(stabilizer_codes(), max_size=3),
    strategies.lists(strategies.integers(), min_size=10, max_size=10),
)
def test_error_properties_are_internally_consistent(
    codes: list[StabilizerCode], integers: list[int]
) -> None:
    return
    tensor = SeparableCode.by_stacking(*codes)
    fused = StabilizerCode(tensor.stabilizers, logical_basis=tensor.logical_basis)
    errors = list(PauliEnumerator(tensor.support).up_to_weight(1))
    indexes = [integer % len(errors) for integer in integers]
    for index in indexes:
        error = errors[index]
        assert tensor.syndrome_of(error) == fused.syndrome_of(error)
        assert tensor.is_trivial_error(error) == fused.is_trivial_error(error)
        assert tensor.is_trivial_logical_error(error) == fused.is_trivial_logical_error(
            error
        )
        assert tensor.is_logical_error(error) == fused.is_logical_error(error)
        assert tensor.is_non_trivial_logical_error(
            error
        ) == fused.is_non_trivial_logical_error(error)
        assert tensor.logical_action_of(error) == fused.logical_action_of(error)
        assert tensor.unsigned_logical_action_of(
            error
        ) == fused.unsigned_logical_action_of(error)


@settings(deadline=1000)
@given(strategies.lists(stabilizer_codes(), max_size=3))
def test_representatives_are_internally_consistent(codes: list[StabilizerCode]) -> None:
    tensor = SeparableCode.by_stacking(*codes)
    fused = StabilizerCode(tensor.stabilizers, logical_basis=tensor.logical_basis)
    paulis = PauliEnumerator(set(range(tensor.logical_qubit_count))).up_to_weight(1)
    for pauli in paulis:
        assert tensor.representative_of(pauli) == fused.representative_of(pauli)


@given(stabilizer_codes())
def test_overlapping(code: StabilizerCode) -> None:
    with pytest.raises(ValueError):
        SeparableCode(code, code)


def _weight_profile_of(pauli: Pauli) -> "Multiset[Any]":
    return Multiset(characters_of(pauli).values())
