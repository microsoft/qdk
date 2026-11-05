# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Presentation policy for nonfatal code-generation diagnostics."""

import sys
import warnings
from types import FrameType
from typing import List, Optional, Tuple


def report_qir_warnings(result: Tuple[str, List[Tuple[str, str]]]) -> str:
    """Report code/message pairs at the first caller outside the QDK wrappers."""
    qir, diagnostics = result
    stacklevel = 2
    frame: Optional[FrameType] = sys._getframe(1)
    while frame is not None:
        module = frame.f_globals.get("__name__", "")
        if module != "qdk" and not module.startswith("qdk."):
            break
        stacklevel += 1
        frame = frame.f_back
    del frame

    for code, message in diagnostics:
        warnings.warn(
            f"{code}: {message}" if code else message,
            UserWarning,
            stacklevel=stacklevel,
        )
    return qir
