# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from dataclasses import KW_ONLY, dataclass, field
from typing import Generator

from ..._architecture import _Context
from ..._instruction import (
    LOGICAL,
    ISATransform,
    constraint,
    instruction,
)
from ..._qre import (
    ISA,
    ISARequirements,
    linear_function,
)
from ...instruction_ids import (
    LATTICE_SURGERY,
    MEAS_X,
    MEAS_XX,
    MEAS_Z,
    MEAS_ZZ,
)


@dataclass
class ThreeAux(ISATransform):
    """
    This class models the pairwise measurement-based surface code with three
    auxiliary qubits per stabilizer measurement.

    Hyper parameters:
        distance: int
            The code distance of the surface code.
        single_rail: bool
            Whether to use single-rail encoding.

    References:

    - Linnea Grans-Samuelsson, Ryan V. Mishmash, David Aasen, Christina Knapp,
      Bela Bauer, Brad Lackey, Marcus P. da Silva, Parsa Bonderson: Improved
      Pairwise Measurement-Based Surface Code,
      [arXiv:2310.12981](https://arxiv.org/abs/2310.12981)
    """

    _: KW_ONLY
    distance: int = field(default=3, metadata={"domain": range(3, 26, 2)})
    single_rail: bool = field(default=False)

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(MEAS_X),
            constraint(MEAS_Z),
            constraint(MEAS_XX, arity=2),
            constraint(MEAS_ZZ, arity=2),
        )

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        meas_x = impl_isa[MEAS_X]
        meas_z = impl_isa[MEAS_Z]
        meas_xx = impl_isa[MEAS_XX]
        meas_zz = impl_isa[MEAS_ZZ]

        gate_time = max(meas_xx.expect_time(), meas_zz.expect_time())

        physical_error_rate = max(
            meas_x.expect_error_rate(),
            meas_z.expect_error_rate(),
            meas_xx.expect_error_rate(),
            meas_zz.expect_error_rate(),
        )

        # See arXiv:2310.12981, Table 1 and Figs. 2, 3, 4, 6, and 7
        depth = 5 if self.single_rail else 4

        # See arXiv:2310.12981, Table 1
        error_correction_threshold = 0.0051 if self.single_rail else 0.0066

        # See arXiv:2310.12981, Fig. 23
        crossing_prefactor = 0.05

        # d^2 data qubits and 3 qubits for each of the d^2 - 1 stabilizer
        # measurements
        space_formula = linear_function(4 * self.distance**2 - 3)

        # The measurement circuits do not overlap perfectly, so there is an
        # additional 4 steps that need to be accounted for independent of the
        # distance (see Section 2 between Eqs. (2) and (3) in arXiv:2310.12981)
        time_value = gate_time * (depth * self.distance + 4)

        # Typical fitting curve for surface code logical error (see
        # arXiv:1208.0928)
        error_formula = linear_function(
            crossing_prefactor
            * (
                (physical_error_rate / error_correction_threshold)
                ** ((self.distance + 1) // 2)
            )
        )

        lattice_surgery = instruction(
            LATTICE_SURGERY,
            encoding=LOGICAL,
            arity=None,
            space=space_formula,
            time=time_value,
            error_rate=error_formula,
            distance=self.distance,
        )

        yield ISA(
            ctx.set_source(self, lattice_surgery, [meas_x, meas_z, meas_xx, meas_zz])
        )
