"""Evaluate audit rules to find authoring mistakes in a qodec.

Where equivalence compares two artifacts, auditing inspects one and reports what
looks wrong. :func:`audit` runs the rule set over a whole qodec and returns a
:class:`Report` of errors, warnings, and informational notes. Each message is a
:class:`Diagnostic` naming the audit rule, the affected object, and the finding.

Rules are ordered by phase: a structural failure suppresses the semantic rules
that depend on it, so a malformed gadget reports one root cause rather than a
cascade.

A whole-qodec audit requires an explicit ``layer.codes`` binding for every block
type in each encoded layer, matching its gadget encodings. The bottom physical
layer needs no code bindings. This completeness check is stricter than
``qodec.Qodec.validate()``, which permits bindings supplied only by encodings.
"""

from ._auditor import Auditor, audit
from ._diagnostic import Diagnostic, Phase, Severity
from ._report import Report
from ._rule import Rule

__all__ = [
    "Auditor",
    "Diagnostic",
    "Phase",
    "Report",
    "Rule",
    "Severity",
    "audit",
]
