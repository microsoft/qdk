# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.qre.instruction_ids package: re-export of qsharp.qre.instruction_ids symbols.

Requires installation: ``pip install "qdk[qre]"``.

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
