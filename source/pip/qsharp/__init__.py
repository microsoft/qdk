# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from . import telemetry_events
from ._qsharp import (
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
)

telemetry_events.on_import()

from ._native import Result, Pauli, QSharpError, TargetProfile, estimate_custom

# IPython notebook specific features
try:
    if __IPYTHON__:  # type: ignore
        from ._ipython import register_magic, enable_classic_notebook_codemirror_mode

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
]
