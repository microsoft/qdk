# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Simulation utilities for the Q# ecosystem.

This module exposes the core building blocks for noise-aware quantum simulation:

- :class:`~qsharp._device._atom.NeutralAtomDevice` — models a
  neutral atom quantum device with configurable zone layouts, qubit registers,
  and movement constraints. Used to compile and simulate circuits on a
  realistic hardware topology.

- :class:`~qsharp._simulation.NoiseConfig` — configures per-gate Pauli noise
  (including qubit loss) for use with the Q# simulator. Assign noise tables
  to individual gate intrinsics to model depolarizing, bit-flip, phase-flip,
  or correlated noise channels.

- :func:`~qsharp._simulation.run_qir` — simulates QIR as given in one of
  three backend simulators: clifford, gpu or cpu.

- :class:`~qsharp.noisy_simulator.DensityMatrixSimulator` — an experimental simulator that uses
  a density-matrix to track its state.

- :class:`~qsharp.noisy_simulator.StateVectorSimulator` — an experimental simulator that uses
  a state-vector to track its state.
"""

from ._device._atom import NeutralAtomDevice
from ._simulation import NoiseConfig, run_qir
from .noisy_simulator import (
    DensityMatrixSimulator,
    StateVectorSimulator,
    DensityMatrix,
    StateVector,
    Operation,
    Instrument,
)
