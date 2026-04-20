# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import sys
from pathlib import Path

# Add local package root so 'qdk' can be imported without installation and add tests dir so 'mocks' is importable.
_root = Path(__file__).resolve().parent
_pkg_root = _root.parent
for p in (_pkg_root, _root):
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))

# Ensure a qsharp stub (if real package absent) via centralized mocks helper.
import mocks

mocks.mock_qsharp()
