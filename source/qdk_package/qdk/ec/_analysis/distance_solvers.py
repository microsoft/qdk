"""Internal distance backends sharing a certified-bounds result."""

from __future__ import annotations

import math
from dataclasses import dataclass
from functools import reduce
from importlib import import_module
from itertools import combinations
from operator import xor
from typing import Any, Callable, Literal, Optional, TYPE_CHECKING, Union

from binar import BitMatrix, BitVector

if TYPE_CHECKING:
    from .odd_cycles import OddCycles


@dataclass
class EnumerationSolverOptions:
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


@dataclass(kw_only=True)
class HighsSolverOptions:
    timeout: float | None = None


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


Solver = Union[
    Literal["enumeration", "mwpf", "highs"],
    EnumerationSolverOptions,
    MwpfSolverOptions,
    HighsSolverOptions,
    CustomExactSolver,
    CustomBoundsSolver,
]
ExactSolver = Solver
BoundsSolver = Solver


def solver_options(solver: Solver) -> Solver:
    if isinstance(solver, str):
        if solver == "enumeration":
            return EnumerationSolverOptions()
        if solver == "mwpf":
            return MwpfSolverOptions()
        if solver == "highs":
            return HighsSolverOptions()
        raise ValueError("solver must be 'enumeration', 'mwpf', or 'highs'")
    return solver


def solve_bounds(
    data: "OddCycles",
    upper_bound: Optional[int],
    coset: Optional[frozenset[int]],
    solver: Solver,
) -> tuple[int, int, list[int]]:
    """Return certified bounds in reduced-column coordinates for every backend."""
    if isinstance(solver, EnumerationSolverOptions):
        return enumeration_bounds(data, upper_bound, coset, solver)
    if isinstance(solver, MwpfSolverOptions):
        return mwpf_bounds(data, upper_bound, coset, solver)
    if isinstance(solver, HighsSolverOptions):
        return highs_bounds(data, upper_bound, coset, solver)
    if isinstance(solver, CustomBoundsSolver):
        return solver.solver(data, upper_bound, coset)
    if isinstance(solver, CustomExactSolver):
        size, cycle = solver.solver(data, upper_bound, coset)
        return size, size, cycle
    raise NotImplementedError(f"Unsupported solver {solver!r}")


def _is_logical(parity: frozenset[int], coset: Optional[frozenset[int]]) -> bool:
    return bool(parity) if coset is None else len(parity & coset) % 2 == 1


def _residual(matrix: list[frozenset[int]], columns: tuple[int, ...]) -> frozenset[int]:
    return reduce(xor, (matrix[column] for column in columns), frozenset())


def enumeration_bounds(
    data: "OddCycles",
    upper_bound: Optional[int],
    coset: Optional[frozenset[int]],
    options: EnumerationSolverOptions,
) -> tuple[int, int, list[int]]:
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
                return size, size, list(columns)
    return cap + 1, count + 1, []


