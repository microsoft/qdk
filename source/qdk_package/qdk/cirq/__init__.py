# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Cirq interoperability for the Q# ecosystem.

This module provides a :class:`~qsharp.interop.cirq.NeutralAtomSampler` — a standard
``cirq.Sampler`` that runs Cirq circuits on the local NeutralAtomDevice
simulator.

Usage:

    import cirq
    from qsharp.interop.cirq import NeutralAtomSampler

    q0, q1 = cirq.LineQubit.range(2)
    circuit = cirq.Circuit([
        cirq.H(q0),
        cirq.CNOT(q0, q1),
        cirq.measure(q0, q1, key="m"),
    ])

    sampler = NeutralAtomSampler(seed=42)
    result = sampler.run(circuit, repetitions=1000)
    print(result.histogram(key="m"))
"""

from ._neutral_atom import NeutralAtomSampler
from ._result import NeutralAtomCirqResult

__all__ = [
    "NeutralAtomSampler",
    "NeutralAtomCirqResult",
]
