# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.applications.magnets package: re-export of qsharp.applications.magnets symbols.

Requires installation: ``pip install \"qdk[applications]\"``.

Example:
    from qdk.applications.magnets import IsingModel

"""

try:
    # Re-export the top-level qsharp.applications.magnets names.
    from qsharp.applications.magnets import *
except Exception as ex:
    raise ImportError(
        "qdk.applications.magnets requires the applications extras. Install with 'pip install \"qdk[applications]\"'."
    ) from ex
