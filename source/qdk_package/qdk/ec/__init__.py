"""``qdk.ec`` — develop, test, and deploy quantum error correction schemes.

A *qodec* is a declarative description of a compilation pipeline together with
the quantum error correction schemes that lower each layer of that pipeline.
The ``qodec`` package defines the file format and the in-memory object model;
``qdk.ec`` is the tooling that works with those objects.

The API is organised around what you are trying to do.

Develop
-------
Move qodecs between disk, memory, and YAML text, and let automated analysis
finish the parts a human should not have to write.

* :func:`load`, :func:`save`, :func:`from_yaml`, :func:`to_yaml` — primitives.
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

Deploy
------
* :mod:`~qdk.ec.targets` — target-conditioned evaluation and execution backends:
  samplers, detector error models, circuit-level distance, and running an
  ordinary QIR program under a qodec.

Installing
----------
``qdk.ec`` and its dependencies are an optional extra of the ``qdk`` package::

    pip install "qdk[ec]"              # authoring and analysis
    pip install "qdk[ec,ec-backends]"  # ... plus the stim / mwpf backends

Example
-------
>>> import qdk.ec as ec  # doctest: +SKIP
>>> codec = ec.load("my_codec.qodec.yaml")  # doctest: +SKIP
>>> report = ec.lint.diagnose(codec)  # doctest: +SKIP
"""

from __future__ import annotations

import importlib
from typing import TYPE_CHECKING, Any

from ._completion import complete_gadget, complete_qodec
from ._primitives import from_yaml, load, save, to_yaml
from ._synthesis import memory_program, qodec_from_code, synthesis_notes

#: Submodules resolved on first attribute access, so ``import qdk.ec`` stays
#: cheap and optional backends (stim, mwpf, deq) are only required by the
#: module that actually needs them.
_LAZY_SUBMODULES = (
    "action",
    "checks",
    "code",
    "distance",
    "equivalence",
    "faults",
    "lint",
    "readouts",
    "targets",
)

__all__ = [
    *_LAZY_SUBMODULES,
    "complete_gadget",
    "complete_qodec",
    "from_yaml",
    "load",
    "memory_program",
    "qodec_from_code",
    "save",
    "synthesis_notes",
    "to_yaml",
]


def __getattr__(name: str) -> Any:
    if name in _LAZY_SUBMODULES:
        module = importlib.import_module(f"{__name__}.{name}")
        globals()[name] = module
        return module
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(__all__)


if TYPE_CHECKING:
    from . import (
        action,
        checks,
        code,
        distance,
        equivalence,
        faults,
        lint,
        readouts,
        targets,
    )
