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

from ._analysis.check_discovery import Profile, profile_of
from ._analysis.essential_checks import outcomes_flipped_by_anti_observables_of
from ._analysis.outcome_profile import OutcomeProfile, outcome_profile_of

__all__ = [
    "OutcomeProfile",
    "Profile",
    "outcome_profile_of",
    "outcomes_flipped_by_anti_observables_of",
    "profile_of",
]
