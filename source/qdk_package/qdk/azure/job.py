# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Job and session management for Azure Quantum.

This module re-exports all public symbols from [azure.quantum.job](:mod:`azure.quantum.job`),
making them available under the ``qdk.azure.job`` namespace.

Key exports:

- :class:`azure.quantum.job.Job` — an Azure Quantum job, with methods to wait for and retrieve results.
- :class:`azure.quantum.job.Session`, :class:`azure.quantum.job.SessionDetails`, :class:`azure.quantum.job.SessionHost` — grouped job session management.
- :class:`azure.quantum.job.WorkspaceItem` — lower-level workspace item type.

Requires the ``azure`` extra: ``pip install qdk[azure]``.
"""

try:
    from azure.quantum.job import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
