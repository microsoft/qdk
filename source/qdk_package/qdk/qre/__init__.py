# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""Quantum Resource Estimator (QRE) for the Q# ecosystem.

This module re-exports all public symbols from [qsharp.qre](:mod:`qsharp.qre`),
making them available under the ``qdk.qre`` namespace. It provides tools for
estimating the resources required to run quantum applications on specific
hardware architectures.

Example:

    from qdk import qre
    results = qre.estimate(app, arch, isa_query)

Requires the ``qre`` extra: ``pip install qdk[qre]``.
"""

import sys

try:
    # Re-export the top-level qsharp.qre names (also loads many submodules).
    from qsharp.qre import *

    # Load additional qsharp.qre submodules not imported by qsharp.qre.__init__,
    # then register all qsharp.qre.* entries as qdk.qre.* aliases so that
    # ``from qdk.qre._architecture import ISAContext`` etc. work correctly.
    import qsharp.qre.instruction_ids  # noqa: E402
    import qsharp.qre.property_keys  # noqa: E402

    # Optional submodules (require extras such as cirq / pyqir).
    try:
        import qsharp.qre.application  # noqa: E402
    except ImportError:
        pass
    try:
        import qsharp.qre.interop  # noqa: E402
    except ImportError:
        pass
    try:
        import qsharp.qre.models  # noqa: E402
    except ImportError:
        pass

    # Register all currently-loaded qsharp.qre.* modules as qdk.qre.* aliases.
    for _key in list(sys.modules.keys()):
        if _key.startswith("qsharp.qre."):
            _qdk_key = "qdk.qre." + _key[len("qsharp.qre."):]
            sys.modules.setdefault(_qdk_key, sys.modules[_key])

    del _key, _qdk_key

except Exception as ex:
    raise ImportError(
        "qdk.qre requires the qre extra. Install with 'pip install qdk[qre]'."
    ) from ex
