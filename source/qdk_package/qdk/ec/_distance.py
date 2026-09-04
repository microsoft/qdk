"""Internal code-distance analysis."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional, Sequence, Union

import qodec as qc

from ._analysis.code_algebra import (
    SubsystemCode,
    logical_effect_indicators_of,
    one_qubit_errors_on_support,
    subsystem_code_of,
    syndrome_indicators_of,
)
from ._analysis.distance_solvers import (
    BoundsSolver,
    CustomBoundsSolver,
    CustomExactSolver,
    ExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
)
from ._analysis.odd_cycles import OddCycles, cycle_labels
from ._analysis.propagation.pauli import Pauli

Errors = Union[str, Sequence[Pauli]]


def _code_view(code: qc.Code | SubsystemCode) -> SubsystemCode:
    if isinstance(code, qc.Code):
        return subsystem_code_of(code)
    if isinstance(code, SubsystemCode):
        return code
    raise TypeError(f"expected qodec.Code, got {type(code).__name__}")


def _errors_of(code: SubsystemCode, errors: Errors) -> list[Pauli]:
    return (
        one_qubit_errors_on_support(code, errors)
        if isinstance(errors, str)
        else list(errors)
    )


@dataclass
class CodeDistanceData:
    code: SubsystemCode
    errors: list[Pauli]
    odd_cycles: OddCycles

    @staticmethod
    def of(code: qc.Code | SubsystemCode, errors: Errors = "XZ") -> "CodeDistanceData":
        view = _code_view(code)
        error_paulis = _errors_of(view, errors)
        return CodeDistanceData(
            view,
            error_paulis,
            OddCycles(
                syndrome_indicators_of(view, error_paulis),
                logical_effect_indicators_of(view, error_paulis),
            ),
        )

    def parity_indicator(self, operator: Optional[Pauli]) -> Optional[frozenset[int]]:
        if operator is None:
            return None
        return frozenset(
            index
            for index, logical in enumerate(self.code.logical_basis)
            if not logical.commutes_with(operator)
        )


def code_distance_of(
    code: qc.Code | SubsystemCode,
    *,
    errors: Errors = "XZ",
    distance_upper_bound: Optional[int] = None,
    coset_representative: Optional[Pauli] = None,
    solver: Optional[ExactSolver] = None,
) -> tuple[int, list[Pauli]]:
    """Return the exact distance of ``code`` and a minimum-weight witness."""
    data = CodeDistanceData.of(code, errors)
    size, cycle = data.odd_cycles.shortest(
        solver or ExhaustiveSolverOptions(),
        coset_indicator=data.parity_indicator(coset_representative),
        cycle_size_upper_bound=distance_upper_bound,
    )
    return size, cycle_labels(cycle, data.errors)


def code_distance_bounds_of(
    code: qc.Code | SubsystemCode,
    *,
    errors: Errors = "XZ",
    distance_upper_bound: Optional[int] = None,
    coset_representative: Optional[Pauli] = None,
    solver: Optional[BoundsSolver] = None,
) -> tuple[int, int, list[Pauli]]:
    """Return lower/upper distance bounds for ``code`` and a witness."""
    data = CodeDistanceData.of(code, errors)
    lower, upper, cycle = data.odd_cycles.bounds(
        odd_cycle_length_upper_bound=distance_upper_bound,
        coset_indicator=data.parity_indicator(coset_representative),
        solver=solver or MwpfSolverOptions(),
    )
    return lower, upper, cycle_labels(cycle, data.errors)


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
