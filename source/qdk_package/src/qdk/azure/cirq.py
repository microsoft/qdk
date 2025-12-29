# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""qdk.azure.cirq: re-export of azure.quantum.cirq symbols.

Requires installation: ``pip install \"qdk[azure]\"``.

Example:
    from qdk.azure.cirq import <symbol>

"""

try:
    from azure.quantum.cirq import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure.cirq requires the azure extra (and cirq). Install with 'pip install \"qdk[azure]\"'."
    ) from ex
