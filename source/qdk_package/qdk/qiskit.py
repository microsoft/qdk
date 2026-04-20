# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Qiskit interoperability for the Q# ecosystem.

This module provides Qiskit backends backed by the local Q# simulator and
NeutralAtomDevice, allowing Qiskit circuits to be run locally without any
cloud connection.

Available backends
------------------
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

Usage::

    from qiskit import QuantumCircuit
    from qdk.qiskit import NeutralAtomBackend
    from qdk.simulation import NoiseConfig

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

try:
    from qsharp.interop.qiskit import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.qiskit requires the qiskit extra. Install with 'pip install qdk[qiskit]'."
    ) from ex
