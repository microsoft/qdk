"""Diagnose a qodec: structured checks that it says what its author meant.

Where :mod:`qdk.ec.equivalence` compares two artifacts, linting inspects one and
reports what looks wrong. :func:`diagnose` runs a rule set over a whole qodec —
or a single code, instruction set, or gadget — and returns a :class:`Report` of
:class:`Diagnostic` objects, each naming the rule that fired, the object it fired
on, and why.

Rules are ordered by phase: a structural failure suppresses the semantic rules
that depend on it, so a malformed gadget reports one root cause rather than a
cascade. :func:`why_not_valid` reduces a single gadget's report to one sentence.
"""

from ._auditor import Auditor, audit as diagnose
from ._diagnostic import Diagnostic, Phase
from ._gadget import why_not_valid
from ._report import Report
from ._rule import Rule
from ._severity import Severity

__all__ = [
    "Auditor",
    "Diagnostic",
    "Phase",
    "Report",
    "Rule",
    "Severity",
    "diagnose",
    "why_not_valid",
]
