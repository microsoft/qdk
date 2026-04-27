# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""QRE instruction identifiers.

This module re-exports all public symbols from [qsharp.qre.instruction_ids](:mod:`qsharp.qre.instruction_ids`),
making them available under the ``qdk.qre.instruction_ids`` namespace. It provides
constants identifying the quantum instruction set operations used in resource
estimation traces.

Requires the ``qre`` extra: ``pip install qdk[qre]``.

Example:

    from qdk.qre.instruction_ids import *
"""

try:
    # Re-export the top-level qsharp.qre.instruction_ids names.
    from qsharp.qre.instruction_ids import *
except Exception as ex:
    raise ImportError(
        "qdk.qre.instruction_ids requires the qre extras. Install with 'pip install \"qdk[qre]\"'."
    ) from ex
