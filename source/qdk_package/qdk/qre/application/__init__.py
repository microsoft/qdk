# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Any

from ._qir import QIRApplication
from ._qsharp import QSharpApplication
from ._openqasm import OpenQASMApplication

try:
    from ._cirq import CirqApplication, CirqApplicationParams
except ImportError:

    class _CirqNotInstalled:
        """Placeholder that raises a helpful error when cirq is not installed."""

        def __init__(self, *args: Any, **kwargs: Any):
            raise ImportError(
                "CirqApplication requires the 'cirq' extra. "
                "Install it with: pip install qdk[qre,cirq]"
            )

    CirqApplication = _CirqNotInstalled
    CirqApplicationParams = _CirqNotInstalled

__all__ = [
    "CirqApplication",
    "CirqApplicationParams",
    "OpenQASMApplication",
    "QIRApplication",
    "QSharpApplication",
]
