# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Q# interpreter public API.

This module is the public surface for Q# interpreter functionality
within the ``qdk`` package.


Key exports:

- :func:`init`, :func:`eval`, :func:`run` — initialize and execute Q# code.
- :class:`StateDump`, :class:`TargetProfile` — state inspection and compilation target.
- :class:`PauliNoise`, :class:`DepolarizingNoise`, :class:`BitFlipNoise`, :class:`PhaseFlipNoise` — noise models.
- :func:`dump_operation` — compute the unitary matrix of a Q# operation.
"""

from ._types import (
    StateDump,
    ShotResult,
    PauliNoise,
    DepolarizingNoise,
    BitFlipNoise,
    PhaseFlipNoise,
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
]
