# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Full re-export of the ``qsharp`` package as ``qdk.qsharp``.

This module makes the entire ``qsharp`` public API available under the
``qdk.qsharp`` namespace, so code that imports from ``qdk.qsharp`` behaves
identically to importing from ``qsharp`` directly. It also pulls in
``dump_operation`` from ``qsharp.utils``.

Key exports include ``init``, ``run``, ``eval``, ``compile``, ``circuit``,
``estimate``, ``dump_machine``, ``dump_circuit``, ``StateDump``,
``TargetProfile``, and the noise classes ``PauliNoise``, ``DepolarizingNoise``,
``BitFlipNoise``, and ``PhaseFlipNoise``.
"""

from qsharp import *  # pyright: ignore[reportWildcardImportFromLibrary]
from qsharp.utils import dump_operation  # pyright: ignore[reportUnusedImport]
