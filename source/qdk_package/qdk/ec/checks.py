"""The deterministic parity structure among a gadget's measurement outcomes.

A *check* is a parity of measurement outcomes whose value is fixed in the
absence of faults, so a flip signals that something went wrong. Checks are what
a decoder consumes, and they are discovered by exact simulation rather than
authored by hand — see :func:`~qdk.ec.complete_gadget`, which writes them back
into a gadget.

:func:`checks_of` reports every deterministic parity a channel admits;
:func:`essential_checks_of` reduces those to an independent generating set;
:func:`outcome_code_of` presents the whole outcome structure as a classical code.

What those outcomes *mean* — which parity carries the logical answer — is the
subject of :mod:`qdk.ec.readouts`.
"""

from ._analysis.check_discovery import Profile, checks_of, profile_of
from ._analysis.essential_checks import essential_checks_of
from ._analysis.outcome_code import OutcomeCode, outcome_code_of

__all__ = [
    "OutcomeCode",
    "Profile",
    "checks_of",
    "essential_checks_of",
    "outcome_code_of",
    "profile_of",
]
