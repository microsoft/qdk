# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Centralized mock helpers for tests.

Provides lightweight stand-ins for optional dependencies.

Functions return a list of module names they created so callers can later clean them
up using cleanup_modules(). This keeps test intent explicit.
"""

import sys
import types
from typing import List


def _not_impl(*_a, **_k):
    raise NotImplementedError("stub: dependency not installed")


def mock_widgets() -> List[str]:
    created: List[str] = []
    if "qsharp_widgets" not in sys.modules:
        mod = types.ModuleType("qsharp_widgets")
        sys.modules["qsharp_widgets"] = mod
        created.append("qsharp_widgets")
    return created


def mock_azure() -> List[str]:
    created: List[str] = []
    if "azure" not in sys.modules:
        sys.modules["azure"] = types.ModuleType("azure")
        created.append("azure")
    if "azure.quantum" not in sys.modules:
        aq = types.ModuleType("azure.quantum")
        # Minimal submodules expected by qdk.azure shim
        tgt = types.ModuleType("azure.quantum.target")
        argt = types.ModuleType("azure.quantum.argument_types")
        job = types.ModuleType("azure.quantum.job")
        # Register in sys.modules first
        sys.modules["azure.quantum.target"] = tgt
        sys.modules["azure.quantum.argument_types"] = argt
        sys.modules["azure.quantum.job"] = job
        # Attach to parent for attribute access
        aq.target = tgt
        aq.argument_types = argt
        aq.job = job
        sys.modules["azure.quantum"] = aq
        created.extend(
            [
                "azure.quantum",
                "azure.quantum.target",
                "azure.quantum.argument_types",
                "azure.quantum.job",
            ]
        )
    return created


def cleanup_modules(created: List[str]) -> None:
    """Remove synthetic modules created during a test if still present."""
    for name in created:
        sys.modules.pop(name, None)
