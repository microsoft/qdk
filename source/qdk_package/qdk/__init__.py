# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""qdk bundling top-level package.

Provides a single import root ``qdk`` that exposes bundled quantum tooling as
submodules (``qdk.qsharp``, ``qdk.widgets``, etc.).

Optional extras install additional dependencies and submodules:

- ``azure`` — installs ``azure-quantum``, available as ``qdk.azure``.
- ``qiskit`` — installs ``qiskit`` and makes Qiskit interop functionality available as ``qdk.qiskit``.
- ``cirq`` — installs ``cirq-core`` + ``cirq-ionq`` and makes Cirq interop functionality available as ``qdk.cirq``.
- ``jupyter`` — installs ``qsharp-widgets`` + ``qsharp-jupyterlab``; exposes ``qdk.widgets``.

"""

from .telemetry_events import on_qdk_import

on_qdk_import()

# Some common utilities are lifted to the qdk root.
from . import code
from ._native import Result, TargetProfile
from ._types import (
    StateDump,
    ShotResult,
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
)
from ._interpreter import (
    set_quantum_seed,
    set_classical_seed,
    dump_machine,
    init,
)

# Register the %%qsharp cell magic when running inside IPython/Jupyter.
try:
    if __IPYTHON__:  # type: ignore
        from ._ipython import register_magic

        register_magic()
except NameError:
    pass

# utilities lifted from qsharp
__all__ = [
    "code",
    "set_quantum_seed",
    "set_classical_seed",
    "dump_machine",
    "init",
    "Result",
    "TargetProfile",
    "StateDump",
    "ShotResult",
    "PauliNoise",
    "DepolarizingNoise",
    "BitFlipNoise",
    "PhaseFlipNoise",
]
