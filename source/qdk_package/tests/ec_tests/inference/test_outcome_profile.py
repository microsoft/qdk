"""Tests for outcome-profile computation."""
from qdk.ec._qodec_compat import check_outcomes, observables_as_xor_map
from qdk.ec.profile import OutcomeProfile, essential_checks_of, outcome_profile_of
import qodec


def test_outcome_profile_defaults_to_essential_checks(idle_gadget: qodec.Gadget) -> None:
    profile = outcome_profile_of(idle_gadget)
    assert isinstance(profile, OutcomeProfile)
    assert tuple(profile.checks) == essential_checks_of(idle_gadget)


def test_outcome_profile_non_essential_keeps_declared_checks(idle_gadget: qodec.Gadget) -> None:
    profile = outcome_profile_of(idle_gadget, essential=False)
    assert len(profile.checks) == len(idle_gadget.checks)
    for declared, parsed in zip(idle_gadget.checks, profile.checks):
        assert parsed == frozenset(check_outcomes(declared))


def test_outcome_profile_observables_pair_objective_and_realisation(measure_xx_gadget: qodec.Gadget) -> None:
    profile = outcome_profile_of(measure_xx_gadget)
    observables = list(observables_as_xor_map(measure_xx_gadget).values())
    assert len(profile.observables) == len(observables)
    for objective_outcome, (paired_objective, realisation_outcomes) in enumerate(
        profile.observables
    ):
        assert paired_objective == objective_outcome
        assert realisation_outcomes == frozenset(
            observables[objective_outcome]
        )
