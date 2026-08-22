"""Tests for outcome-profile computation."""

from qdk.ec._readouts import observables_as_xor_map
from qdk.ec._references import outcomes_of, parse_equation
from qdk.ec.checks import essential_checks_of
from qdk.ec.readouts import OutcomeProfile, outcome_profile_of
import qodec as qc


def test_outcome_profile_defaults_to_essential_checks(
    idle_gadget: qc.Gadget,
) -> None:
    profile = outcome_profile_of(idle_gadget)
    assert isinstance(profile, OutcomeProfile)
    assert tuple(profile.checks) == essential_checks_of(idle_gadget)


def test_outcome_profile_non_essential_keeps_declared_checks(
    idle_gadget: qc.Gadget,
) -> None:
    profile = outcome_profile_of(idle_gadget, essential=False)
    assert len(profile.checks) == len(idle_gadget.checks)
    for declared, parsed in zip(idle_gadget.checks, profile.checks):
        assert parsed == frozenset(outcomes_of(parse_equation(declared)))


def test_outcome_profile_observables_pair_declared_and_realized(
    measure_xx_gadget: qc.Gadget,
) -> None:
    profile = outcome_profile_of(measure_xx_gadget)
    observables = list(observables_as_xor_map(measure_xx_gadget).values())
    assert len(profile.observables) == len(observables)
    for declared_outcome, (paired_declared, realized_outcomes) in enumerate(
        profile.observables
    ):
        assert paired_declared == declared_outcome
        assert realized_outcomes == frozenset(observables[declared_outcome])
