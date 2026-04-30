# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from . import telemetry_events
from ._native import (
    CircuitGenerationMethod,
    Pauli,
    QSharpError,
    Result,
    TargetProfile,
    estimate_custom,
)
from ._noise import BitFlipNoise, DepolarizingNoise, PauliNoise, PhaseFlipNoise
from ._qsharp import (
    circuit,
    compile,
    dump_circuit,
    dump_machine,
    estimate,
    eval,
    init,
    logical_counts,
    run,
    set_classical_seed,
    set_quantum_seed,
)
from ._session import Session
from ._types import ShotResult, StateDump

telemetry_events.on_import()


# IPython notebook specific features
try:
    if __IPYTHON__:  # type: ignore
        from ._ipython import register_magic

        register_magic()
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
    "Session",
]
