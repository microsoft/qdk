"""Tests for essential-check profiling."""

import qodec as qc
from qdk.ec._references import outcomes_of, parse_equations
from qdk.ec._checks import essential_checks_of
from qdk.ec._analysis.essential_checks import outcomes_flipped_by_anti_observables_of


def test_anti_observable_flips_one_per_logical_basis_element(
    idle_gadget: qc.Gadget,
) -> None:
    flips = outcomes_flipped_by_anti_observables_of(idle_gadget)
    expected_count = sum(
        len(list(encoding.code.x)) * 2 for encoding in idle_gadget.inputs
    )
    assert len(flips) == expected_count
    for flip in flips:
        assert isinstance(flip, frozenset)


def test_essential_checks_collapse_duplicate_checks(idle_gadget: qc.Gadget) -> None:
    declared = tuple(
        frozenset(outcomes_of(check)) for check in parse_equations(idle_gadget.checks)
    )
    essential = essential_checks_of(idle_gadget)
    assert len(set(essential)) == len(essential)
    assert len(set(essential)) <= len(set(declared))
