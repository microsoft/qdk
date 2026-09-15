"""Minimum odd-cycle engine used by code and gadget distance."""

from __future__ import annotations

from typing import Iterable, Optional, Sequence, TypeVar

from .distance_solvers import (
    MwpfSolverOptions,
    Solver,
    solver_options,
    solve_bounds,
)

Label = TypeVar("Label")


def unique_non_empty_elements_of(
    sets: Iterable[frozenset[int]],
) -> tuple[list[frozenset[int]], list[list[int]], list[int]]:
    unique: list[frozenset[int]] = []
    ids: dict[frozenset[int], int] = {}
    groups: list[list[int]] = []
    empty: list[int] = []
    for index, item in enumerate(sets):
        if not item:
            empty.append(index)
        elif item in ids:
            groups[ids[item]].append(index)
        else:
            ids[item] = len(unique)
            unique.append(item)
            groups.append([index])
    return unique, groups, empty


def cycle_labels(cycle: Iterable[int], labels: Sequence[Label]) -> list[Label]:
    return [labels[index] for index in cycle]


class OddCycles:
    def __init__(
        self,
        check_matrix: Sequence[frozenset[int]],
        parity_indicators: Sequence[frozenset[int]],
        unique_columns_ids: Optional[Sequence[int]] = None,
    ) -> None:
        if len(check_matrix) != len(parity_indicators):
            raise ValueError("check and logical columns must have equal lengths")
        self._source_checks = tuple(check_matrix)
        self._source_parities = tuple(parity_indicators)
        self._source_ids = (
            tuple(range(len(check_matrix)))
            if unique_columns_ids is None
            else tuple(unique_columns_ids)
        )
        self.odd_cycle_length: Optional[int] = None
        self.short_odd_cycle: Optional[list[int]] = None
        self.short_odd_cycle_lower_bound = 3
        if unique_columns_ids is not None:
            self.check_matrix = list(check_matrix)
            self.parity_indicators = list(parity_indicators)
            self.unique_columns_ids = list(unique_columns_ids)
            return
        unique, groups, empty = unique_non_empty_elements_of(check_matrix)
        self.unique_columns_ids = [group[0] for group in groups]
        self.check_matrix = unique
        self.parity_indicators = [
            parity_indicators[index] for index in self.unique_columns_ids
        ]
        for index in empty:
            if parity_indicators[index]:
                self.odd_cycle_length = 1
                self.short_odd_cycle = [index]
                self.short_odd_cycle_lower_bound = 1
                return
        for group in groups:
            base = parity_indicators[group[0]]
            for other in group[1:]:
                if parity_indicators[other] != base:
                    self.odd_cycle_length = 2
                    self.short_odd_cycle = [group[0], other]
                    self.short_odd_cycle_lower_bound = 2
                    return

    def shortest(
        self,
        solver: Solver,
        coset_indicator: Optional[frozenset[int]] = None,
        cycle_size_upper_bound: Optional[int] = None,
    ) -> tuple[int, list[int]]:
        lower, upper, cycle = self.bounds(
            cycle_size_upper_bound, coset_indicator, solver
        )
        if lower != upper:
            raise RuntimeError(
                f"solver did not prove exact distance: bounds are {lower} and {upper}"
            )
        return upper, cycle

    def bounds(
        self,
        odd_cycle_length_upper_bound: Optional[int] = None,
        coset_indicator: Optional[frozenset[int]] = None,
        solver: Optional[Solver] = None,
    ) -> tuple[int, int, list[int]]:
        solver = solver_options(MwpfSolverOptions() if solver is None else solver)
        if (
            odd_cycle_length_upper_bound is not None
            and odd_cycle_length_upper_bound < 0
        ):
            raise ValueError("upper_bound must be nonnegative")
        if coset_indicator is not None:
            projected = OddCycles(
                self._source_checks,
                [
                    (
                        frozenset({0})
                        if len(indicator & coset_indicator) % 2
                        else frozenset()
                    )
                    for indicator in self._source_parities
                ],
            )
            lower, upper, cycle = projected.bounds(
                odd_cycle_length_upper_bound, solver=solver
            )
            return lower, upper, cycle_labels(cycle, self._source_ids)
        if self.odd_cycle_length is not None:
            assert self.short_odd_cycle is not None
            return (
                self.odd_cycle_length,
                self.odd_cycle_length,
                self.short_odd_cycle,
            )
        lower, upper, cycle = solve_bounds(
            self, odd_cycle_length_upper_bound, coset_indicator, solver
        )
        return lower, upper, cycle_labels(cycle, self.unique_columns_ids)


__all__ = ["OddCycles", "cycle_labels", "unique_non_empty_elements_of"]
