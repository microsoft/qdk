# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field

from ..._architecture import Architecture
from ...instruction_ids import CNOT, CZ, MEAS_Z, PAULI_I, H, T
from ..._instruction import ISA, Encoding, instruction


@dataclass
class AQREGateBased(Architecture):
    """
    References:
    - [arXiv:2211.07629](https://arxiv.org/abs/2211.07629)
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
