"""Tests for gadget-distance estimation."""
from __future__ import annotations

import qodec
from qdk.ec.profile import FaultEffect
from qdk.ec.profile.distance import MwpfSolverOptions
from qdk.ec.targets import (
    GadgetDistanceData,
    depolarizing,
    gadget_distance_bounds_of,
    gadget_distance_of,
)
from ec_tests.testing.optional import requires_mwpf


def test_measure_xx_gadget_distance_is_two(
    measure_xx_gadget: qodec.Gadget,
) -> None:
    distance, witness = gadget_distance_of(measure_xx_gadget, depolarizing(0.001))
    assert distance == 2
    assert len(witness) == 2
    assert all(isinstance(effect, FaultEffect) for effect in witness)


def test_measure_xx_witness_is_an_undetectable_logical_error(
    measure_xx_gadget: qodec.Gadget,
) -> None:
    _, witness = gadget_distance_of(measure_xx_gadget, depolarizing(0.001))
    combined_checks: frozenset[int] = frozenset()
    combined_observables: frozenset[int] = frozenset()
    for effect in witness:
        combined_checks ^= effect.flipped_checks
        combined_observables ^= effect.flipped_observables
    assert combined_checks == frozenset()
    assert len(combined_observables) > 0


@requires_mwpf
def test_mwpf_agrees_with_exhaustive_on_gadget_distance(
    measure_xx_gadget: qodec.Gadget,
) -> None:
    exact, _ = gadget_distance_of(measure_xx_gadget, depolarizing(0.001))
    lower, upper, _ = gadget_distance_bounds_of(
        measure_xx_gadget, depolarizing(0.001), solver=MwpfSolverOptions()
    )
    assert upper == exact
    assert lower <= upper


def test_gadget_distance_data_exposes_propagated_effects(
    measure_xx_gadget: qodec.Gadget,
) -> None:
    data = GadgetDistanceData.of(measure_xx_gadget, depolarizing(0.001))
    assert len(data.effects) > 0
    assert any(effect.flipped_observables for effect in data.effects)


def test_idle_gadget_distance_uses_encoding_residual_observables(
    idle_gadget: qodec.Gadget,
) -> None:
    distance, witness = gadget_distance_of(idle_gadget, depolarizing(0.001))
    assert distance >= 1
    assert all(not effect.flipped_observables for effect in witness)
    combined_checks: frozenset[int] = frozenset()
    has_logical_residual = False
    for effect in witness:
        combined_checks ^= effect.flipped_checks
        if any(residual.support for residual in effect.residuals.values()):
            has_logical_residual = True
    assert combined_checks == frozenset()
    assert has_logical_residual


def test_idle_gadget_mwpf_agrees_with_exhaustive(
    idle_gadget: qodec.Gadget,
) -> None:
    exact, _ = gadget_distance_of(idle_gadget, depolarizing(0.001))
    _, upper, _ = gadget_distance_bounds_of(
        idle_gadget, depolarizing(0.001), solver=MwpfSolverOptions()
    )
    assert upper == exact
