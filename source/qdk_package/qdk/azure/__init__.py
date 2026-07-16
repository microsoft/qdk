# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Azure Quantum integration for the Q# ecosystem.

This module re-exports all public symbols from [azure.quantum](:mod:`azure.quantum`),
making them available under the ``qdk.azure`` namespace. The primary entry point is
:class:`azure.quantum.Workspace`, which represents a connection to an Azure Quantum
workspace.

Key exports:

- :class:`azure.quantum.Workspace` — connect to an Azure Quantum workspace and submit jobs.
- :class:`azure.quantum.job.Job`, :class:`azure.quantum.job.Session` — job and session management.
- :class:`azure.quantum.job.JobDetails`, ``JobStatus``, :class:`azure.quantum.job.SessionDetails`, :class:`azure.quantum.job.SessionStatus` — status and metadata types.
- ``ItemType``, :class:`azure.quantum.job.SessionHost`, :class:`azure.quantum.job.SessionJobFailurePolicy` — configuration enumerations.

Usage:

    from qdk import azure
    ws = azure.Workspace(...)  # if upstream exposes Workspace at top-level

Requires the ``azure`` extra: ``pip install qdk[azure]``.
"""

try:
    # Re-export the top-level azure.quantum names.
    from azure.quantum import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.azure requires the azure extra. Install with 'pip install qdk[azure]'."
    ) from ex
