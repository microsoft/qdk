# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""qdk.azure package: re-export of azure.quantum namespaces.

Requires optional extra installation: `pip install qdk[azure]`.

Usage examples:
    from qdk import azure
    ws = azure.Workspace(...)  # if upstream exposes Workspace at top-level

"""

try:
    # Re-export the top-level azure.quantum names.
    from azure.quantum import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
