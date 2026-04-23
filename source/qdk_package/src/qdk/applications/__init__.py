# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.applications package: re-export of qsharp.applications symbols.

Requires installation: ``pip install \"qdk[applications]\"``.

Example:
    from qdk.applications import QSharpApplication

"""

try:
    # Re-export the top-level qsharp.applications names.
    from qsharp.applications import *
except Exception as ex:
    raise ImportError(
        "qdk.applications requires the applications extras. Install with 'pip install \"qdk[applications]\"'."
    ) from ex
