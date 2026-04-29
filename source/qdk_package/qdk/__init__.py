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

import sys

from qsharp.telemetry_events import on_qdk_import

on_qdk_import()

# Re-export all public symbols from qsharp at the qdk root.
from qsharp import code, utils
from qsharp import (
    set_quantum_seed,
    set_classical_seed,
    dump_machine,
    dump_circuit,
    init,
    eval,
    run,
    compile,
    circuit,
    estimate,
    estimate_custom,
    logical_counts,
    Result,
    Pauli,
    QSharpError,
    TargetProfile,
    StateDump,
    ShotResult,
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
    CircuitGenerationMethod,
)

# Register qsharp private submodules as qdk.* aliases so that
# ``import qdk._native``, ``import qdk._fs``, etc. work correctly and refer to
# the same underlying module objects (important for tests that patch module
# attributes, e.g. qdk._fs.read_file = ...).
import qsharp._native  # noqa: E402
import qsharp._qsharp  # noqa: E402
import qsharp._fs  # noqa: E402
import qsharp._http  # noqa: E402
import qsharp._simulation  # noqa: E402
import qsharp._adaptive_pass  # noqa: E402
import qsharp._adaptive_bytecode  # noqa: E402
import qsharp.noisy_simulator  # noqa: E402
import qsharp._device  # noqa: E402
import qsharp._device._atom  # noqa: E402
import qsharp._device._atom._decomp  # noqa: E402
import qsharp._device._atom._validate  # noqa: E402

_qsharp_private_submodules = [
    "_native",
    "_qsharp",
    "_fs",
    "_http",
    "_simulation",
    "_adaptive_pass",
    "_adaptive_bytecode",
    "noisy_simulator",
    "code",
    "utils",
    "_device",
    "_device._atom",
    "_device._atom._decomp",
    "_device._atom._validate",
]


def _register_qsharp_aliases() -> None:
    for _submod in _qsharp_private_submodules:
        _qdk_key = f"qdk.{_submod}"
        _qsharp_key = f"qsharp.{_submod}"
        if _qdk_key not in sys.modules and _qsharp_key in sys.modules:
            sys.modules[_qdk_key] = sys.modules[_qsharp_key]


_register_qsharp_aliases()
del _qsharp_private_submodules, _register_qsharp_aliases

__all__ = [
    "code",
    "utils",
    "set_quantum_seed",
    "set_classical_seed",
    "dump_machine",
    "dump_circuit",
    "init",
    "eval",
    "run",
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
