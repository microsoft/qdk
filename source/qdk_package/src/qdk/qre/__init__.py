# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""qdk.qre package: re-export of qsharp.qre namespaces.

Requires optional extra installation: `pip install qdk[qre]`.

Usage examples:
    from qdk import qre
    results = qre.estimate(app, arch, isa_query)

"""

try:
    # Re-export the top-level qsharp.qre names.
    from qsharp.qre import *
except Exception as ex:
    raise ImportError(
        "qdk.qre requires the qre extra. Install with 'pip install qdk[qre]'."
    ) from ex
