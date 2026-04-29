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
    "See https://github.com/microsoft/qdk for migration guidance.",
    DeprecationWarning,
    stacklevel=2,
)

# Re-export the full public API from qdk so that existing code keeps working.
from qdk._qsharp import (
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
    StateDump,
    ShotResult,
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
    CircuitGenerationMethod,
)

from qdk._native import Result, Pauli, QSharpError, TargetProfile, estimate_custom

from qdk import telemetry_events

telemetry_events.on_import()

# IPython notebook specific features
try:
    if __IPYTHON__:  # type: ignore
        from qdk._ipython import register_magic, enable_classic_notebook_codemirror_mode

        register_magic()
        enable_classic_notebook_codemirror_mode()
except NameError:
    pass


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
