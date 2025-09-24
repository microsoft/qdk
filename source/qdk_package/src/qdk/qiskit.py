# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Interop-only shim for qdk.qiskit.

This module re-exports the QDK Qiskit interop surface from ``qsharp.interop.qiskit``
without importing the external ``qiskit`` package. Users should import upstream
Qiskit APIs directly from ``qiskit``.
"""

try:
    from qsharp.interop.qiskit import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.qiskit requires the qiskit extra. Install with 'pip install qdk[qiskit]'."
    ) from ex
