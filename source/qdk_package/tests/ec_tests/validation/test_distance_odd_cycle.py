"""Tests for the ``OddCycles`` distance engine and its solver backends."""

from __future__ import annotations

from unittest.mock import MagicMock, patch
from types import SimpleNamespace
from itertools import count
from random import Random
from importlib import import_module
from typing import cast

import pytest

from qdk.ec._analysis.distance_solvers import (
    CustomBoundsSolver,
    CustomExactSolver,
    EnumerationSolverOptions,
    HighsSolverOptions,
    MwpfSolverOptions,
    Solver,
)
from qdk.ec._analysis.odd_cycles import OddCycles, unique_non_empty_elements_of
from ec_tests.testing.optional import requires_highs, requires_mwpf


class PanicException(BaseException):
    pass


@requires_mwpf
@pytest.mark.parametrize("stage", ["solve", "subgraph", "subgraph_range"])
def test_mwpf_failures_do_not_certify_remaining_logical_searches(stage: str) -> None:
    data = OddCycles(
        [
            frozenset(column)
            for column in ({0, 1}, {1, 2}, {2, 0}, {3, 4}, {4, 5}, {5, 6}, {6, 3})
        ],
        [frozenset(column) for column in ({0}, set(), set(), {1}, set(), set(), set())],
    )
    assert data.shortest(EnumerationSolverOptions())[0] == 3
    failed = MagicMock()
    failed.subgraph.return_value = [0, 1, 2]
    getattr(failed, stage).side_effect = PanicException("backend failure")
    completed = MagicMock()
    completed.subgraph.return_value = [3, 4, 5, 6]
    completed.subgraph_range.return_value = (
        [],
        SimpleNamespace(lower=SimpleNamespace(float=lambda: 4.0)),
    )
    factory = MagicMock(side_effect=[failed, completed])
    with patch(
        "qdk.ec._analysis.distance_solvers._mwpf_solver_class", return_value=factory
    ):
        with pytest.raises(RuntimeError, match="MWPF"):
            data.bounds(solver=MwpfSolverOptions())
    assert factory.call_count == 1


@requires_mwpf
@pytest.mark.parametrize("witness", [[], [0], [0, 1, 9], [0, 1, 2, 2]])
def test_mwpf_invalid_witness_raises(witness: list[int]) -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    solver = MagicMock()
    solver.subgraph.return_value = witness
    with patch(
        "qdk.ec._analysis.distance_solvers._mwpf_solver_class",
        return_value=MagicMock(return_value=solver),
    ):
        with pytest.raises(RuntimeError, match="invalid witness"):
            data.bounds(solver=MwpfSolverOptions())


@requires_mwpf
@pytest.mark.parametrize("lower", [float("nan"), float("inf"), -1.0, 4.0])
def test_mwpf_invalid_lower_bound_raises(lower: float) -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    solver = MagicMock()
    solver.subgraph.return_value = [0, 1, 2]
    solver.subgraph_range.return_value = (
        [],
        SimpleNamespace(lower=SimpleNamespace(float=lambda: lower)),
    )
    with patch(
        "qdk.ec._analysis.distance_solvers._mwpf_solver_class",
        return_value=MagicMock(return_value=solver),
    ):
        with pytest.raises(RuntimeError, match="lower bound"):
            data.bounds(solver=MwpfSolverOptions())


@requires_mwpf
def test_mwpf_keeps_reachable_searches_alongside_impossible_ones() -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0, 1}), frozenset(), frozenset({0})],
    )
    lower, upper, witness = data.bounds(solver=MwpfSolverOptions())
    assert lower <= upper == 3
    assert set(witness) == {0, 1, 2}


@pytest.mark.parametrize("indicators", [[{0}, set()], [{0}, {0}]])
def test_mwpf_skips_only_provably_unreachable_indicators(
    indicators: list[set[int]],
) -> None:
    data = OddCycles(
        [frozenset({0}), frozenset({1})], [frozenset(item) for item in indicators]
    )
    with patch("qdk.ec._analysis.distance_solvers._solve_observable") as backend:
        assert data.bounds(solver=MwpfSolverOptions()) == (3, 3, [])
        backend.assert_not_called()


def test_distance_one_fast_path_detects_undetectable_logical() -> None:
    check_matrix = [frozenset({0}), frozenset()]
    parity_indicators = [frozenset(), frozenset({0})]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert odd_cycles.odd_cycle_length == 1
    size, cycle = odd_cycles.shortest(EnumerationSolverOptions())
    assert size == 1
    assert cycle == [1]


