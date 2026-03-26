# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from __future__ import annotations

from dataclasses import dataclass

from cirq import Circuit


from .._qre import Trace
from .._application import Application
from ..interop import trace_from_cirq


@dataclass
class CirqApplication(Application[None]):
    def __init__(self, circuit_or_qasm: str | Circuit):
        if isinstance(circuit_or_qasm, str):
            try:
                from cirq.contrib.qasm_import import circuit_from_qasm

                self._circuit = circuit_from_qasm(circuit_or_qasm)
            except ImportError:
                raise ImportError(
                    "Missing optional 'ply' dependency. To install run: "
                    "pip install ply"
                )
        else:
            self._circuit = circuit_or_qasm

    def get_trace(self, parameters: None = None) -> Trace:
        return trace_from_cirq(self._circuit)
