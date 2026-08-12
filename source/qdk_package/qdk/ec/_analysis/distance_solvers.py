"""Exact and MWPF distance solver backends."""

from __future__ import annotations

import math
from dataclasses import dataclass
from functools import reduce
from itertools import combinations
from operator import xor
from typing import Any, Callable, Optional, TYPE_CHECKING, Union

if TYPE_CHECKING:
    from .odd_cycles import OddCycles


@dataclass
class ExhaustiveSolverOptions:
    size_upper_bound: Optional[int] = None


@dataclass
class MwpfSolverOptions:
    solver: str = "joint_single_hair"
    cluster_node_limit: Optional[int] = None
    timeout: Optional[float] = None

    def config(self) -> dict[str, dict[str, float]]:
        primal: dict[str, float] = {}
        if self.timeout is not None:
            primal["timeout"] = self.timeout
        if self.cluster_node_limit is not None:
            primal["cluster_node_limit"] = self.cluster_node_limit
        return {"primal": primal}


@dataclass
class CustomExactSolver:
    solver: Callable[
        ["OddCycles", Optional[int], Optional[frozenset[int]]],
        tuple[int, list[int]],
    ]


@dataclass
class CustomBoundsSolver:
    solver: Callable[
        ["OddCycles", Optional[int], Optional[frozenset[int]]],
        tuple[int, int, list[int]],
    ]


ExactSolver = Union[ExhaustiveSolverOptions, CustomExactSolver]
BoundsSolver = Union[ExhaustiveSolverOptions, MwpfSolverOptions, CustomBoundsSolver]


def _is_logical(parity: frozenset[int], coset: Optional[frozenset[int]]) -> bool:
    return bool(parity) if coset is None else len(parity & coset) % 2 == 1


def _residual(matrix: list[frozenset[int]], columns: tuple[int, ...]) -> frozenset[int]:
    return reduce(xor, (matrix[column] for column in columns), frozenset())


def exhaustive_shortest_odd_cycle(
    data: "OddCycles",
    upper_bound: Optional[int],
    coset: Optional[frozenset[int]],
    options: ExhaustiveSolverOptions,
) -> tuple[int, list[int]]:
    count = len(data.check_matrix)
    cap = min(
        value
        for value in (count, upper_bound, options.size_upper_bound)
        if value is not None
    )
    for size in range(1, cap + 1):
        for columns in combinations(range(count), size):
            if _residual(data.check_matrix, columns):
                continue
            if _is_logical(_residual(data.parity_indicators, columns), coset):
                return size, list(columns)
    return count + 1, []


def _is_panic(exception: BaseException) -> bool:
    return type(exception).__name__ == "PanicException"


def _mwpf_solver_class(name: str) -> Any:
    import mwpf

    classes = {
        "joint_single_hair": mwpf.SolverSerialJointSingleHair,
        "single_hair": mwpf.SolverSerialSingleHair,
        "union_find": mwpf.SolverSerialUnionFind,
    }
    if name not in classes:
        raise ValueError(
            f"Unknown mwpf solver {name!r}; expected one of {sorted(classes)}"
        )
    return classes[name]


def _initializer(
    checks: list[frozenset[int]],
    parities: list[frozenset[int]],
    observable: int,
) -> tuple[Any, int]:
    import mwpf

    vertices: dict[int, int] = {}
    edges = []
    for column, check_set in enumerate(checks):
        edge = [vertices.setdefault(check, len(vertices)) for check in check_set]
        edges.append((edge, observable in parities[column]))
    boundary = len(vertices)
    hyper_edges = [
        mwpf.HyperEdge(edge + [boundary] if touches else edge, 1.0)
        for edge, touches in edges
    ]
    return mwpf.SolverInitializer(boundary + 1, hyper_edges), boundary


def _lower_bound(solver: Any, default: int) -> int:
    try:
        _, weight_range = solver.subgraph_range()
        return max(1, math.ceil(float(weight_range.lower.float()) - 1e-9))
    except BaseException as exception:
        if not _is_panic(exception):
            raise
        return default


def _solve_observable(
    checks: list[frozenset[int]],
    parities: list[frozenset[int]],
    observable: int,
    options: MwpfSolverOptions,
) -> Optional[tuple[int, int, list[int]]]:
    import mwpf

    initializer, boundary = _initializer(checks, parities, observable)
    solver = _mwpf_solver_class(options.solver)(initializer, options.config())
    try:
        solver.solve(mwpf.SyndromePattern([boundary]))
        subgraph = list(solver.subgraph())
    except BaseException as exception:
        if not _is_panic(exception):
            raise
        return None
    columns = tuple(subgraph)
    if _residual(checks, columns):
        return None
    if observable not in _residual(parities, columns):
        return None
    return _lower_bound(solver, len(subgraph)), len(subgraph), subgraph


def mwpf_bounds(
    data: "OddCycles",
    upper_bound: Optional[int],
    coset: Optional[frozenset[int]],
    options: MwpfSolverOptions,
) -> tuple[int, int, list[int]]:
    del upper_bound
    observables = (
        sorted(coset)
        if coset is not None
        else sorted(
            {
                observable
                for indicator in data.parity_indicators
                for observable in indicator
            }
        )
    )
    witnesses = [
        result
        for observable in observables
        if (
            result := _solve_observable(
                data.check_matrix,
                data.parity_indicators,
                observable,
                options,
            )
        )
        is not None
    ]
    if not witnesses:
        unreachable = len(data.check_matrix) + 1
        return unreachable, unreachable, []
    lower = min(item[0] for item in witnesses)
    best = min(witnesses, key=lambda item: item[1])
    return lower, best[1], best[2]


__all__ = [
    "BoundsSolver",
    "CustomBoundsSolver",
    "CustomExactSolver",
    "ExactSolver",
    "ExhaustiveSolverOptions",
    "MwpfSolverOptions",
]
