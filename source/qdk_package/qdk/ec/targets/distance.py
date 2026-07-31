"""Target-conditioned fault distance of a qodec gadget."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

import qodec
from qodec.circuits import Program

from .._qodec_compat import realization
from ..profile.distance_solvers import (
    BoundsSolver,
    ExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
)
from ..profile.faults import FaultEffect, fault_profile_of
from ..profile.odd_cycles import OddCycles
from ..profile.propagation.pauli import characters_of
from .model import TargetModel


def _logical_indicators(
    effects: list[FaultEffect],
) -> list[frozenset[int]]:
    named = {index for effect in effects for index in effect.flipped_observables}
    offset = max(named) + 1 if named else 0
    slots: dict[tuple[str, int, str], int] = {}

    def slot(operand: str, logical: int, basis: str) -> int:
        key = (operand, logical, basis)
        if key not in slots:
            slots[key] = offset + len(slots)
        return slots[key]

    indicators = []
    for effect in effects:
        flipped = set(effect.flipped_observables)
        for operand, residual in effect.residuals.items():
            for logical, character in characters_of(residual).items():
                if character in ("X", "Y"):
                    flipped.add(slot(operand, logical, "Z"))
                if character in ("Z", "Y"):
                    flipped.add(slot(operand, logical, "X"))
        indicators.append(frozenset(flipped))
    return indicators


@dataclass
class GadgetDistanceData:
    effects: list[FaultEffect]
    odd_cycles: OddCycles

    @staticmethod
    def of(gadget: qodec.Gadget, target_model: TargetModel) -> "GadgetDistanceData":
        channel = realization(gadget)
        program = Program(channel.instructions, channel.isa)
        profile = fault_profile_of(gadget, target_model.fault_basis_of(program))
        effects = list(profile.effects)
        return GadgetDistanceData(
            effects,
            OddCycles(
                [effect.flipped_checks for effect in effects],
                _logical_indicators(effects),
            ),
        )


def gadget_distance_of(
    gadget: qodec.Gadget,
    target_model: TargetModel,
    *,
    distance_upper_bound: Optional[int] = None,
    solver: Optional[ExactSolver] = None,
) -> tuple[int, list[FaultEffect]]:
    data = GadgetDistanceData.of(gadget, target_model)
    size, cycle = data.odd_cycles.shortest(
        solver or ExhaustiveSolverOptions(),
        cycle_size_upper_bound=distance_upper_bound,
    )
    return size, [data.effects[index] for index in cycle]


def gadget_distance_bounds_of(
    gadget: qodec.Gadget,
    target_model: TargetModel,
    *,
    distance_upper_bound: Optional[int] = None,
    solver: Optional[BoundsSolver] = None,
) -> tuple[int, int, list[FaultEffect]]:
    data = GadgetDistanceData.of(gadget, target_model)
    lower, upper, cycle = data.odd_cycles.bounds(
        odd_cycle_length_upper_bound=distance_upper_bound,
        solver=solver or MwpfSolverOptions(),
    )
    return lower, upper, [data.effects[index] for index in cycle]


__all__ = [
    "GadgetDistanceData",
    "gadget_distance_bounds_of",
    "gadget_distance_of",
]
