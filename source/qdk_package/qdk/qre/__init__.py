# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""Quantum Resource Estimator (QRE) for the Q# ecosystem.

This module re-exports all public symbols from [qsharp.qre](:mod:`qsharp.qre`),
making them available under the ``qdk.qre`` namespace. It provides tools for
estimating the resources required to run quantum applications on specific
hardware architectures.

Example:

    from qdk import qre
    results = qre.estimate(app, arch, isa_query)

Requires the ``qre`` extra: ``pip install qdk[qre]``.
"""

try:
    # Re-export the top-level qsharp.qre names.
    from qsharp.qre import *
except Exception as ex:
    raise ImportError(
        "qdk.qre requires the qre extra. Install with 'pip install qdk[qre]'."
    ) from ex
