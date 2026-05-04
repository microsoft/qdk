# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Argument types for Azure Quantum job inputs.

This module re-exports all public symbols from
[azure.quantum.argument_types](:mod:`azure.quantum.argument_types`),
making them available under the ``qdk.azure.argument_types`` namespace.

Key exports:

- :class:`azure.quantum.argument_types.EmptyArray` — represents an empty typed array argument.
- :class:`azure.quantum.argument_types.Pauli` — Pauli operator argument type (I, X, Y, Z).
- :class:`azure.quantum.argument_types.Range` — integer range argument.
- :class:`azure.quantum.argument_types.Result` — qubit measurement result argument (Zero or One).

Requires the ``azure`` extra: ``pip install qdk[azure]``.
"""

try:
    from azure.quantum.argument_types import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
