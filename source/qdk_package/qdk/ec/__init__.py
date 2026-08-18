"""``qdk.ec`` — develop and test quantum error correction schemes.

A *qodec* is a declarative description of a compilation pipeline together with
the quantum error correction schemes that lower each layer of that pipeline.
The ``qodec`` package defines the file format and the in-memory object model;
``qdk.ec`` is the tooling that works with those objects.

The API is organised around what you are trying to do.

Develop
-------
Move qodecs between disk, memory, and YAML text, and let automated analysis
finish the parts a human should not have to write.

* :func:`load_yaml`, :func:`save_yaml`, :func:`from_yaml`, :func:`to_yaml` —
  moving qodecs between disk, memory, and YAML text.
* :func:`complete_gadget`, :func:`complete_qodec` — derive the checks and
  observable bindings exact simulation can determine.
* :func:`qodec_from_code` — synthesize a whole runnable qodec from a bare
  stabilizer code.

Profile
-------
Compute focused, typed characteristics of a qodec or its parts. Each module
answers one question:

* :mod:`~qdk.ec.action` — what a gadget declares it does, and what its circuit
  actually does.
* :mod:`~qdk.ec.checks` — the deterministic parity structure among measurement
  outcomes.
* :mod:`~qdk.ec.code` — characteristics of :class:`qodec.Code` objects.
* :mod:`~qdk.ec.distance` — code distance, exactly or in bounds.
* :mod:`~qdk.ec.faults` — how a basis of faults reaches the gadget boundary.
* :mod:`~qdk.ec.readouts` — what a gadget's measurement outcomes mean.

Some of these — checks and readouts especially — are *completions* of a gadget
and can be written back into a qodec; others, such as faults and actions, are
information that would not go back in.

Test
----
Verify that a qodec does what its author intended.

* :mod:`~qdk.ec.equivalence` — is this artifact the same as that one, and if
  not, why?
* :mod:`~qdk.ec.lint` — run a rule set over a qodec and get structured
  diagnostics.

Installing
----------
``qdk.ec`` and its dependencies are an optional extra of the ``qdk`` package::

    pip install "qdk[ec]"

Example
-------
>>> import qdk.ec as ec  # doctest: +SKIP
>>> qodec = ec.load_yaml("my_qodec.qodec.yaml")  # doctest: +SKIP
>>> report = ec.lint.diagnose(qodec)  # doctest: +SKIP
"""

from __future__ import annotations

from . import (
    action,
    checks,
    code,
    distance,
    equivalence,
    faults,
    lint,
    readouts,
)
from ._completion import complete_gadget, complete_qodec
from ._io import from_yaml, load_yaml, save_yaml, to_yaml
from ._synthesis import memory_program, qodec_from_code, synthesis_notes

__all__ = [
    "action",
    "checks",
    "code",
    "complete_gadget",
    "complete_qodec",
    "distance",
    "equivalence",
    "faults",
    "from_yaml",
    "lint",
    "load_yaml",
    "memory_program",
    "qodec_from_code",
    "readouts",
    "save_yaml",
    "synthesis_notes",
    "to_yaml",
]


def __dir__() -> list[str]:
    return sorted(__all__)
