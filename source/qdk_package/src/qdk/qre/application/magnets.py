# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.qre.application.magnets package: re-export of qsharp.qre.application.magnets symbols.

Requires installation: ``pip install \"qdk[qre]\"``.

Example:
    from qdk.qre.application.magnets import IsingModel

"""

try:
    # Re-export the top-level qsharp.qre.application.magnets names.
    from qsharp.qre.application.magnets import *
except Exception as ex:
    raise ImportError(
        "qdk.qre.application.magnets requires the qre extras. Install with 'pip install \"qdk[qre]\"'."
    ) from ex
