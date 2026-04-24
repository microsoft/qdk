# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""Magnetic system applications for the Q# ecosystem.

This module re-exports all public symbols from [qsharp.applications.magnets](:mod:`qsharp.applications.magnets`),
making them available under the ``qdk.applications.magnets`` namespace. It
provides classes for modeling and simulating magnetic systems such as the Ising
model using quantum algorithms.

Requires the ``applications`` extra: ``pip install "qdk[applications]"``.

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
