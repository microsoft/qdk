"""Single-gadget audit convenience."""

import qodec as qc

from ._auditor import Auditor


def why_not_valid(gadget: qc.Gadget) -> str:
    if not gadget.inputs and not gadget.outputs:
        return "Gadget has no input or output encoding."
    errors = Auditor().audit_gadget(gadget).errors()
    if not errors:
        return ""
    first = errors[0]
    return f"{first.summary}: {first.detail}" if first.detail else first.summary


__all__ = ["why_not_valid"]
