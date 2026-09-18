"""Internal code and circuit-fault distance adapters."""

from __future__ import annotations

from collections.abc import Callable, Iterator
from copy import deepcopy
from dataclasses import dataclass
from functools import reduce
from operator import mul
from typing import Optional, Sequence, TypeVar, Union

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
    EnumerationSolverOptions,
    HighsSolverOptions,
    MwpfSolverOptions,
)
from ._analysis.odd_cycles import OddCycles, cycle_labels
from ._analysis.propagation.pauli import Pauli
from ._code_profile import CodeProfile
from ._distance_result import Distance
from ._faults import FaultEffect, FaultEvent

Errors = Union[str, Sequence[Pauli]]
Factor = TypeVar("Factor")


def distance_result_of(
    problem: OddCycles,
    factors: Sequence[Factor],
    *,
    solver: ExactSolver,
    upper_bound: int | None = None,
    coset_indicator: frozenset[int] | None = None,
    exact: bool = False,
    product: Callable[[tuple[Factor, ...]], Factor],
    copy: Callable[[Factor], Factor] = deepcopy,
) -> Distance[Factor]:
    snapshot = tuple(copy(factor) for factor in factors)
    lower, upper, cycle = problem.bounds(upper_bound, coset_indicator, solver)
    if exact and lower != upper:
        raise RuntimeError(
            f"solver did not prove exact distance: bounds are {lower} and {upper}"
        )
    if not cycle:
        return Distance._create(None if lower == upper else lower, None)
    selected = tuple(sorted(cycle))
    witness = Distance.Witness._create(
        tuple(snapshot[index] for index in selected), product=product, copy=copy
    )

    def alternatives() -> Iterator[Distance.Witness[Factor]]:
        for selection in problem.witnesses(upper, coset_indicator):
            if selection != selected:
                yield Distance.Witness._create(
                    tuple(snapshot[index] for index in selection),
                    product=product,
                    copy=copy,
                )

    return Distance._create(lower, upper, witness=witness, alternatives=alternatives)


def _pauli_product(factors: tuple[Pauli, ...]) -> Pauli:
    return reduce(mul, factors, Pauli.identity())


def _fault_product(factors: tuple[FaultEvent, ...]) -> FaultEvent:
    return reduce(mul, factors, FaultEvent())


def _copy_fault(fault: FaultEvent) -> FaultEvent:
    return FaultEvent._from_locations(
        {
            call: (error.copy(), flips)
            for call, (error, flips) in fault._locations.items()
        }
    )


@dataclass
class _FaultDistanceData:
    faults: tuple[FaultEvent, ...]
    odd_cycles: OddCycles

    @classmethod
    def of(
        cls,
        faults: tuple[FaultEvent, ...],
        effects: Sequence[FaultEffect],
        output_syndromes: Sequence[frozenset[int]],
        indicators: Sequence[frozenset[int]],
        *,
        flag_positions: frozenset[int] = frozenset(),
    ) -> _FaultDistanceData:
        output_offset = 1 + max(
            (index for effect in effects for index in effect.syndrome), default=-1
        )
        flag_offset = output_offset + 1 + max(
            (index for syndrome in output_syndromes for index in syndrome), default=-1
        )
        constraints = [
            effect.syndrome
            | frozenset(output_offset + index for index in output_syndrome)
            | frozenset(
                flag_offset + index for index in effect.readout_flips & flag_positions
            )
            for effect, output_syndrome in zip(effects, output_syndromes, strict=True)
        ]
        return cls(faults, OddCycles(constraints, indicators))


def _code_view(code: qc.Code | CodeProfile | SubsystemCode) -> SubsystemCode:
    if isinstance(code, qc.Code):
        return subsystem_code_of(code)
    if isinstance(code, CodeProfile):
        return code._algebra
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
    def of(
        code: qc.Code | CodeProfile | SubsystemCode, errors: Errors = "XYZ"
    ) -> "CodeDistanceData":
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
    code: qc.Code | CodeProfile | SubsystemCode,
    *,
    errors: Errors = "XYZ",
    distance_upper_bound: Optional[int] = None,
    coset_representative: Optional[Pauli] = None,
    solver: Optional[ExactSolver] = None,
) -> tuple[int, list[Pauli]]:
    """Return the minimum allowed-error count and its list of Pauli factors."""
    data = CodeDistanceData.of(code, errors)
    size, cycle = data.odd_cycles.shortest(
        HighsSolverOptions() if solver is None else solver,
        coset_indicator=data.parity_indicator(coset_representative),
        cycle_size_upper_bound=distance_upper_bound,
    )
    return size, cycle_labels(cycle, data.errors)


def code_distance_bounds_of(
    code: qc.Code | CodeProfile | SubsystemCode,
    *,
    errors: Errors = "XYZ",
    distance_upper_bound: Optional[int] = None,
    coset_representative: Optional[Pauli] = None,
    solver: Optional[BoundsSolver] = None,
) -> tuple[int, int, list[Pauli]]:
    """Bound the allowed-error count and return factors witnessing the upper bound."""
    data = CodeDistanceData.of(code, errors)
    lower, upper, cycle = data.odd_cycles.bounds(
        odd_cycle_length_upper_bound=distance_upper_bound,
        coset_indicator=data.parity_indicator(coset_representative),
        solver=HighsSolverOptions() if solver is None else solver,
    )
    return lower, upper, cycle_labels(cycle, data.errors)


__all__ = [
    "BoundsSolver",
    "CodeDistanceData",
    "CustomBoundsSolver",
    "CustomExactSolver",
    "ExactSolver",
    "EnumerationSolverOptions",
    "MwpfSolverOptions",
    "OddCycles",
    "CodeProfile",
    "code_distance_bounds_of",
    "code_distance_of",
]
