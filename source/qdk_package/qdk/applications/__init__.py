# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""Quantum applications for the Q# ecosystem.

This module re-exports all public symbols from [qsharp.applications](:mod:`qsharp.applications`),
making them available under the ``qdk.applications`` namespace.

Requires the ``applications`` extra: ``pip install "qdk[applications]"``.

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
