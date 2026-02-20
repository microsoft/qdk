# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field

from ..._architecture import Architecture
from ...instruction_ids import (
    T,
    PREP_X,
    PREP_Z,
    MEAS_XX,
    MEAS_ZZ,
    MEAS_X,
    MEAS_Z,
)
from ..._instruction import ISA, instruction


@dataclass
class Majorana(Architecture):
    """
    This class models physical instructions that may be relevant for future
    Majorana qubits [71, 74, 117]. For these qubits, we assume that measurements
    and the physical T gate each take 1 µs. Owing to topological protection in
    the hardware, we assume single and two-qubit measurement error rates
    (Clifford error rates) in $10^{-4}$, $10^{-5}$, and $10^{-6}$ as a range
    between realistic and optimistic targets.  Non-Clifford operations in this
    architecture do not have topological protection, so we assume a 5%, 1.5%,
    and 1% error rate for non-Clifford physical T gates for the three cases,
    respectively.

    References:

    - Torsten Karzig, Christina Knapp, Roman M. Lutchyn, Parsa Bonderson,
      Matthew B. Hastings, Chetan Nayak, Jason Alicea, Karsten Flensberg,
      Stephan Plugge, Yuval Oreg, Charles M. Marcus, Michael H. Freedman:
      Scalable Designs for Quasiparticle-Poisoning-Protected Topological Quantum
      Computation with Majorana Zero Modes,
      [arXiv:1610.05289](https://arxiv.org/abs/1610.05289)
    - Alexei Kitaev: Unpaired Majorana fermions in quantum wires,
      [arXiv:cond-mat/0010440](https://arxiv.org/abs/cond-mat/0010440)
    - Sankar Das Sarma, Michael Freedman, Chetan Nayak: Majorana Zero Modes and
      Topological Quantum Computation,
      [arXiv:1501.02813](https://arxiv.org/abs/1501.02813)
    """

    _: KW_ONLY
    error_rate: float = field(default=1e-5, metadata={"domain": [1e-4, 1e-5, 1e-6]})

    @property
    def provided_isa(self) -> ISA:
        if abs(self.error_rate - 1e-4) <= 1e-8:
            t_error_rate = 0.05
        elif abs(self.error_rate - 1e-5) <= 1e-8:
            t_error_rate = 0.015
        elif abs(self.error_rate - 1e-6) <= 1e-8:
            t_error_rate = 0.01

        return ISA(
            instruction(
                PREP_X,
                time=1000,
                error_rate=self.error_rate,
            ),
            instruction(
                PREP_Z,
                time=1000,
                error_rate=self.error_rate,
            ),
            instruction(
                MEAS_XX,
                arity=2,
                time=1000,
                error_rate=self.error_rate,
            ),
            instruction(
                MEAS_ZZ,
                arity=2,
                time=1000,
                error_rate=self.error_rate,
            ),
            instruction(
                MEAS_X,
                time=1000,
                error_rate=self.error_rate,
            ),
            instruction(
                MEAS_Z,
                time=1000,
                error_rate=self.error_rate,
            ),
            instruction(
                T,
                time=1000,
                error_rate=t_error_rate,
            ),
        )
