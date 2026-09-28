"""Vendored qodec fixtures for qdk.ec tests.

A self-contained, single-file qodec snapshot, kept here so tests have a
concrete qodec to sample, decode, and analyze without a bespoke qodec generator
in the qdk.ec package itself. ``c4`` is a saved snapshot of the retired
``qdk.ec.qodecs.c4()`` output. Regenerate with
``qodec.save(path, single_file=True)``.
"""

from __future__ import annotations

from pathlib import Path

import qodec as qc

_fixtures_dir = Path(__file__).parent


def _load(name: str) -> qc.Qodec:
    return qc.Qodec.load(str(_fixtures_dir / f"{name}.qodec.yaml"))


def c4() -> qc.Qodec:
    """The C4 [[4,2,2]] error-detecting qodec (two logical qubits)."""
    return _load("c4")
