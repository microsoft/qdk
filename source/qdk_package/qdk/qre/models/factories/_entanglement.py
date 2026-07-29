# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import dataclass
from math import ceil
from typing import Generator

from ... import ISA, ISARequirements, ISATransform, LOGICAL, PHYSICAL, constraint
from ..._architecture import ISAContext
from ...instruction_ids import CNOT, BELL_STATE, MEAS_Z


@dataclass
class BBPSSWeBitFactory(ISATransform):
    """
    Implements the original Bennett, Brassard, Popescu, Schumacher, Smolin,
    and Wootters (BBPSSW) entanglement purification protocol for producing
    a high-fidelity Bell state from two low-fidelity Bell states.

    The accepable rate of the protocol is 1 - 4p_e/3 + 8p_e^2/9 where p_e
    is the error rate of the input Bell states. (To do: add Clifford and
    measurement error contributions to the acceptance rate.)

    The output Bell state has two contributions to its error rate:
    - Distillation error: (6p_e - 2p_e^2) / (9 - 12p_e + 8p_e^2) where p_e is
      the error rate of the input Bell states.
    - Clifford error: 2*p_L where p_L is the error rate of the CNOT
    - Measurement error: 2*p_M where p_M is the error rate of the MEAS_Z

    The factory production time includes an overhead factor of (1 + 8·p_T) to
    account for the failure probability when consuming the T states.

    Reference:
        - C.H. Bennett, G. Brassard, S. Popescu, B. Schumacher, J.A. Smolin,
          and W.K. Wootters, "Purification of noisy entanglement and faithful
          teleportation via noisy channels," Phys. Rev. Lett. 76, 722 (1996).
    """

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(BELL_STATE, encoding=PHYSICAL),
            constraint(CNOT, encoding=PHYSICAL),
            constraint(MEAS_Z, encoding=PHYSICAL),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        # Required gates and states
        cnot = impl_isa[CNOT]
        bell_state = impl_isa[BELL_STATE]
        meas_z = impl_isa[MEAS_Z]

        # Alice and Bob obtain two Bell states (in parallel). The entire
        # circuit is two CNOTs and two measurements (one of each for each
        # of Alice and Bob which they run concurrently).
        num_physical_qubits = 2 * bell_state.expect_space()
        single_shot_time = (
            bell_state.expect_time() + cnot.expect_time() + meas_z.expect_time()
        )

        # The error rate of the input Bell states
        ebit_error = bell_state.expect_error_rate()
        cnot_error = cnot.expect_error_rate()
        meas_error = meas_z.expect_error_rate()

        failure_rate = 4 * ebit_error / 3 + 8 * ebit_error**2 / 9
        output_error_rate = (6 * ebit_error - 2 * ebit_error**2) / (
            9 - 12 * ebit_error + 8 * ebit_error**2
        )

        yield ctx.make_isa(
            ctx.add_instruction(
                BELL_STATE,
                arity=2,
                encoding=PHYSICAL,
                space=num_physical_qubits,
                time=single_shot_time,
                error_rate=output_error_rate,
                transform=self,
                source=[cnot, bell_state, meas_z],
            )
        )
