# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import sys
from pathlib import Path

# Add local package root so 'qdk' can be imported without installation (useful
# after ``maturin develop``) and add tests dir so 'mocks' is importable.
# In CI the wheel is installed before tests run, so this is a convenience fallback.
_root = Path(__file__).resolve().parent
_pkg_root = _root.parent
for p in (_pkg_root, _root):
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))
