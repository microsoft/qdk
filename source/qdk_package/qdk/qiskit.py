# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Qiskit interoperability for the Q# ecosystem.

This module re-exports all public symbols from [qsharp.interop.qiskit](:mod:`qsharp.interop.qiskit`),
making them available under the ``qdk.qiskit`` namespace. It provides Qiskit
backends backed by the local Q# simulator and NeutralAtomDevice, allowing
Qiskit circuits to be run locally without any cloud connection.

Key exports:

- :class:`~qsharp.interop.qiskit.backends.qsharp_backend.QSharpBackend`
- :class:`~qsharp.interop.qiskit.backends.neutral_atom_backend.NeutralAtomBackend`
- :class:`~qsharp.interop.qiskit.backends.re_backend.ResourceEstimatorBackend`
- :func:`~qsharp.interop.qiskit.estimate`

For full API documentation see [qsharp.interop.qiskit](:mod:`qsharp.interop.qiskit`).

Requires the ``qiskit`` extra: ``pip install qdk[qiskit]``.

Usage:

    from qiskit import QuantumCircuit
    from qdk.qiskit import NeutralAtomBackend

    circuit = QuantumCircuit(2, 2)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.measure([0, 1], [0, 1])

    backend = NeutralAtomBackend()
    job = backend.run(circuit, shots=1000)
    result = job.result()
    print(result.results[0].data.counts)
"""

try:
    from qsharp.interop.qiskit import *  # pyright: ignore[reportWildcardImportFromLibrary]
except Exception as ex:
    raise ImportError(
        "qdk.qiskit requires the qiskit extra. Install with 'pip install qdk[qiskit]'."
    ) from ex
