# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""QRE interoperability utilities.

This module re-exports all public symbols from [qsharp.qre.interop](:mod:`qsharp.qre.interop`),
making them available under the ``qdk.qre.interop`` namespace. It provides
functions for generating resource estimation traces from Q#, Cirq, QIR, and
OpenQASM programs.

Requires the ``qre`` extra: ``pip install qdk[qre]``.

Example:

    from qdk.qre.interop import trace_from_qir
"""

try:
    # Re-export the top-level qsharp.qre.interop names.
    from qsharp.qre.interop import *
except Exception as ex:
    raise ImportError(
        "qdk.qre.interop requires the qre extras. Install with 'pip install \"qdk[qre]\"'."
    ) from ex
