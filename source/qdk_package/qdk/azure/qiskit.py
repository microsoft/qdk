# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Qiskit integration for Azure Quantum.

This module re-exports all public symbols from [azure.quantum.qiskit](:mod:`azure.quantum.qiskit`),
making them available under the ``qdk.azure.qiskit`` namespace.

Key exports:

- :class:`azure.quantum.qiskit.AzureQuantumProvider` — a Qiskit ``Provider`` that
  exposes Azure Quantum targets as ``Backend`` instances.
- :class:`azure.quantum.qiskit.AzureQuantumJob` — a Qiskit ``Job`` representing a
  circuit submitted to Azure Quantum.

Requires the ``azure`` and ``qiskit`` extras: ``pip install "qdk[azure,qiskit]"``.
"""

try:
    from azure.quantum.qiskit import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure.qiskit requires the azure and qiskit extras. Install with 'pip install \"qdk[azure,qiskit]\"'."
    ) from ex
