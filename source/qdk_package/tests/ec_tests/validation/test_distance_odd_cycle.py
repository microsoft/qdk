"""Tests for the ``OddCycles`` distance engine and its solver backends."""
from __future__ import annotations

from qdk.ec._analysis.distance_solvers import (
    CustomExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
)
from qdk.ec._analysis.odd_cycles import OddCycles, unique_non_empty_elements_of
from ec_tests.testing.optional import requires_mwpf


def test_distance_one_fast_path_detects_undetectable_logical() -> None:
    check_matrix = [frozenset({0}), frozenset()]
    parity_indicators = [frozenset(), frozenset({0})]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert odd_cycles.odd_cycle_length == 1
    size, cycle = odd_cycles.shortest(ExhaustiveSolverOptions())
    assert size == 1
    assert cycle == [1]


def test_distance_two_fast_path_detects_equal_checks_distinct_parity() -> None:
    check_matrix = [frozenset({0}), frozenset({0})]
    parity_indicators = [frozenset(), frozenset({0})]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert odd_cycles.odd_cycle_length == 2
    size, cycle = odd_cycles.shortest(ExhaustiveSolverOptions())
    assert size == 2
    assert set(cycle) == {0, 1}


def test_exhaustive_finds_size_three_triangle_cycle() -> None:
    check_matrix = [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})]
    parity_indicators = [frozenset({0}), frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert odd_cycles.odd_cycle_length is None
    size, cycle = odd_cycles.shortest(ExhaustiveSolverOptions())
    assert size == 3
    assert set(cycle) == {0, 1, 2}


@requires_mwpf
def test_mwpf_matches_exhaustive_on_triangle_cycle() -> None:
    check_matrix = [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})]
    parity_indicators = [frozenset({0}), frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    lower, upper, cycle = odd_cycles.bounds(solver=MwpfSolverOptions())
    assert lower <= upper == 3
    assert set(cycle) == {0, 1, 2}


def test_duplicate_columns_are_deduplicated_but_witness_uses_original_ids() -> None:
    check_matrix = [frozenset({0, 1}), frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})]
    parity_indicators = [frozenset({0}), frozenset({0}), frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert len(odd_cycles.check_matrix) == 3
    assert odd_cycles.unique_columns_ids == [0, 2, 3]
    size, cycle = odd_cycles.shortest(ExhaustiveSolverOptions())
    assert size == 3
    assert set(cycle) == {0, 2, 3}


def test_unique_non_empty_elements_of_groups_and_collects_empties() -> None:
    sets = [frozenset({0}), frozenset(), frozenset({0}), frozenset({1})]
    unique, groups, empties = unique_non_empty_elements_of(sets)
    assert unique == [frozenset({0}), frozenset({1})]
    assert groups == [[0, 2], [3]]
    assert empties == [1]


def test_custom_exact_solver_seam_is_dispatched() -> None:
    check_matrix = [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})]
    parity_indicators = [frozenset({0}), frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)

    def fixed_solver(
        _data: OddCycles,
        _bound: int | None,
        _coset: frozenset[int] | None,
    ) -> tuple[int, list[int]]:
        return 1, [0]

    size, cycle = odd_cycles.shortest(CustomExactSolver(fixed_solver))
    assert size == 1
    assert cycle == [0]


def test_no_logical_returns_empty_witness() -> None:
    check_matrix = [frozenset({0}), frozenset({1})]
    parity_indicators: list[frozenset[int]] = [frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    size, cycle = odd_cycles.shortest(ExhaustiveSolverOptions())
    assert cycle == []
    assert size > len(check_matrix)
