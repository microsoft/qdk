"""Coerce a `Program | source` argument into a `Program`.

Targets accept either a pre-built `Program` or a source value (str
text, `Path` to a source file, or a native frontend object such as a
``cirq.Circuit``). This helper centralises the dispatch so every
target's ``execute`` can do the conversion in one line.
"""

from __future__ import annotations

import qodec
from qodec.circuits import Program


def coerce_program(program: object, isa: qodec.InstructionSet) -> Program:
    """Return ``program`` if it's already a `Program`; otherwise parse it."""
    if isinstance(program, Program):
        return program
    from qodec.circuits import parse  # imported lazily so parsing deps stay optional

    return parse(program, isa)
