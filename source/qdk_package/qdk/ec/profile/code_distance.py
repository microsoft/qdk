"""Distance of an algebraic stabilizer-code view."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional, Sequence, Union

from .propagation.pauli import Pauli
from .code_algebra import (
    SubsystemCode,
    logical_effect_indicators_of,
    one_qubit_errors_on_support,
    syndrome_indicators_of,
)
from .distance_solvers import (
    BoundsSolver,
    ExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
)
from .odd_cycles import OddCycles, cycle_labels

Errors = Union[str, Sequence[Pauli]]


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
    def of(code: SubsystemCode, errors: Errors = "XZ") -> "CodeDistanceData":
        error_paulis = _errors_of(code, errors)
        return CodeDistanceData(
            code,
            error_paulis,
            OddCycles(
                syndrome_indicators_of(code, error_paulis),
                logical_effect_indicators_of(code, error_paulis),
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


def code_distance_of_view(
    code: SubsystemCode,
    *,
    errors: Errors = "XZ",
    distance_upper_bound: Optional[int] = None,
    coset_representative: Optional[Pauli] = None,
    solver: Optional[ExactSolver] = None,
) -> tuple[int, list[Pauli]]:
    data = CodeDistanceData.of(code, errors)
    size, cycle = data.odd_cycles.shortest(
        solver or ExhaustiveSolverOptions(),
        coset_indicator=data.parity_indicator(coset_representative),
        cycle_size_upper_bound=distance_upper_bound,
    )
    return size, cycle_labels(cycle, data.errors)


def code_distance_bounds_of_view(
    code: SubsystemCode,
    *,
    errors: Errors = "XZ",
    distance_upper_bound: Optional[int] = None,
    coset_representative: Optional[Pauli] = None,
    solver: Optional[BoundsSolver] = None,
) -> tuple[int, int, list[Pauli]]:
    data = CodeDistanceData.of(code, errors)
    lower, upper, cycle = data.odd_cycles.bounds(
        odd_cycle_length_upper_bound=distance_upper_bound,
        coset_indicator=data.parity_indicator(coset_representative),
        solver=solver or MwpfSolverOptions(),
    )
    return lower, upper, cycle_labels(cycle, data.errors)


__all__ = [
    "CodeDistanceData",
    "code_distance_bounds_of_view",
    "code_distance_of_view",
]
