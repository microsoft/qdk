"""Tests for essential-check profiling."""
import qodec
from qdk.ec._qodec_compat import check_outcomes, realization
from qdk.ec.checks import essential_checks_of
from qdk.ec.readouts import outcomes_flipped_by_anti_observables_of


def test_anti_observable_flips_one_per_logical_basis_element(idle_gadget: qodec.Gadget) -> None:
    flips = outcomes_flipped_by_anti_observables_of(idle_gadget)
    expected_count = sum(
        len(list(encoding.code.x)) * 2
        for encoding in realization(idle_gadget).encoding_in
    )
    assert len(flips) == expected_count
    for flip in flips:
        assert isinstance(flip, frozenset)


def test_essential_checks_collapse_duplicate_checks(idle_gadget: qodec.Gadget) -> None:
    declared = tuple(frozenset(check_outcomes(atoms)) for atoms in idle_gadget.checks)
    essential = essential_checks_of(idle_gadget)
    assert len(set(essential)) == len(essential)
    assert len(set(essential)) <= len(set(declared))
