"""Skip markers for dependencies that may be absent in source environments.

Published ``qdk[ec]`` installs MWPF, but source checkouts do not necessarily
have the package installed. Tests that need it carry this marker.
"""

from __future__ import annotations

from importlib.util import find_spec

import pytest


def _requires(module: str) -> pytest.MarkDecorator:
    return pytest.mark.skipif(
        find_spec(module) is None,
        reason=f"{module} is not installed (pip install 'qdk[ec]')",
    )


requires_mwpf = _requires("mwpf")

__all__ = ["requires_mwpf"]
