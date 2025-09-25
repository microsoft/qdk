# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Submodule re-export for azure.quantum.target as qdk.azure.target."""

try:
    from azure.quantum.target import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
