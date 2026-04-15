# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Submodule re-export for azure.quantum.job as qdk.azure.job."""

try:
    from azure.quantum.job import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
