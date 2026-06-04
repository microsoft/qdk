# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from __future__ import annotations

from dataclasses import dataclass, field

import cirq

from ... import telemetry_events
from .._application import Application
from .._qre import Trace
from ..interop import trace_from_cirq


@dataclass
class CirqApplicationParams:
    """Application parameters that control how the resource estimation trace is generated from a Cirq circuit.

    Args:
        track_memory_qubits (bool): When True, memory qubits are tracked
            separately from compute qubits. When False, all qubits are treated
            as compute qubits. Also, if True, read-from-memory and
            write-to-memory instructions are preserved in the trace, otherwise,
            they are decompsed into SWAP and RESET instructions.  Defaults to
            True.
    """
    track_memory_qubits: bool = field(default=True, metadata={"domain": [True]})


@dataclass
class CirqApplication(Application[CirqApplicationParams]):
    """Application that produces a resource estimation trace from a Cirq circuit.

    Accepts either a Cirq ``Circuit`` object or an OpenQASM string. When a
    QASM string is provided, it is parsed into a circuit using
    ``cirq.contrib.qasm_import`` (requires the optional ``ply`` dependency).

    Args:
        circuit_or_qasm: A Cirq Circuit or an OpenQASM string.
        classical_control_probability: Probability that a classically
            controlled operation is included in the trace. Defaults to 0.5.
        rotation_threshold: Rotation exponents with absolute value below
            this threshold are treated as identity and omitted from the
            trace. This applies to single-qubit rotations (RX, RY, RZ) as
            well as to the rotation components of controlled-Z
            decompositions. Defaults to 1e-6.
    """

    circuit_or_qasm: str | cirq.CIRCUIT_LIKE
    classical_control_probability: float = 0.5
    rotation_threshold: float = 1e-6

    def __post_init__(self):
        telemetry_events.on_qre_application_created("CirqApplication")
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

    def get_trace(self, parameters: CirqApplicationParams = CirqApplicationParams()) -> Trace:
        """Return the resource estimation trace for the Cirq circuit.

        Args:
            parameters (None): Unused. Defaults to None.

        Returns:
            Trace: The resource estimation trace.
        """
        return trace_from_cirq(
            self._circuit,
            classical_control_probability=self.classical_control_probability,
            rotation_threshold=self.rotation_threshold,
            track_memory_qubits=parameters.track_memory_qubits,
        )
