# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Simulation utilities for the Q# ecosystem.

This module exposes the core building blocks for noise-aware quantum simulation:

- :class:`~qdk.simulation.NeutralAtomDevice` — models a
  neutral atom quantum device with configurable zone layouts, qubit registers,
  and movement constraints. Used to compile and simulate circuits on a
  realistic hardware topology.

- :class:`~qdk.simulation.NoiseConfig` — configures per-gate Pauli noise
  (including qubit loss) for use with the Q# simulator. Assign noise tables
  to individual gate intrinsics to model depolarizing, bit-flip, phase-flip,
  or correlated noise channels.

- :func:`~qdk.simulation.run_qir` — simulates QIR as given in one of
  three backend simulators: clifford, gpu or cpu.

- :class:`~qdk.simulation.DensityMatrixSimulator` — an experimental simulator that uses
  a density-matrix to track its state.

- :class:`~qdk.simulation.StateVectorSimulator` — an experimental simulator that uses
  a state-vector to track its state.
"""

from .._device._atom import NeutralAtomDevice
from ._simulation import NoiseConfig, run_qir
from ._noisy_simulator import (
    NoisySimulatorError,
    DensityMatrixSimulator,
    StateVectorSimulator,
    DensityMatrix,
    StateVector,
    Operation,
    Instrument,
)

__all__ = [
    "NeutralAtomDevice",
    "NoiseConfig",
    "run_qir",
    "NoisySimulatorError",
    "Operation",
    "Instrument",
    "DensityMatrixSimulator",
    "StateVectorSimulator",
    "DensityMatrix",
    "StateVector",
]
