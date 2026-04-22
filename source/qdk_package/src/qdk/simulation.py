# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Simulation utilities for the Q# ecosystem.

This module exposes the core building blocks for noise-aware quantum simulation:

- ``NeutralAtomDevice`` — models a neutral atom quantum device with configurable
  zone layouts, qubit registers, and movement constraints. Used to compile
  and simulate circuits on a realistic hardware topology.

- ``run_qir`` — simulates QIR as given in one of three backend simulators:
  clifford, gpu or cpu.

- ``NoiseConfig`` — configures per-gate Pauli noise (including qubit loss) for
  use with the QDK simulators. Assign noise tables to individual gate intrinsics
  to model depolarizing, bit-flip, phase-flip, or correlated noise channels.

- ``DensityMatrixSimulator`` and ``StateVectorSimulator`` — two experimental simulators
  that use density-matrices and state-vectors respectively to track their state.
"""

from qsharp._device._atom import NeutralAtomDevice
from qsharp._simulation import NoiseConfig, run_qir
from qsharp.noisy_simulator import (
    DensityMatrixSimulator,
    DensityMatrix,
    StateVectorSimulator,
    StateVector,
    Operation,
    Instrument,
)
