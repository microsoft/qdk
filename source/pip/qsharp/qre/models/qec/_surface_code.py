# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from dataclasses import KW_ONLY, dataclass, field
from typing import Generator
from ..._instruction import (
    ISA,
    ISARequirements,
    ISATransform,
    instruction,
    constraint,
    ConstraintBound,
    LOGICAL,
)
from ..._isa_enumeration import _Context
from ..._qre import linear_function
from ...instruction_ids import CNOT, GENERIC, H, LATTICE_SURGERY, MEAS_Z


@dataclass
class SurfaceCode(ISATransform):
    """
    Attributes:
        crossing_prefactor: float
            The prefactor for logical error rate due to error correction
            crossings.  (Default is 0.03, see Eq. (11) in arXiv:1208.0928)
        error_correction_threshold: float
            The error correction threshold for the surface code.  Default is
            0.01 (1%), see arXiv:1009.3686.

    Hyper parameters:
        distance: int
            The code distance of the surface code.

    References:
    - [arXiv:1208.0928](https://arxiv.org/abs/1208.0928)
    - [arXiv:1009.3686](https://arxiv.org/abs/1009.3686)
    """

    crossing_prefactor: float = 0.03
    error_correction_threshold: float = 0.01
    _: KW_ONLY
    distance: int = field(default=3, metadata={"domain": range(3, 26, 2)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(H, error_rate=ConstraintBound.lt(0.01)),
            constraint(CNOT, arity=2, error_rate=ConstraintBound.lt(0.01)),
            constraint(MEAS_Z, error_rate=ConstraintBound.lt(0.01)),
        )

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        cnot = impl_isa[CNOT]
        h = impl_isa[H]
        meas_z = impl_isa[MEAS_Z]

        cnot_time = cnot.expect_time()
        h_time = h.expect_time()
        meas_time = meas_z.expect_time()

        physical_error_rate = max(
            cnot.expect_error_rate(),
            h.expect_error_rate(),
            meas_z.expect_error_rate(),
        )

        space_formula = linear_function(2 * self.distance**2)

        time_value = (h_time + meas_time + cnot_time * 4) * self.distance

        error_formula = linear_function(
            self.crossing_prefactor
            * (
                (physical_error_rate / self.error_correction_threshold)
                ** ((self.distance + 1) // 2)
            )
        )

        generic = instruction(
            GENERIC,
            encoding=LOGICAL,
            arity=None,
            space=space_formula,
            time=time_value,
            error_rate=error_formula,
        )

        lattice_surgery = instruction(
            LATTICE_SURGERY,
            encoding=LOGICAL,
            arity=None,
            space=space_formula,
            time=time_value,
            error_rate=error_formula,
        )

        yield ISA(
            ctx.set_source(self, generic, [cnot, h, meas_z]),
            ctx.set_source(self, lattice_surgery, [cnot, h, meas_z]),
        )
