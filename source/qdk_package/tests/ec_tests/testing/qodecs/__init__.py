"""Vendored qodec fixtures for qdk.ec tests.

A self-contained, single-file codec snapshot, kept here so tests have a
concrete codec to sample, decode, and analyze without a bespoke codec generator
in the qdk.ec package itself. ``c4`` is a saved snapshot of the retired
``qdk.ec.codecs.c4()`` output. Regenerate with
``codec.save(path, single_file=True)``.
"""
from __future__ import annotations

from pathlib import Path

import qodec

_fixtures_dir = Path(__file__).parent


def _load(name: str) -> qodec.Qodec:
    return qodec.Qodec.load(str(_fixtures_dir / f"{name}.qodec.yaml"))


def c4() -> qodec.Qodec:
    """The C4 [[4,2,2]] error-detecting codec (two logical qubits)."""
    return _load("c4")
