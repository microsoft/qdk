# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Q# interpreter public API.

This module is the public surface for Q# interpreter functionality
within the ``qdk`` package.

Key exports:

- :func:`~qdk.qsharp.init` — initialize or reset the Q# interpreter.
- :func:`~qdk.qsharp.eval` — evaluate a Q# expression and return its value.
- :func:`~qdk.qsharp.run` — run a Q# entry expression for one or more shots.
- :func:`~qdk.qsharp.compile` — compile Q# source to QIR for hardware submission.
- :func:`~qdk.qsharp.circuit` — synthesize a circuit diagram from Q# code.
- :func:`~qdk.qsharp.estimate` — estimate quantum resources (deprecated; use
  :mod:`qdk.qre` instead).
- :func:`~qdk.qsharp.logical_counts` — extract logical gate counts from Q# code.
- :func:`~qdk.qsharp.dump_machine` — return the current simulator state as a
  :class:`~qdk.qsharp.StateDump`.
- :func:`~qdk.qsharp.dump_circuit` — return the traced circuit (requires
  ``trace_circuit=True`` in :func:`~qdk.qsharp.init`).
- :func:`~qdk.qsharp.dump_operation` — compute the unitary matrix of a Q# operation.
- :func:`~qdk.qsharp.set_quantum_seed`, :func:`~qdk.qsharp.set_classical_seed` — control RNG seeds.
- :func:`~qdk.qsharp.estimate_custom` — run the generic resource estimator with
  user-supplied algorithm, qubit, and code parameters.
- :class:`~qdk.qsharp.QSharpError` — raised on Q# compilation or runtime errors.
- :class:`~qdk.qsharp.TargetProfile` — compilation target profile enum.
- :class:`~qdk.qsharp.Result`, :class:`~qdk.qsharp.Pauli` — Q# primitive types.
- :class:`~qdk.qsharp.CircuitGenerationMethod` — controls how circuits are synthesized.
- :class:`~qdk.qsharp.Circuit` — synthesized circuit representation.
- :class:`~qdk.qsharp.QirInputData` — compiled QIR wrapper for hardware submission.
- :class:`~qdk.qsharp.Config` — interpreter configuration returned by :func:`~qdk.qsharp.init`.
- :class:`~qdk.qsharp.StateDump`, :class:`~qdk.qsharp.ShotResult` — interpreter output types.
- :class:`~qdk.qsharp.PauliNoise`, :class:`~qdk.qsharp.DepolarizingNoise`, :class:`~qdk.qsharp.BitFlipNoise`,
  :class:`~qdk.qsharp.PhaseFlipNoise` — noise models for simulation.
"""

from ._types import (
    StateDump,
    ShotResult,
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
    QirInputData,
    Config,
)
from ._interpreter import (
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
    dump_operation,
)
from ._native import (  # type: ignore
    Circuit,
    Result,
    Pauli,
    QSharpError,
    TargetProfile,
    estimate_custom,
    CircuitGenerationMethod,
)

__all__ = [
    # Core operations
    "init",
    "eval",
    "run",
    "compile",
    "circuit",
    "estimate",
    "logical_counts",
    # Seed / state
    "set_quantum_seed",
    "set_classical_seed",
    "dump_machine",
    "dump_circuit",
    "dump_operation",
    # Native types
    "Circuit",
    "Result",
    "Pauli",
    "QSharpError",
    "TargetProfile",
    "estimate_custom",
    "CircuitGenerationMethod",
    # Python types
    "StateDump",
    "ShotResult",
    "PauliNoise",
    "DepolarizingNoise",
    "BitFlipNoise",
    "PhaseFlipNoise",
    "QirInputData",
    "Config",
]
