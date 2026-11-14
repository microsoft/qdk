# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Presentation policy for nonfatal code-generation diagnostics."""

import inspect
import warnings
from typing import List, Tuple


def report_qir_warnings(result: Tuple[str, List[Tuple[str, str]]]) -> str:
    """Report warnings outside QDK wrappers, or at the direct caller if frames are unavailable."""
    qir, diagnostics = result
    stacklevel = 2
    # Once the minimum supported Python is 3.12, replace this walk with
    # warnings.warn(..., skip_file_prefixes=(qdk_package_directory,)).
    # That API skips package frames without manually inspecting the stack.
    frame = inspect.currentframe()
    try:
        if frame is not None:
            frame = frame.f_back
        while frame is not None:
            module = frame.f_globals.get("__name__", "")
            if module != "qdk" and not module.startswith("qdk."):
                break
            stacklevel += 1
            frame = frame.f_back
    finally:
        del frame

    for code, message in diagnostics:
        warnings.warn(
            f"{code}: {message}" if code else message,
            UserWarning,
            stacklevel=stacklevel,
        )
    return qir
