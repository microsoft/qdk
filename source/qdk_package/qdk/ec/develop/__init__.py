"""Develop qodec artifacts: load them, save them, and complete drafts.

Two kinds of operation live here:

*Primitives* (:mod:`qdk.ec.develop.primitives`) move qodecs between disk, memory,
and YAML text — :func:`load`, :func:`save`, :func:`from_yaml`, :func:`to_yaml`.

*Smart tooling* (:mod:`qdk.ec.develop.completion`) does automated analysis and
returns new qodec objects — :func:`complete_gadget` and :func:`complete_qodec`
derive the checks and observable bindings that exact simulation can determine,
so an author only has to write the parts that cannot be inferred.

*Synthesis* (:mod:`qdk.ec.develop.synthesis`) goes one step further:
:func:`qodec_from_code` turns a bare :class:`qodec.Code` into a runnable qodec,
generating a logical instruction set and a textbook circuit for each of its
instructions.
"""

from .completion import complete_gadget, complete_qodec
from .primitives import from_yaml, load, save, to_yaml
from .synthesis import qodec_from_code, synthesis_notes

__all__ = [
    "complete_gadget",
    "complete_qodec",
    "from_yaml",
    "load",
    "qodec_from_code",
    "save",
    "synthesis_notes",
    "to_yaml",
]
