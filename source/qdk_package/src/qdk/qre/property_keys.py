# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.qre.property_keys package: re-export of qsharp.qre.property_keys symbols.

Requires installation: ``pip install "qdk[qre]"``.

Example:
    from qdk.qre.property_keys import *

"""

try:
    # Re-export the top-level qsharp.qre.property_keys names.
    from qsharp.qre.property_keys import *
except Exception as ex:
    raise ImportError(
        "qdk.qre.property_keys requires the qre extras. Install with 'pip install \"qdk[qre]\"'."
    ) from ex