def test_distance_two_fast_path_detects_equal_checks_distinct_parity() -> None:
    check_matrix = [frozenset({0}), frozenset({0})]
    parity_indicators = [frozenset(), frozenset({0})]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert odd_cycles.odd_cycle_length == 2
    size, cycle = odd_cycles.shortest(EnumerationSolverOptions())
    assert size == 2
    assert set(cycle) == {0, 1}


def test_enumeration_finds_size_three_triangle_cycle() -> None:
    check_matrix = [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})]
    parity_indicators = [frozenset({0}), frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert odd_cycles.odd_cycle_length is None
    size, cycle = odd_cycles.shortest(EnumerationSolverOptions())
    assert size == 3
    assert set(cycle) == {0, 1, 2}


@requires_mwpf
def test_mwpf_matches_enumeration_on_triangle_cycle() -> None:
    check_matrix = [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})]
    parity_indicators = [frozenset({0}), frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    lower, upper, cycle = odd_cycles.bounds(solver=MwpfSolverOptions())
    assert lower <= upper == 3
    assert set(cycle) == {0, 1, 2}


def test_duplicate_columns_are_deduplicated_but_witness_uses_original_ids() -> None:
    check_matrix = [
        frozenset({0, 1}),
        frozenset({0, 1}),
        frozenset({1, 2}),
        frozenset({0, 2}),
    ]
    parity_indicators = [frozenset({0}), frozenset({0}), frozenset(), frozenset()]
    odd_cycles = OddCycles(check_matrix, parity_indicators)
    assert len(odd_cycles.check_matrix) == 3
    assert odd_cycles.unique_columns_ids == [0, 2, 3]
    size, cycle = odd_cycles.shortest(EnumerationSolverOptions())
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
    size, cycle = odd_cycles.shortest(EnumerationSolverOptions())
    assert cycle == []
    assert size > len(check_matrix)


def test_exact_search_requires_closed_bounds() -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    solver = CustomBoundsSolver(lambda *_: (2, 3, [0, 1, 2]))
    assert data.bounds(solver=solver) == (2, 3, [0, 1, 2])
    with pytest.raises(RuntimeError, match="exact distance"):
        data.shortest(solver)


def test_enumeration_cutoff_returns_bounds_not_false_exactness() -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({2, 3}), frozenset({3, 0})],
        [frozenset({0}), frozenset(), frozenset(), frozenset()],
    )
    assert data.bounds(
        odd_cycle_length_upper_bound=2, solver=EnumerationSolverOptions()
    ) == (3, 5, [])
    with pytest.raises(RuntimeError, match="exact distance"):
        data.shortest(EnumerationSolverOptions(), cycle_size_upper_bound=2)


@requires_highs
def test_default_bounds_uses_highs() -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    with patch(
        "qdk.ec._analysis.distance_solvers.import_module", wraps=import_module
    ) as backend:
        lower, upper, witness = data.bounds()
    assert lower == upper == 3
    assert set(witness) == {0, 1, 2}
    backend.assert_called_once_with("highspy")


@requires_highs
def test_highs_matches_enumeration_and_handles_cutoff() -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({2, 3}), frozenset({3, 0})],
        [frozenset({0}), frozenset(), frozenset(), frozenset()],
    )
    assert data.shortest("highs")[0] == data.shortest("enumeration")[0] == 4
    lower, upper, witness = data.bounds(solver="highs")
    assert lower == upper == 4
    assert set(witness) == {0, 1, 2, 3}
    assert data.bounds(odd_cycle_length_upper_bound=2, solver="highs") == (3, 5, [])
    with pytest.raises(RuntimeError, match="exact distance"):
        data.shortest("highs", cycle_size_upper_bound=2)


@requires_highs
def test_highs_proves_infeasibility() -> None:
    data = OddCycles([frozenset({0}), frozenset({1})], [frozenset({0}), frozenset({0})])
    assert data.bounds(solver="highs") == (3, 3, [])
    assert data.shortest("highs") == (3, [])


def test_highs_missing_package_has_install_hint() -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    with patch(
        "qdk.ec._analysis.distance_solvers.import_module",
        side_effect=ModuleNotFoundError(name="highspy"),
    ):
        assert data.shortest("enumeration")[0] == 3
        with pytest.raises(ImportError, match="qdk\\[ec\\]"):
            data.shortest("highs")


