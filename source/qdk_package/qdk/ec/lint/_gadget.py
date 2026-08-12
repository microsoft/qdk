"""Single-gadget audit convenience."""

import qodec

from .._qodec_compat import realization
from ._auditor import Auditor


def why_not_valid(gadget: qodec.Gadget) -> str:
    channel = realization(gadget)
    if not channel.encoding_in and not channel.encoding_out:
        return "Channel has no input or output encoding."
    errors = Auditor().audit_gadget(gadget).errors()
    if not errors:
        return ""
    first = errors[0]
    return f"{first.summary}: {first.detail}" if first.detail else first.summary


__all__ = ["why_not_valid"]
