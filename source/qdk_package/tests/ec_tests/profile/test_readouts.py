"""``qdk.ec.profile.readouts`` — what a gadget's measurement outcomes mean."""

from __future__ import annotations

import qodec

from qdk.ec.profile import checks as checks_module
from qdk.ec.profile import readouts


def test_profile_of_discovers_the_observable_bindings(
    measure_zz_gadget: qodec.Gadget,
) -> None:
    profile = readouts.profile_of(measure_zz_gadget)

    assert profile.observables, "measure_zz binds at least one observable"
    assert all(
        isinstance(name, str) and all(isinstance(index, int) for index in outcomes)
        for name, outcomes in profile.observables.items()
    )


def test_readouts_of_is_profile_of() -> None:
    assert readouts.readouts_of is readouts.profile_of


def test_outcome_profile_agrees_with_the_discovered_profile(
    measure_zz_gadget: qodec.Gadget,
) -> None:
    profile = readouts.profile_of(measure_zz_gadget)
    outcome_profile = readouts.outcome_profile_of(measure_zz_gadget)

    assert {
        position: frozenset(outcomes)
        for position, outcomes in enumerate(profile.observables.values())
    } == dict(outcome_profile.observables)


def test_outcome_profile_checks_are_the_essential_checks(
    measure_zz_gadget: qodec.Gadget,
) -> None:
    outcome_profile = readouts.outcome_profile_of(measure_zz_gadget)

    assert outcome_profile.checks == checks_module.essential_checks_of(
        measure_zz_gadget
    )


def test_anti_observable_flips_are_reported_per_outcome(
    measure_zz_gadget: qodec.Gadget,
) -> None:
    flipped = readouts.outcomes_flipped_by_anti_observables_of(measure_zz_gadget)

    assert all(isinstance(entry, frozenset) for entry in flipped)
    assert any(entry for entry in flipped), (
        "measuring ZZ must be flipped by some anti-observable"
    )


def test_idle_gadget_has_no_observables(idle_gadget: qodec.Gadget) -> None:
    assert readouts.profile_of(idle_gadget).observables == {}
