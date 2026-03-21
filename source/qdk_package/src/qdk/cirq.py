# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Interop-only shim for qdk.cirq.

This module re-exports the QDK Cirq interop surface from ``qsharp.interop.cirq``
without importing the external ``cirq`` package. Users should import upstream
Cirq APIs directly from ``cirq``.
"""

try:
    from qsharp.interop.cirq import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.cirq requires the cirq extra. Install with 'pip install qdk[cirq]'."
    ) from ex
