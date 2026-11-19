# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Simulation utilities for the Q# ecosystem.

This module exposes the core building blocks for quantum simulation: noise-aware
simulators, and a tensor-network simulator:

- :class:`~qdk.simulation.NeutralAtomDevice` — models a
  neutral atom quantum device with configurable zone layouts, qubit registers,
  and movement constraints. Used to compile and simulate circuits on a
  realistic hardware topology.

- :class:`~qdk.simulation.NoiseConfig` — configures per-gate Pauli noise
  (including qubit loss) for use with the Q# simulator. Assign noise tables
  to individual gate intrinsics to model depolarizing, bit-flip, phase-flip,
  or correlated noise channels.

- :class:`~qdk.simulation.LossPolicy` — selects how a gate behaves when one of
  its qubit operands is lost. Assign it to a noise table's ``on_loss`` attribute
  (e.g. ``noise.cx.on_loss = LossPolicy.SKIP``).

- :func:`~qdk.simulation.run_qir` — simulates QIR as given in one of
  three backend simulators: clifford, gpu or cpu.

- :func:`~qdk.simulation.tensornetwork_qir` — simulates QIR with a
  tensor-network simulation method: shots on a matrix product state, on an
  NVIDIA GPU (``engine="gpu"``, the default, after
  ``pip install "qdk[tensornetwork]"``) or the CPU (``engine="cpu"``). It
  takes :class:`~qdk.simulation.Sample` queries and
  :class:`~qdk.simulation.MpsOptions`, and returns
  :class:`~qdk.simulation.TensorNetworkResults` with the run's
  :class:`~qdk.simulation.TensorNetworkReport`;
  :func:`~qdk.simulation.check_tensornetwork_setup` checks the GPU setup
  and returns a :class:`~qdk.simulation.TensorNetworkSetup`.

- :class:`~qdk.simulation.DensityMatrixSimulator` — an experimental simulator that uses
  a density-matrix to track its state.

- :class:`~qdk.simulation.StateVectorSimulator` — an experimental simulator that uses
  a state-vector to track its state.
"""

from .._device._atom import NeutralAtomDevice
from ._simulation import NoiseConfig, LossPolicy, run_qir
from ._tensor_network import (
    MpsOptions,
    Sample,
    TensorNetworkSetup,
    TensorNetworkReport,
    TensorNetworkResults,
    check_tensornetwork_setup,
    tensornetwork_qir,
)
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
    "LossPolicy",
    "run_qir",
    "tensornetwork_qir",
    "check_tensornetwork_setup",
    "Sample",
    "MpsOptions",
    "TensorNetworkResults",
    "TensorNetworkReport",
    "TensorNetworkSetup",
    "NoisySimulatorError",
    "Operation",
    "Instrument",
    "DensityMatrixSimulator",
    "StateVectorSimulator",
    "DensityMatrix",
    "StateVector",
]
