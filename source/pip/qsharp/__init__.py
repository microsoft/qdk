# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Deprecated: The ``qsharp`` package has been replaced by ``qdk``.

All functionality previously available in ``qsharp`` is now provided by the
``qdk`` package. This package is a thin compatibility shim that re-exports the
``qdk`` public API so that existing code continues to work.

To migrate, replace ``import qsharp`` with ``from qdk import qsharp`` or
``import qdk`` and use the ``qdk.*`` namespace directly.
"""

import warnings as _warnings

_warnings.warn(
    "The 'qsharp' package is deprecated and will be removed in a future release. "
    "Please use the 'qdk' package instead. "
    "See https://github.com/microsoft/qdk/wiki/Migrating-from-qsharp-to-qdk for migration guidance.",
    DeprecationWarning,
    stacklevel=2,
)

# Re-export the public API from qdk.qsharp so that existing code keeps working.
from qdk.qsharp import (  # noqa: F401
    StateDump,
    ShotResult,
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
    init,
    eval,
    run,
    compile,
    circuit,
    estimate,
    logical_counts,
    set_quantum_seed,
    set_classical_seed,
    dump_machine,
    dump_circuit,
    Result,
    Pauli,
    QSharpError,
    TargetProfile,
    estimate_custom,
    CircuitGenerationMethod,
)

from qdk import telemetry_events

telemetry_events.on_import()

__all__ = [
    "init",
    "eval",
    "run",
    "set_quantum_seed",
    "set_classical_seed",
    "dump_machine",
    "dump_circuit",
    "compile",
    "circuit",
    "estimate",
    "estimate_custom",
    "logical_counts",
    "Result",
    "Pauli",
    "QSharpError",
    "TargetProfile",
    "StateDump",
    "ShotResult",
    "PauliNoise",
    "DepolarizingNoise",
    "BitFlipNoise",
    "PhaseFlipNoise",
    "CircuitGenerationMethod",
]
