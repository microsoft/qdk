# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Microsoft Quantum Development Kit (QDK) for Python.

The ``qdk`` package is the unified Python entry point for the Microsoft Quantum
Development Kit. It bundles the Q# interpreter, OpenQASM tooling, simulators,
the Resource Estimator, and interoperability utilities under a single import
root.

Core submodules (always available):

- :mod:`qdk.qsharp` — Q# interpreter and core operations
  (:func:`~qdk.qsharp.init`, :func:`~qdk.qsharp.eval`, :func:`~qdk.qsharp.run`,
  :func:`~qdk.qsharp.compile`, :func:`~qdk.qsharp.circuit`, etc.).
- :mod:`qdk.openqasm` — compile, run, and estimate OpenQASM programs.
- :mod:`qdk.simulation` — noise-aware quantum simulators and the
  ``NeutralAtomDevice``.
- :mod:`qdk.estimator` — the legacy Microsoft Resource Estimator API.
- :mod:`qdk.qre` — the next-generation Resource Estimator (QRE v3).
- :mod:`qdk.code` — namespace populated with user-defined Q# and OpenQASM callables.

Frequently used utilities are also re-exported at the package root for
convenience: :func:`~qdk.qsharp.init`, :func:`~qdk.qsharp.dump_machine`,
:func:`~qdk.qsharp.set_quantum_seed`, :func:`~qdk.qsharp.set_classical_seed`,
:class:`~qdk.qsharp.Result`, :class:`~qdk.qsharp.TargetProfile`,
:class:`~qdk.qsharp.StateDump`, :class:`~qdk.qsharp.ShotResult`,
:class:`~qdk.qsharp.PauliNoise`, :class:`~qdk.qsharp.DepolarizingNoise`,
:class:`~qdk.qsharp.BitFlipNoise`, :class:`~qdk.qsharp.PhaseFlipNoise`,
and :class:`~qdk.Context`.

Optional extras enable additional submodules:

- ``qdk[azure]`` — Azure Quantum job submission and management
  (:mod:`qdk.azure`).
- ``qdk[qiskit]`` — Qiskit interoperability (:mod:`qdk.qiskit`).
- ``qdk[cirq]`` — Cirq interoperability (:mod:`qdk.cirq`).
- ``qdk[jupyter]`` — interactive Jupyter widgets and JupyterLab integration
  (:mod:`qdk.widgets`).
"""

from .telemetry_events import on_qdk_import

on_qdk_import()

# Some common utilities are lifted to the qdk root.
from . import code
from ._context import Context
from ._interpreter import (
    dump_machine,
    init,
    set_classical_seed,
    set_quantum_seed,
)
from ._native import Result, TargetProfile
from ._types import (
    BitFlipNoise,
    DepolarizingNoise,
    PauliNoise,
    PhaseFlipNoise,
    ShotResult,
    StateDump,
)

# Register the %%qsharp cell magic when running inside IPython/Jupyter.
try:
    if __IPYTHON__:  # type: ignore
        from ._ipython import register_magic

        register_magic()
except NameError:
    pass

# Public API exposed at the top of the qdk package.
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
    "Context",
]
