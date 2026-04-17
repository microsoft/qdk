# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Qiskit interoperability for the Q# ecosystem.

This module provides Qiskit backends backed by the local Q# simulator and
NeutralAtomDevice, allowing Qiskit circuits to be run locally without any
cloud connection.

Available backends:

:class:`~qsharp.interop.qiskit.QSharpBackend`
    Runs any Qiskit ``QuantumCircuit`` using the Q# simulator. Supports
    noise-free simulation via QASM export and QIR compilation.

:class:`~qsharp.interop.qiskit.NeutralAtomBackend`
    Runs Qiskit circuits on the local NeutralAtomDevice simulator. Decomposes
    gates to the native ``{Rz, SX, CZ}`` gate set and optionally models
    per-gate noise (including qubit loss). Loss shots are exposed separately
    from accepted shots in the job result.

:class:`~qsharp.interop.qiskit.ResourceEstimatorBackend`
    Estimates quantum resources (qubits, T-gates, etc.) for a Qiskit circuit
    without running a full simulation.

:func:`~qsharp.interop.qiskit.estimate`
    Convenience function that runs resource estimation on a Qiskit circuit
    and returns an :class:`~qsharp.estimator.EstimatorResult` directly, without
    needing to construct a backend or job manually.

Usage:

    from qiskit import QuantumCircuit
    from qsharp.interop.qiskit import NeutralAtomBackend
    from qsharp._simulation import NoiseConfig

    circuit = QuantumCircuit(2, 2)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.measure([0, 1], [0, 1])

    noise = NoiseConfig()
    noise.rz.loss = 0.05  # 5% qubit loss per Rz gate

    backend = NeutralAtomBackend()
    job = backend.run(circuit, shots=1000, noise=noise, seed=42)
    result = job.result()
    print(result.results[0].data.counts)      # accepted shots only
    print(result.results[0].data.raw_counts)  # includes loss shots
"""
from typing import Any, Dict, List, Optional, Union

from ...estimator import EstimatorParams, EstimatorResult
from ..._native import OutputSemantics, ProgramType, QasmError
from .backends import (
    NeutralAtomBackend,
    QSharpBackend,
    ResourceEstimatorBackend,
    QirTarget,
)
from .jobs import QsJob, QsSimJob, ReJob, QsJobSet
from .execution import DetaultExecutor
from qiskit import QuantumCircuit


def estimate(
    circuit: QuantumCircuit,
    params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None,
    **options,
) -> EstimatorResult:
    """
    Estimates resources for Qiskit QuantumCircuit.

    :param circuit: The input Qiskit QuantumCircuit object.
    :param params: The parameters to configure physical estimation.
    :type params: EstimatorParams or dict or list
    :param **options: Additional options for the transpiler, exporter, or Qiskit passes
        configuration. Defaults to backend config values. Common options:

        - ``optimization_level`` (int): Transpiler optimization level.
        - ``basis_gates`` (list): Basis gates for transpilation.
        - ``includes`` (list): Include paths for QASM resolution.
        - ``search_path`` (str): Search path for resolving file references.
    :raises QasmError: If there is an error generating or parsing QASM.
    :return: The estimated resources.
    :rtype: EstimatorResult
    """
    from ..._qsharp import ipython_helper

    ipython_helper()
    backend = ResourceEstimatorBackend()
    job = backend.run(circuit, params=params, **options)
    return job.result()


# __all__ = [
#     "NeutralAtomBackend",
#     "QSharpBackend",
#     "ResourceEstimatorBackend",
#     "QirTarget",
#     "QsJob",
#     "QsSimJob",
#     "ReJob",
#     "QsJobSet",
#     "estimate",
#     "EstimatorParams",
#     "EstimatorResult",
# ]