def highs_bounds(
    data: "OddCycles",
    upper_bound: int | None,
    coset: frozenset[int] | None,
    options: HighsSolverOptions,
) -> tuple[int, int, list[int]]:
    """Minimize unit-cost faults using parity constraints and integer slack."""
    if options.timeout is not None and (
        not math.isfinite(options.timeout) or options.timeout < 0
    ):
        raise ValueError("HiGHS timeout must be finite and nonnegative")
    try:
        highs: Any = import_module("highspy")
    except ModuleNotFoundError as exception:
        if exception.name != "highspy":
            raise
        raise ImportError(
            "HiGHS is required for this solver; install it with pip install 'qdk[ec]'"
        ) from exception
    count = len(data.check_matrix)
    cap = count if upper_bound is None else min(count, upper_bound)
    model = highs.Highs()

    def require_ok(status: Any) -> None:
        if status != highs.HighsStatus.kOk:
            raise RuntimeError(f"HiGHS rejected model configuration: {status}")

    require_ok(model.setOptionValue("output_flag", False))
    require_ok(model.setOptionValue("mip_rel_gap", 0.0))
    require_ok(model.setOptionValue("mip_abs_gap", 0.0))
    if options.timeout is not None:
        require_ok(model.setOptionValue("time_limit", options.timeout))
    variables = [int(model.addBinary(obj=1.0)) for _ in range(count)]
    checks: dict[int, list[int]] = {}
    parities: dict[int, list[int]] = {}
    for column, (check, indicator) in enumerate(
        zip(data.check_matrix, data.parity_indicators, strict=True)
    ):
        for index in sorted(check):
            checks.setdefault(index, []).append(variables[column])
        selected = (
            indicator
            if coset is None
            else (frozenset({0}) if len(indicator & coset) % 2 else frozenset())
        )
        for index in sorted(selected):
            parities.setdefault(index, []).append(variables[column])
    if not parities:
        return count + 1, count + 1, []
    changed = []
    for is_logical, rows in ((False, checks), (True, parities)):
        for support in rows.values():
            slack = int(model.addIntegral(lb=0, ub=len(support) // 2))
            indices = support + [slack]
            coefficients = [1.0] * len(support) + [-2.0]
            if is_logical:
                indicator = int(model.addBinary())
                changed.append(indicator)
                indices.append(indicator)
                coefficients.append(-1.0)
            require_ok(model.addRow(0.0, 0.0, len(indices), indices, coefficients))
    require_ok(
        model.addRow(1.0, highs.kHighsInf, len(changed), changed, [1.0] * len(changed))
    )
    if cap < count:
        require_ok(model.addRow(0.0, float(cap), count, variables, [1.0] * count))
    if model.run() == highs.HighsStatus.kError:
        raise RuntimeError("HiGHS failed to solve the distance model")
    status = model.getModelStatus()
    if status == highs.HighsModelStatus.kInfeasible:
        return cap + 1, count + 1, []
    limited = (
        highs.HighsModelStatus.kTimeLimit,
        highs.HighsModelStatus.kIterationLimit,
        highs.HighsModelStatus.kSolutionLimit,
        highs.HighsModelStatus.kInterrupt,
    )
    if status != highs.HighsModelStatus.kOptimal and status not in limited:
        raise RuntimeError(
            f"HiGHS did not return usable distance bounds: {model.modelStatusToString(status)}"
        )
    info = model.getInfo()
    solution = model.getSolution()
    if not info.valid or not solution.value_valid:
        raise RuntimeError("HiGHS stopped without a feasible distance witness")
    values = [float(solution.col_value[index]) for index in variables]
    if any(
        not math.isfinite(value) or min(abs(value), abs(value - 1)) > 1e-6
        for value in values
    ):
        raise RuntimeError("HiGHS returned a nonbinary distance witness")
    cycle = [index for index, value in enumerate(values) if value > 0.5]
    if (
        len(cycle) > cap
        or _residual(data.check_matrix, tuple(cycle))
        or not _is_logical(_residual(data.parity_indicators, tuple(cycle)), coset)
    ):
        raise RuntimeError("HiGHS returned an invalid distance witness")
    dual_bound = float(info.mip_dual_bound)
    if not math.isfinite(dual_bound):
        raise RuntimeError("HiGHS returned an invalid distance lower bound")
    lower = max(1, math.ceil(dual_bound - 1e-6))
    if lower > len(cycle):
        raise RuntimeError("HiGHS lower bound exceeds its witness weight")
    if status == highs.HighsModelStatus.kOptimal and lower != len(cycle):
        raise RuntimeError("HiGHS optimum has inconsistent distance bounds")
    return lower, len(cycle), cycle


def _is_panic(exception: BaseException) -> bool:
    return type(exception).__name__ == "PanicException"


def _mwpf_solver_class(name: str) -> Any:
    mwpf: Any = import_module("mwpf")

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
    mwpf: Any = import_module("mwpf")

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


def _lower_bound(solver: Any) -> int:
    try:
        _, weight_range = solver.subgraph_range()
        lower = float(weight_range.lower.float())
    except BaseException as exception:
        if not _is_panic(exception):
            raise
        raise RuntimeError("MWPF could not certify a lower bound") from exception
    if not math.isfinite(lower) or lower < 0:
        raise RuntimeError("MWPF returned an invalid lower bound")
    return max(1, math.ceil(lower - 1e-9))


def _solve_observable(
    checks: list[frozenset[int]],
    parities: list[frozenset[int]],
    observable: int,
    options: MwpfSolverOptions,
) -> tuple[int, int, list[int]]:
    mwpf: Any = import_module("mwpf")

    initializer, boundary = _initializer(checks, parities, observable)
    solver = _mwpf_solver_class(options.solver)(initializer, options.config())
    try:
        solver.solve(mwpf.SyndromePattern([boundary]))
        subgraph = list(solver.subgraph())
    except BaseException as exception:
        if not _is_panic(exception):
            raise
        raise RuntimeError(
            f"MWPF failed for logical indicator {observable}"
        ) from exception
    columns = tuple(subgraph)
    if (
        len(set(columns)) != len(columns)
        or any(not 0 <= column < len(checks) for column in columns)
        or _residual(checks, columns)
        or observable not in _residual(parities, columns)
    ):
        raise RuntimeError(
            f"MWPF returned an invalid witness for logical indicator {observable}"
        )
    lower = _lower_bound(solver)
    if lower > len(subgraph):
        raise RuntimeError("MWPF lower bound exceeds its witness weight")
    return lower, len(subgraph), subgraph


def mwpf_bounds(
    data: "OddCycles",
    upper_bound: Optional[int],
    coset: Optional[frozenset[int]],
    options: MwpfSolverOptions,
) -> tuple[int, int, list[int]]:
    """Combine certified bounds, skipping only algebraically impossible searches."""
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
    unreachable = len(data.check_matrix) + 1
    if not observables:
        return unreachable, unreachable, []
    check_indices = {
        index: position
        for position, index in enumerate(
            sorted({index for column in data.check_matrix for index in column})
        )
    }
    transpose = BitMatrix.zeros(len(data.check_matrix), len(check_indices))
    for row, column in enumerate(data.check_matrix):
        for index in column:
            transpose[row, check_indices[index]] = True
    undetectable = tuple(transpose.T.kernel().rows)
    witnesses = []
    for observable in observables:
        indicator = BitVector(observable in column for column in data.parity_indicators)
        if not any(indicator.dot(cycle) for cycle in undetectable):
            continue
        witnesses.append(
            _solve_observable(
                data.check_matrix, data.parity_indicators, observable, options
            )
        )
    if not witnesses:
        return unreachable, unreachable, []
    lower = min(item[0] for item in witnesses)
    best = min(witnesses, key=lambda item: item[1])
    return lower, best[1], best[2]


__all__ = [
    "BoundsSolver",
    "CustomBoundsSolver",
    "CustomExactSolver",
    "ExactSolver",
    "EnumerationSolverOptions",
    "MwpfSolverOptions",
]
