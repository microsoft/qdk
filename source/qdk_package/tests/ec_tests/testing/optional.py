"""Skip markers for the optional backends ``qdk.ec.targets`` can drive.

``qdk[ec]`` installs the analysis and authoring tooling; the simulator and
decoder backends are a separate ``qdk[ec-backends]`` extra. Tests that need one
of them carry the matching marker so a bare ``qdk[ec]`` install still runs a
green suite.
"""

from __future__ import annotations

from importlib.util import find_spec

import pytest


def _requires(module: str) -> pytest.MarkDecorator:
    return pytest.mark.skipif(
        find_spec(module) is None,
        reason=f"{module} is not installed (pip install 'qdk[ec-backends]')",
    )


requires_mwpf = _requires("mwpf")
requires_stim = _requires("stim")

__all__ = ["requires_mwpf", "requires_stim"]
