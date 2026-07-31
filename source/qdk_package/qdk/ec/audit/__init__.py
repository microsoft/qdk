"""Verify that a qodec does what its author intended.

This is the "test" stage of develop/test/deploy. Two kinds of check live here:

*Diagnostics* — :func:`audit` runs a rule set over a whole qodec (or one code,
instruction set, or gadget) and returns a :class:`Report` of structured
:class:`Diagnostic` objects. :func:`why_not_valid` reduces a single gadget's
report to one human-readable sentence.

*Equivalence* (:mod:`qdk.ec.audit.equivalence`) — compare two artifacts, or two
already-computed actions, and say whether they do the same thing.

The check and readout profiles a gadget declares are audited here too, using
:mod:`qdk.ec.profile.checks` and :mod:`qdk.ec.profile.readouts`; those modules
are re-exported as :data:`checks` and :data:`readouts` for convenience.
"""

from ..profile import checks, readouts
from .auditor import Auditor, audit
from .diagnostic import Diagnostic, Phase
from .equivalence import (
    actions_equivalent_mod_pauli,
    actions_outcome_equivalent,
    codes_equivalent,
    gadgets_equivalent,
    why_not_equivalent,
)
from .gadget import why_not_valid
from .report import Report
from .rule import Rule
from .severity import Severity

__all__ = [
    "Auditor",
    "Diagnostic",
    "Phase",
    "Report",
    "Rule",
    "Severity",
    "actions_equivalent_mod_pauli",
    "actions_outcome_equivalent",
    "audit",
    "checks",
    "codes_equivalent",
    "gadgets_equivalent",
    "readouts",
    "why_not_equivalent",
    "why_not_valid",
]
