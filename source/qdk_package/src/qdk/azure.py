# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Re-export shim for the optional azure-quantum package as qdk.azure.

If azure is not installed (with the qdk[azure] extra), importing this
module raises an ImportError describing how to enable it.
"""

try:
    from azure.quantum import *  # pyright: ignore[reportWildcardImportFromLibrary]
    from azure.quantum import target as target
    from azure.quantum import argument_types as argument_types
    from azure.quantum import job as job
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
