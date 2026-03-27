# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from __future__ import annotations

from dataclasses import dataclass

import cirq

from .._application import Application
from .._qre import Trace
from ..interop import trace_from_cirq


@dataclass
class CirqApplication(Application[None]):
    """Application that produces a resource estimation trace from a Cirq circuit.

    Accepts either a Cirq ``Circuit`` object or an OpenQASM string. When a
    QASM string is provided, it is parsed into a circuit using
    ``cirq.contrib.qasm_import`` (requires the optional ``ply`` dependency).

    Args:
        circuit_or_qasm: A Cirq Circuit or an OpenQASM string.
        classical_control_probability: Probability that a classically
            controlled operation is included in the trace. Defaults to 0.5.
    """

    circuit_or_qasm: str | cirq.CIRCUIT_LIKE
    classical_control_probability: float = 0.5

    def __post_init__(self):
        if isinstance(self.circuit_or_qasm, str):
            try:
                from cirq.contrib.qasm_import import circuit_from_qasm

                self._circuit = circuit_from_qasm(self.circuit_or_qasm)
            except ImportError:
                raise ImportError(
                    "Missing optional 'ply' dependency. To install run: "
                    "pip install ply"
                )
        else:
            self._circuit = self.circuit_or_qasm

    def get_trace(self, parameters: None = None) -> Trace:
        return trace_from_cirq(self._circuit)
