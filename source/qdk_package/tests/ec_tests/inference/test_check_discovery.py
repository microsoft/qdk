"""Smoke tests for check discovery through `qdk.ec.profile`.

The module's heavy logic is exercised through `audit` and the C4 demo;
this file pins the public surface (`profile_of`, `simulate_channel`,
`Profile`) so a refactor cannot accidentally remove or rename them.
"""
from __future__ import annotations

from qdk.ec.profile import Profile, profile_of
from qdk.ec.profile.propagation import simulate_channel
from qdk.ec._qodec_compat import realization
from ec_tests.testing.qodecs import c4


def test_profile_of_returns_profile_with_checks_and_observables() -> None:
    codec = c4()
    gadget = codec.layers[0].gadgets["measure_zz"]
    profile = profile_of(gadget)
    assert isinstance(profile, Profile)
    assert len(profile.checks) >= 1
    # measure_zz has two objective observe outcomes, named positionally.
    assert set(profile.observables) >= {"0", "1"}


def test_profile_of_idle_round_finds_four_stabilizer_checks() -> None:
    """C4's `idle` realisation runs both X- and Z-stabilizer extractions
    in and out, yielding 4 deterministic checks."""
    codec = c4()
    gadget = codec.layers[0].gadgets["idle"]
    profile = profile_of(gadget)
    assert len(profile.checks) == 4


def test_simulate_channel_with_channel_returns_simulation() -> None:
    codec = c4()
    gadget = codec.layers[0].gadgets["idle"]
    sim = simulate_channel(realization(gadget))
    assert sim.simulation.outcome_count > 0


def test_simulate_channel_with_gadget_records_objective_outcomes() -> None:
    """Passing a gadget tells `simulate_channel` to also probe each
    objective `Observe` Pauli after the walk."""
    codec = c4()
    gadget = codec.layers[0].gadgets["measure_zz"]
    sim = simulate_channel(gadget=gadget)
    assert len(sim.objective_outcomes) == 2