@requires_highs
def test_highs_limited_result_retains_gap() -> None:
    backend = import_module("highspy")
    model = MagicMock()
    indices = count()
    model.setOptionValue.return_value = backend.HighsStatus.kOk
    model.addRow.return_value = backend.HighsStatus.kOk
    model.addBinary.side_effect = lambda **_: next(indices)
    model.addIntegral.side_effect = lambda **_: next(indices)
    model.run.return_value = backend.HighsStatus.kWarning
    model.getModelStatus.return_value = backend.HighsModelStatus.kTimeLimit
    model.getInfo.return_value = SimpleNamespace(valid=True, mip_dual_bound=2.0)
    model.getSolution.return_value = SimpleNamespace(
        value_valid=True, col_value=[1.0, 1.0, 1.0]
    )
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    with patch.object(backend, "Highs", return_value=model):
        assert data.bounds(solver=HighsSolverOptions(timeout=0.1)) == (2, 3, [0, 1, 2])
        model.setOptionValue.assert_any_call("time_limit", 0.1)
        indices = count()
        with pytest.raises(RuntimeError, match="exact distance"):
            data.shortest("highs")
        indices = count()
        model.getSolution.return_value = SimpleNamespace(value_valid=False)
        with pytest.raises(RuntimeError, match="without a feasible"):
            data.bounds(solver="highs")
        indices = count()
        model.getModelStatus.return_value = backend.HighsModelStatus.kSolveError
        with pytest.raises(RuntimeError, match="usable distance bounds"):
            data.bounds(solver="highs")


@requires_highs
@pytest.mark.parametrize(
    "values,bound",
    [
        ([0.5, 1.0, 1.0], 2.0),
        ([1.0, 0.0, 0.0], 1.0),
        ([1.0, 1.0, 1.0], 4.0),
        ([1.0, 1.0, 1.0], float("nan")),
    ],
)
def test_highs_rejects_invalid_results(values: list[float], bound: float) -> None:
    backend = import_module("highspy")
    model = MagicMock()
    indices = count()
    model.setOptionValue.return_value = backend.HighsStatus.kOk
    model.addRow.return_value = backend.HighsStatus.kOk
    model.addBinary.side_effect = lambda **_: next(indices)
    model.addIntegral.side_effect = lambda **_: next(indices)
    model.run.return_value = backend.HighsStatus.kOk
    model.getModelStatus.return_value = backend.HighsModelStatus.kOptimal
    model.getInfo.return_value = SimpleNamespace(valid=True, mip_dual_bound=bound)
    model.getSolution.return_value = SimpleNamespace(value_valid=True, col_value=values)
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    with patch.object(backend, "Highs", return_value=model), pytest.raises(
        RuntimeError, match="HiGHS"
    ):
        data.bounds(solver="highs")


@requires_highs
def test_highs_matches_small_enumeration_instances() -> None:
    random = Random(1827)
    for _ in range(20):
        checks = [
            frozenset(index for index in range(4) if random.getrandbits(1))
            for _ in range(7)
        ]
        parities = [
            frozenset(index for index in range(3) if random.getrandbits(1))
            for _ in checks
        ]
        data = OddCycles(checks, parities)
        for coset in (None, frozenset({0, 2})):
            expected, _ = data.shortest("enumeration", coset_indicator=coset)
            lower, upper, witness = data.bounds(solver="highs", coset_indicator=coset)
            assert lower == upper == expected
            syndrome: set[int] = set()
            logical: set[int] = set()
            for index in witness:
                syndrome ^= checks[index]
                logical ^= parities[index]
            assert not syndrome
            if witness:
                assert logical if coset is None else len(logical & coset) % 2


@pytest.mark.parametrize(
    "solver", ["enumeration", "mwpf", pytest.param("highs", marks=requires_highs)]
)
def test_shared_coset_selection_precedes_shortcuts(solver: str) -> None:
    data = OddCycles([frozenset(), frozenset()], [frozenset({0, 1}), frozenset({1})])
    selected = cast(Solver, solver)
    assert data.shortest(selected, coset_indicator=frozenset({0, 1})) == (1, [1])
    assert data.bounds(solver=selected, coset_indicator=frozenset()) == (1, 1, [])


@requires_mwpf
def test_mwpf_can_supply_an_exact_result() -> None:
    data = OddCycles(
        [frozenset({0, 1}), frozenset({1, 2}), frozenset({0, 2})],
        [frozenset({0}), frozenset(), frozenset()],
    )
    assert data.shortest("mwpf")[0] == 3


@pytest.mark.parametrize("name", ["other", "", "exhaustive"])
def test_invalid_solver_is_rejected_even_for_shortcuts(name: str) -> None:
    data = OddCycles([frozenset()], [frozenset({0})])
    with pytest.raises(ValueError, match="solver must be"):
        data.shortest(cast(Solver, name))
