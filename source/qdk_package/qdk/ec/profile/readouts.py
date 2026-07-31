"""Readout characteristics of qodec gadgets.

Where :mod:`qdk.ec.profile.checks` answers *which parities are deterministic*,
this module answers *what a gadget's measurement outcomes mean*: the discovered
observable bindings (:func:`profile_of`), the joint distribution structure of
the outcomes (:func:`outcome_profile_of`), and which outcomes are flipped by the
anti-observables of the input encoding
(:func:`outcomes_flipped_by_anti_observables_of`).
"""

from .check_discovery import Profile, profile_of
from .essential_checks import outcomes_flipped_by_anti_observables_of
from .outcome_profile import OutcomeProfile, outcome_profile_of

#: Alias reading as "the readouts of this gadget".
readouts_of = profile_of

__all__ = [
    "OutcomeProfile",
    "Profile",
    "outcome_profile_of",
    "outcomes_flipped_by_anti_observables_of",
    "profile_of",
    "readouts_of",
]
