"""Target-conditioned fault distance of a qodec gadget."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

import qodec
from qodec.circuits import Program

from .._qodec_compat import realization
from .._analysis.distance_solvers import (
    BoundsSolver,
    ExactSolver,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
)
from ..faults import FaultEffect, fault_profile_of
from .._analysis.odd_cycles import OddCycles
from .._analysis.propagation.pauli import characters_of
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


def circuit_distance_of(
    codec: qodec.Qodec,
    program: Program,
    *,
    noise: Optional[dict] = None,
    max_weight: int = 8,
) -> int:
    """Fault distance of the *whole compiled circuit* for ``program``.

    Lowers ``program`` through ``codec`` to a physical stim circuit and returns
    the smallest number of circuit faults that together flip a logical
    observable while flipping no detector — the circuit-level analogue of code
    distance, and the number that says whether a qodec actually delivers the
    protection its code promises.

    This is a *different* and stricter question than
    :func:`gadget_distance_of`, which scores one gadget in isolation. A single
    round of syndrome extraction can never see a data fault that lands after it
    has already measured its stabilizers, so per-gadget numbers understate a
    memory experiment; only the composed circuit answers the real question.

    ``noise`` is the stim gate-noise model to attach (defaults to uniform
    depolarizing at 0.1%); its magnitudes do not affect the distance, only
    which fault locations exist. ``max_weight`` bounds the search stim performs.

    Requires the ``stim`` backend. Raises :class:`ValueError` if the lowered
    circuit is not well formed — in particular if it carries a detector that is
    not actually deterministic, which means the qodec's declared checks and its
    circuits disagree.
    """
    from .stim import StimEmitter

    emitter = StimEmitter(
        codec, noise=noise if noise is not None else {"p_data": 0.001, "p_meas": 0.001}
    )
    circuit = emitter.build_circuit(program)
    error = circuit.search_for_undetectable_logical_errors(
        dont_explore_detection_event_sets_with_size_above=max_weight,
        dont_explore_edges_with_degree_above=max_weight,
        dont_explore_edges_increasing_symptom_degree=False,
    )
    return len(error)


__all__ = [
    "GadgetDistanceData",
    "circuit_distance_of",
    "gadget_distance_bounds_of",
    "gadget_distance_of",
]
