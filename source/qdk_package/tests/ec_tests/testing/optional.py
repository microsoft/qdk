"""Skip markers for dependencies that may be absent in source environments.

Published ``qdk[ec]`` installs HiGHS. MWPF requires a separate installation.
Stim comes with ``qodec[parsers]``, which ``qdk[ec]`` omits on platforms without
Stim wheels (Linux aarch64, Windows ARM64); tests that parse ``format="stim"``
circuits, directly or through ``build_qodec``, need it. Tests for these
dependencies carry a marker when the package may be absent.
"""

from __future__ import annotations

from importlib.util import find_spec

import pytest


def _requires(module: str, package: str) -> pytest.MarkDecorator:
    return pytest.mark.skipif(
        find_spec(module) is None,
        reason=f"{module} is not installed (pip install '{package}')",
    )


requires_mwpf = _requires("mwpf", "mwpf")
requires_highs = _requires("highspy", "qdk[ec]")
requires_stim = _requires("stim", "stim")

__all__ = ["requires_mwpf", "requires_highs", "requires_stim"]
