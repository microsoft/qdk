# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.qre.application package: re-export of qsharp.qre.application symbols.

Requires installation: ``pip install \"qdk[qre]\"``.

Example:
    from qdk.qre.application import QSharpApplication

"""

try:
    # Re-export the top-level qsharp.qre.application names.
    from qsharp.qre.application import *
except Exception as ex:
    raise ImportError(
        "qdk.qre.application requires the qre extras. Install with 'pip install \"qdk[qre]\"'."
    ) from ex
