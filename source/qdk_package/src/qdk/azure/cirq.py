# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""qdk.azure.cirq: re-export of azure.quantum.cirq symbols.

Requires installation: ``pip install \"qdk[azure,cirq]\"``.

Example:
    from qdk.azure.cirq import <symbol>

"""

try:
    from azure.quantum.cirq import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure.cirq requires the azure and cirq extras. Install with 'pip install \"qdk[azure,cirq]\"'."
    ) from ex
