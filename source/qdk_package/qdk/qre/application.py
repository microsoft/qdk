# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""QRE application definitions.

This module re-exports all public symbols from [qsharp.qre.application](:mod:`qsharp.qre.application`),
making them available under the ``qdk.qre.application`` namespace. It provides
classes for defining quantum applications to be passed to the resource estimator.

Requires the ``qre`` extra: ``pip install qdk[qre]``.

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
