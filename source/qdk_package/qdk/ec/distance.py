"""Code distance: how much protection a code actually provides.

:func:`code_distance_of` computes the exact distance together with a witness —
a minimum-weight logical operator that realizes it. :func:`code_distance_bounds_of`
returns bounds instead, which is what you want for codes too large to solve
exactly.

Both accept ``**options`` selecting a solver: :class:`ExhaustiveSolverOptions`
for an exact search, or :class:`MwpfSolverOptions` for the matching-based
bound (needs the ``mwpf`` backend).

The *circuit-level* analogue — the distance a compiled circuit achieves, which
is the number that says whether an artifact inherits its code's protection —
lives in :mod:`qdk.ec.targets`.
"""

from __future__ import annotations

from typing import Any

import qodec

from ._analysis.code_algebra import SubsystemCode
from ._analysis.code_distance import (
    CodeDistanceData,
    code_distance_bounds_of_view,
    code_distance_of_view,
)
from ._analysis.distance_solvers import (
    BoundsSolver,
    CustomBoundsSolver,
    CustomExactSolver,
    ExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
)
from ._analysis.odd_cycles import OddCycles
from ._analysis.propagation.pauli import Pauli


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
    "SubsystemCode",
    "code_distance_bounds_of",
    "code_distance_of",
]
