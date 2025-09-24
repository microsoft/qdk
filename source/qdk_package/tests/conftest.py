# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import sys
from pathlib import Path

# Add local 'src' so 'qdk' can be imported without installation and add tests dir so 'mocks' is importable.
_root = Path(__file__).resolve().parent
_src = _root.parent / "src"
for p in (_src, _root):
    if str(p) not in sys.path:
        sys.path.insert(0, str(p))

# Ensure a qsharp stub (if real package absent) via centralized mocks helper.
import mocks

mocks.mock_qsharp()
