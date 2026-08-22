"""Minimum odd-cycle engine used by code and gadget distance."""

from __future__ import annotations

from typing import Iterable, Optional, Sequence, TypeVar

from .distance_solvers import (
    BoundsSolver,
    CustomBoundsSolver,
    CustomExactSolver,
    ExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
    exhaustive_shortest_odd_cycle,
    mwpf_bounds,
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
        solver: ExactSolver,
        coset_indicator: Optional[frozenset[int]] = None,
        cycle_size_upper_bound: Optional[int] = None,
    ) -> tuple[int, list[int]]:
        if self.odd_cycle_length is not None:
            assert self.short_odd_cycle is not None
            return self.odd_cycle_length, self.short_odd_cycle
        if isinstance(solver, ExhaustiveSolverOptions):
            size, cycle = exhaustive_shortest_odd_cycle(
                self, cycle_size_upper_bound, coset_indicator, solver
            )
        elif isinstance(solver, CustomExactSolver):
            size, cycle = solver.solver(self, cycle_size_upper_bound, coset_indicator)
        else:
            raise NotImplementedError(f"Unsupported exact solver {solver!r}")
        return size, cycle_labels(cycle, self.unique_columns_ids)

    def bounds(
        self,
        odd_cycle_length_upper_bound: Optional[int] = None,
        coset_indicator: Optional[frozenset[int]] = None,
        solver: Optional[BoundsSolver] = None,
    ) -> tuple[int, int, list[int]]:
        solver = solver or MwpfSolverOptions()
        if self.odd_cycle_length is not None:
            assert self.short_odd_cycle is not None
            return (
                self.odd_cycle_length,
                self.odd_cycle_length,
                self.short_odd_cycle,
            )
        if isinstance(solver, MwpfSolverOptions):
            lower, upper, cycle = mwpf_bounds(
                self,
                odd_cycle_length_upper_bound,
                coset_indicator,
                solver,
            )
        elif isinstance(solver, ExhaustiveSolverOptions):
            size, cycle = exhaustive_shortest_odd_cycle(
                self,
                odd_cycle_length_upper_bound,
                coset_indicator,
                solver,
            )
            lower = upper = size
        elif isinstance(solver, CustomBoundsSolver):
            lower, upper, cycle = solver.solver(
                self, odd_cycle_length_upper_bound, coset_indicator
            )
        else:
            raise NotImplementedError(f"Unsupported bounds solver {solver!r}")
        return lower, upper, cycle_labels(cycle, self.unique_columns_ids)


__all__ = ["OddCycles", "cycle_labels", "unique_non_empty_elements_of"]
