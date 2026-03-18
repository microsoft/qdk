# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Cirq interoperability for the Q# ecosystem.

This module provides utilities for running Cirq circuits on the local
NeutralAtomDevice simulator.

Usage::

    from qsharp.interop.cirq import simulate_with_neutral_atom

    result = simulate_with_neutral_atom(circuit, shots=1000, seed=42)
    print(result.histogram(key="m"))
"""

try:
    from ._neutral_atom import simulate_with_neutral_atom
    from ._result import NeutralAtomCirqResult
except ImportError:
    pass

__all__ = [
    "simulate_with_neutral_atom",
    "NeutralAtomCirqResult",
]
