# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.qre.models package: re-export of qsharp.qre.models symbols.

Requires installation: ``pip install "qdk[qre]"``.

Example:
    from qdk.qre.models import SurfaceCode

"""

try:
    # Re-export the top-level qsharp.qre.models names.
    from qsharp.qre.models import *
except Exception as ex:
    raise ImportError(
        "qdk.qre.models requires the qre extras. Install with 'pip install \"qdk[qre]\"'."
    ) from ex
