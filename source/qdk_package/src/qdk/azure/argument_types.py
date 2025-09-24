# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Submodule re-export for azure.quantum.argument_types as qdk.azure.argument_types."""

try:
    from azure.quantum.argument_types import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
