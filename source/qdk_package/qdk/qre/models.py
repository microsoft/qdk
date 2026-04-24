# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""QRE hardware and QEC models.

This module re-exports all public symbols from [qsharp.qre.models](:mod:`qsharp.qre.models`),
making them available under the ``qdk.qre.models`` namespace. It provides
classes representing hardware architectures, qubit models, and quantum error
correction schemes used in resource estimation.

Requires the ``qre`` extra: ``pip install qdk[qre]``.

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
