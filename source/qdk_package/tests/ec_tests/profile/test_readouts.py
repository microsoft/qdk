"""What a gadget's measurement outcomes mean."""

from __future__ import annotations

import qodec as qc

from qdk.ec import _checks as checks_module
from qdk.ec._analysis import check_discovery
from qdk.ec._analysis.essential_checks import (
    outcomes_flipped_by_anti_observables_of,
)


def test_profile_of_discovers_the_readout_bindings(
    measure_zz_gadget: qc.Gadget,
) -> None:
    profile = check_discovery.profile_of(measure_zz_gadget)

    assert profile.readouts, "measure_zz binds at least one readout"
    assert all(
        isinstance(name, str) and all(isinstance(index, int) for index in outcomes)
        for name, outcomes in profile.readouts.items()
    )


def test_checks_and_readouts_share_one_discovery_pass() -> None:
    """Both views come from the same simulation, so they cannot disagree."""
    assert check_discovery.profile_of is checks_module.profile_of


def test_anti_observable_flips_are_reported_per_outcome(
    measure_zz_gadget: qc.Gadget,
) -> None:
    flipped = outcomes_flipped_by_anti_observables_of(measure_zz_gadget)

    assert all(isinstance(entry, frozenset) for entry in flipped)
    assert any(
        entry for entry in flipped
    ), "measuring ZZ must be flipped by some anti-observable"


def test_idle_gadget_has_no_readouts(idle_gadget: qc.Gadget) -> None:
    assert check_discovery.profile_of(idle_gadget).readouts == {}
