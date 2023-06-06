# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import subprocess
import sys
import re

python_bin = sys.executable


def test_extension_is_installed():
    result = subprocess.run(
        [python_bin, "-m", "jupyter", "labextension", "list"],
        capture_output=True,
        check=True,
        text=True,
    )

    assert re.search(r"qsharp_jupyterlab.*ok", result.stderr, re.IGNORECASE)
