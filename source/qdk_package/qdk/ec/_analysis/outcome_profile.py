"""Declared check and readout parity structure of a gadget."""

from __future__ import annotations

from dataclasses import dataclass

import qodec

from .._qodec_compat import check_outcomes, observables_as_xor_map
from .essential_checks import essential_checks_of


@dataclass(frozen=True)
class OutcomeProfile:
    checks: tuple[frozenset[int], ...]
    observables: tuple[tuple[int, frozenset[int]], ...]


def outcome_profile_of(
    gadget: qodec.Gadget, *, essential: bool = True
) -> OutcomeProfile:
    declared = tuple(frozenset(check_outcomes(atoms)) for atoms in gadget.checks)
    checks = essential_checks_of(gadget, checks=declared) if essential else declared
    observables = tuple(
        (index, frozenset(outcomes))
        for index, outcomes in enumerate(observables_as_xor_map(gadget).values())
    )
    return OutcomeProfile(checks=checks, observables=observables)


__all__ = ["OutcomeProfile", "outcome_profile_of"]
