# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Rigetti target support for Azure Quantum.

This module re-exports all public symbols from
[azure.quantum.target.rigetti](:mod:`azure.quantum.target.rigetti`),
making them available under the ``qdk.azure.target.rigetti`` namespace.

Key exports:

- :class:`azure.quantum.target.rigetti.Rigetti` — high-level target class for Rigetti QPU and simulator backends.
- :class:`azure.quantum.target.rigetti.RigettiTarget` — enumeration of available Rigetti target names.
- :class:`azure.quantum.target.rigetti.InputParams` — job submission parameters for Rigetti targets.
- :class:`azure.quantum.target.rigetti.Result` — result type for Rigetti job outputs.

Requires the ``azure`` extra: ``pip install qdk[azure]``.
"""

try:
    from azure.quantum.target.rigetti import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
