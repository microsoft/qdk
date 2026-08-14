"""What a gadget's measurement outcomes mean.

Where :mod:`qdk.ec.checks` answers *which parities are deterministic*, this
module answers *what those outcomes say*: the discovered observable bindings
(:func:`profile_of`), the outcome structure reduced to its essential checks and
observables (:func:`outcome_profile_of`), and which outcomes are flipped by the
anti-observables of the input encoding
(:func:`outcomes_flipped_by_anti_observables_of`).

Like checks, readouts are a *completion* of a gadget: they can be discovered by
exact simulation and written back into a qodec.
"""

from __future__ import annotations

from dataclasses import dataclass

import qodec as qc

from ._analysis.check_discovery import Profile, profile_of
from ._analysis.essential_checks import (
    essential_checks_of,
    outcomes_flipped_by_anti_observables_of,
)
from ._readouts import observables_as_xor_map
from ._references import outcome_indices


@dataclass(frozen=True)
class OutcomeProfile:
    """A gadget's declared checks and observables, as outcome-index parities."""

    checks: tuple[frozenset[int], ...]
    observables: tuple[tuple[int, frozenset[int]], ...]


def outcome_profile_of(
    gadget: qc.Gadget, *, essential: bool = True
) -> OutcomeProfile:
    """Return ``gadget``'s declared check and observable parity structure."""
    declared = tuple(frozenset(outcome_indices(atoms)) for atoms in gadget.checks)
    checks = essential_checks_of(gadget, checks=declared) if essential else declared
    observables = tuple(
        (index, frozenset(outcomes))
        for index, outcomes in enumerate(observables_as_xor_map(gadget).values())
    )
    return OutcomeProfile(checks=checks, observables=observables)


__all__ = [
    "OutcomeProfile",
    "Profile",
    "outcome_profile_of",
    "outcomes_flipped_by_anti_observables_of",
    "profile_of",
]
