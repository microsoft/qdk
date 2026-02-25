# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field

from ..._architecture import Architecture
from ...instruction_ids import CNOT, CZ, MEAS_Z, PAULI_I, H, T
from ..._instruction import ISA, Encoding, instruction


@dataclass
class AQREGateBased(Architecture):
    """
    A generic gate-based architecture based on the qubit parameters in Azure
    Quantum Resource Estimator (AQRE,
    [arXiv:2211.07629](https://arxiv.org/abs/2211.07629)).  The error rate can
    be set arbitrarily and is either 1e-3 or 1e-4 in the reference.  Gate times
    are set to 50ns and measurement times are set to 100ns, which are typical
    for superconducting transmon qubits
    [arXiv:cond-mat/0703002](https://arxiv.org/abs/cond-mat/0703002).

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

    @property
    def provided_isa(self) -> ISA:
        return ISA(
            instruction(
                PAULI_I,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=50,
                error_rate=self.error_rate,
            ),
            instruction(
                CNOT,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=50,
                error_rate=self.error_rate,
            ),
            instruction(
                CZ,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=50,
                error_rate=self.error_rate,
            ),
            instruction(
                H,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=50,
                error_rate=self.error_rate,
            ),
            instruction(
                MEAS_Z,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=100,
                error_rate=self.error_rate,
            ),
            instruction(
                T,
                encoding=Encoding.PHYSICAL,
                time=50,
                error_rate=self.error_rate,
            ),
        )
