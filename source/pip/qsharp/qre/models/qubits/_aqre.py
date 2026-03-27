# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field
from typing import Optional

from ..._architecture import Architecture, _Context
from ..._instruction import ISA, Encoding
from ...instruction_ids import (
    CNOT,
    CZ,
    MEAS_X,
    MEAS_Y,
    MEAS_Z,
    PAULI_I,
    PAULI_X,
    PAULI_Y,
    PAULI_Z,
    RX,
    RY,
    RZ,
    S_DAG,
    SQRT_X,
    SQRT_X_DAG,
    SQRT_Y,
    SQRT_Y_DAG,
    SQRT_SQRT_X,
    SQRT_SQRT_X_DAG,
    SQRT_SQRT_Y,
    SQRT_SQRT_Y_DAG,
    T_DAG,
    H,
    S,
    T,
)


@dataclass
class AQREGateBased(Architecture):
    """
    A generic gate-based architecture based on the qubit parameters in Azure
    Quantum Resource Estimator (AQRE,
    [arXiv:2211.07629](https://arxiv.org/abs/2211.07629)).  The error rate can
    be set arbitrarily and is either 1e-3 or 1e-4 in the reference.  Typical
    gate times are 50ns and measurement times are 100ns for superconducting
    transmon qubits
    [arXiv:cond-mat/0703002](https://arxiv.org/abs/cond-mat/0703002).

    Args:
        error_rate: The error rate for all gates. Defaults to 1e-4.
        gate_time: The time (in ns) for single-qubit gates.
        measurement_time: The time (in ns) for measurement operations.
        two_qubit_gate_time: The time (in ns) for two-qubit gates (CNOT, CZ).
            If not provided, defaults to the value of ``gate_time``.

    References:

    - Michael E. Beverland, Prakash Murali, Matthias Troyer, Krysta M. Svore,
      Torsten Hoefler, Vadym Kliuchnikov, Guang Hao Low, Mathias Soeken, Aarthi
      Sundaram, Alexander Vaschillo: Assessing requirements to scale to
      practical quantum advantage,
      [arXiv:2211.07629](https://arxiv.org/abs/2211.07629)
    - Jens Koch, Terri M. Yu, Jay Gambetta, A. A. Houck, D. I. Schuster, J.
      Majer, Alexandre Blais, M. H. Devoret, S. M. Girvin, R. J. Schoelkopf:
      Charge insensitive qubit design derived from the Cooper pair box,
      [arXiv:cond-mat/0703002](https://arxiv.org/abs/cond-mat/0703002)
    """

    _: KW_ONLY
    error_rate: float = field(default=1e-4)
    gate_time: int
    measurement_time: int
    two_qubit_gate_time: Optional[int] = field(default=None)

    def __post_init__(self):
        if self.two_qubit_gate_time is None:
            self.two_qubit_gate_time = self.gate_time

    def provided_isa(self, ctx: _Context) -> ISA:
        # Value is initialized in __post_init__
        assert self.two_qubit_gate_time is not None

        # NOTE: This can be improved with instruction coercion once implemented.
        instructions = []

        # Single-qubit gates
        single = [
            PAULI_I,
            PAULI_X,
            PAULI_Y,
            PAULI_Z,
            H,
            SQRT_X,
            SQRT_X_DAG,
            SQRT_Y,
            SQRT_Y_DAG,
            S,
            S_DAG,
            SQRT_SQRT_X,
            SQRT_SQRT_X_DAG,
            SQRT_SQRT_Y,
            SQRT_SQRT_Y_DAG,
            T,
            T_DAG,
            RX,
            RY,
            RZ,
        ]

        for instr in single:
            instructions.append(
                ctx.add_instruction(
                    instr,
                    encoding=Encoding.PHYSICAL,
                    arity=1,
                    time=self.gate_time,
                    error_rate=self.error_rate,
                )
            )

        for instr in [MEAS_X, MEAS_Y, MEAS_Z]:
            instructions.append(
                ctx.add_instruction(
                    instr,
                    encoding=Encoding.PHYSICAL,
                    arity=1,
                    time=self.measurement_time,
                    error_rate=self.error_rate,
                )
            )

        # Two-qubit gates
        for instr in [CNOT, CZ]:
            instructions.append(
                ctx.add_instruction(
                    instr,
                    encoding=Encoding.PHYSICAL,
                    arity=2,
                    time=self.two_qubit_gate_time,
                    error_rate=self.error_rate,
                )
            )

        return ctx.make_isa(*instructions)
