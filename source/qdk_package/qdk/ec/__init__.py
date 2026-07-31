"""``qdk.ec`` — develop, test, and deploy quantum error correction schemes.

A *qodec* is a declarative description of a compilation pipeline together with
the quantum error correction schemes that lower each layer of that pipeline.
The ``qodec`` package defines the file format and the in-memory object model;
``qdk.ec`` is the tooling that works with those objects.

The public API is organised into four subpackages, imported lazily so that
``import qdk.ec`` stays cheap and optional dependencies (``stim``, ``mwpf``,
``deq``, ...) are only required when the subpackage that needs them is first
accessed:

* :mod:`qdk.ec.develop` — load, save, and complete qodec artifacts.
* :mod:`qdk.ec.profile` — compute actions, checks, readouts, faults, and code
  distance.
* :mod:`qdk.ec.audit` — verify that a qodec does what its author intended, with
  structured diagnostics.
* :mod:`qdk.ec.targets` — target-conditioned evaluation and execution backends
  (samplers, detector error models, resource estimation).

Installing
----------
``qdk.ec`` and its dependencies are an optional extra of the ``qdk`` package::

    pip install "qdk[ec]"

Example
-------
>>> from qdk.ec import audit, develop, profile  # doctest: +SKIP
>>> codec = develop.load("my_codec.qodec.yaml")  # doctest: +SKIP
>>> report = audit.audit(codec)  # doctest: +SKIP
"""

from __future__ import annotations

import importlib
from types import ModuleType
from typing import TYPE_CHECKING

_public_submodules = (
    "audit",
    "develop",
    "profile",
    "targets",
)

__all__ = [*_public_submodules]


def __getattr__(name: str) -> ModuleType:
    if name in _public_submodules:
        module = importlib.import_module(f"{__name__}.{name}")
        globals()[name] = module
        return module
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(__all__)


if TYPE_CHECKING:
    from . import (
        audit,
        develop,
        profile,
        targets,
    )
