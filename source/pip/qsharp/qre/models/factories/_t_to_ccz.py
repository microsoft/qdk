from dataclasses import dataclass
from math import ceil
from typing import Generator

from ... import ISA, ISARequirements, ISATransform, LOGICAL, constraint
from ..._architecture import ISAContext
from ...instruction_ids import CCX, LATTICE_SURGERY, T


@dataclass
class GSJ24CCXFactory(ISATransform):
    """
    Implements the 8|T⟩ → |CCX⟩ magic state factory described in Fig. 24 of
    Gidney, Shutty, and Jones (2024). This design converts eight T magic
    states into a single CCX (Toffoli) state using lattice surgery operations
    on 12 logical qubits (including helper qubits) with a circuit depth of 6.

    The output CCX error rate has two contributions:

    - Distillation error: 28 · p_T², where p_T is the T state error rate
      (from pairs of T states failing simultaneously).
    - Logical error: accumulated over 6 lattice surgery rounds on 12 qubits.

    The factory production time includes an overhead factor of (1 + 8·p_T) to
    account for the failure probability when consuming the T states.

    Reference:
        - C. Gidney, C. Shutty, C. Jones, "Magic state cultivation: growing
          T states with 78% reduced overhead", arXiv:2409.17595 (2024).
          https://arxiv.org/abs/2409.17595
        - C. Gidney, A. G. Fowler, "Efficient magic state factories with a
          catalyzed |CCZ⟩ to 2|T⟩ transformation", Quantum 3, 135 (2019).
          arXiv:1812.01238. https://arxiv.org/abs/1812.01238
    """

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(LATTICE_SURGERY, arity=None, encoding=LOGICAL),
            constraint(T, encoding=LOGICAL),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        # Required gates and states
        lattice_surgery = impl_isa[LATTICE_SURGERY]
        t_state = impl_isa[T]

        # The number of logical qubits including helper qubits (see Fig. 24 in
        # arXiv:2409.17595)
        num_logical_qubits = 12

        # We derive the number of qubits per logical qubit from the
        # LATTICE_SURGERY gate
        num_physical_qubits = impl_isa[LATTICE_SURGERY].expect_space(num_logical_qubits)

        # The depth of the CCX factory after consuming the T states (see Fig.
        # 24 in arXiv:2409.17595)
        depth = 6

        # The error rate of the T states
        t_error = t_state.expect_error_rate()

        # The time to produce the T states
        t_state_time = (
            (t_state.expect_space() * t_state.expect_time()) * 8 / num_physical_qubits
        )

        # The time to produce a CCX state from the T states includes an
        # overhead to account for the failure probability
        ccx_time = (
            (1 + 8 * t_error) * depth * lattice_surgery.expect_time(num_logical_qubits)
        )

        # The error rate of the produced CCX state consists of the error from
        # two T states failing as well as the logical error to execute the 6
        # lattice surgery operations on all qubits.
        error_rate = 28 * (t_error**2) + depth * lattice_surgery.expect_error_rate(
            num_logical_qubits
        )

        yield ctx.make_isa(
            ctx.add_instruction(
                CCX,
                arity=3,
                encoding=LOGICAL,
                length=num_logical_qubits,
                space=num_physical_qubits,
                time=ceil(t_state_time + ccx_time),
                error_rate=error_rate,
                transform=self,
                source=[t_state, lattice_surgery],
            )
        )
