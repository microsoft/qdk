# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field
from typing import Optional

from ..._architecture import Architecture, _Context
from ...instruction_ids import CNOT, CZ, MEAS_Z, PAULI_I, H, T
from ..._instruction import ISA, Encoding


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

        return ctx.make_isa(
            ctx.add_instruction(
                PAULI_I,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.gate_time,
                error_rate=self.error_rate,
            ),
            ctx.add_instruction(
                CNOT,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=self.two_qubit_gate_time,
                error_rate=self.error_rate,
            ),
            ctx.add_instruction(
                CZ,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=self.two_qubit_gate_time,
                error_rate=self.error_rate,
            ),
            ctx.add_instruction(
                H,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.gate_time,
                error_rate=self.error_rate,
            ),
            ctx.add_instruction(
                MEAS_Z,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.measurement_time,
                error_rate=self.error_rate,
            ),
            ctx.add_instruction(
                T,
                encoding=Encoding.PHYSICAL,
                time=self.gate_time,
                error_rate=self.error_rate,
            ),
        )
