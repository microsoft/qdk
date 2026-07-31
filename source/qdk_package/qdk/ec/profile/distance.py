"""Code and gadget distance characteristics and witnesses."""

from __future__ import annotations

from typing import Any

import qodec

from .code_algebra import SubsystemCode
from .code_distance import (
    CodeDistanceData,
    code_distance_bounds_of_view,
    code_distance_of_view,
)
from .distance_solvers import (
    BoundsSolver,
    CustomBoundsSolver,
    CustomExactSolver,
    ExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
)
from .odd_cycles import (
    OddCycles,
    cycle_labels,
    unique_non_empty_elements_of,
)
from .propagation.pauli import Pauli


def _code_view(code: object) -> SubsystemCode:
    if isinstance(code, qodec.Code):
        return SubsystemCode.from_qodec(code)
    if isinstance(code, SubsystemCode):
        return code
    raise TypeError(f"expected qodec.Code, got {type(code).__name__}")


def code_distance_of(code: object, **kwargs: Any) -> tuple[int, list[Pauli]]:
    """Return distance and a witness for a qodec code definition."""
    return code_distance_of_view(_code_view(code), **kwargs)


def code_distance_bounds_of(
    code: object, **kwargs: Any
) -> tuple[int, int, list[Pauli]]:
    """Return lower/upper distance bounds and a witness for a qodec code."""
    return code_distance_bounds_of_view(_code_view(code), **kwargs)


__all__ = [
    "BoundsSolver",
    "CodeDistanceData",
    "CustomBoundsSolver",
    "CustomExactSolver",
    "ExactSolver",
    "ExhaustiveSolverOptions",
    "MwpfSolverOptions",
    "OddCycles",
    "code_distance_bounds_of",
    "code_distance_of",
    "cycle_labels",
    "unique_non_empty_elements_of",
]
