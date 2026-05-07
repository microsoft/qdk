# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Full re-export of the ``qsharp`` package as ``qdk.qsharp``.

This module makes the entire ``qsharp`` public API available under the
``qdk.qsharp`` namespace, so code that imports from ``qdk.qsharp`` behaves
identically to importing from ``qsharp`` directly.

Key exports:

- :func:`~qsharp.init`, :func:`~qsharp.eval`, :func:`~qsharp.run` — initialize and execute Q# code.
- :class:`~qsharp.StateDump`, :class:`~qsharp.TargetProfile` — state inspection and compilation target.
- :class:`~qsharp.PauliNoise`, :class:`~qsharp.DepolarizingNoise`, :class:`~qsharp.BitFlipNoise`, :class:`~qsharp.PhaseFlipNoise` — noise models.
- :func:`~qdk.qsharp.dump_operation` — compute the unitary matrix of a Q# operation.

For full API documentation see [qsharp](:mod:`qsharp`).
"""

from ._types import *  # pyright: ignore[reportWildcardImportFromLibrary]
from ._interpreter import *  # pyright: ignore[reportWildcardImportFromLibrary]
