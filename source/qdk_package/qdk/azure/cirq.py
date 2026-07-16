# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Cirq integration for Azure Quantum.

This module re-exports all public symbols from [azure.quantum.cirq](:mod:`azure.quantum.cirq`),
making them available under the ``qdk.azure.cirq`` namespace.

Key exports:

- :class:`azure.quantum.cirq.AzureQuantumService` — a ``cirq.Sampler``-compatible
  service that submits Cirq circuits to Azure Quantum targets.
- :class:`azure.quantum.cirq.Job` — represents an Azure Quantum job submitted via Cirq.

Requires the ``azure`` and ``cirq`` extras: ``pip install "qdk[azure,cirq]"``.
"""

try:
    from azure.quantum.cirq import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure.cirq requires the azure and cirq extras. Install with 'pip install \"qdk[azure,cirq]\"'."
    ) from ex
