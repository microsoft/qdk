# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""qdk.azure.qiskit: re-export of azure.quantum.qiskit symbols.

Requires installation: ``pip install \"qdk[azure,qiskit]\"``.

Example:
    from qdk.azure.qiskit import <symbol>

"""

try:
    from azure.quantum.qiskit import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure.qiskit requires the azure and qiskit extras. Install with 'pip install \"qdk[azure,qiskit]\"'."
    ) from ex
