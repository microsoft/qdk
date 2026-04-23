# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.qre.interop package: re-export of qsharp.qre.interop symbols.

Requires installation: ``pip install "qdk[qre]"``.

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
