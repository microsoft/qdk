# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations
from dataclasses import KW_ONLY, dataclass, field
import math
from typing import Generator, Optional
from ..._instruction import (
    ISA,
    ISARequirements,
    ISATransform,
    constraint,
    ConstraintBound,
    LOGICAL,
)
from ..._isa_enumeration import ISAContext
from ..._qre import linear_function
from ...instruction_ids import (
    CZ,
    LATTICE_SURGERY,
    MEAS_RESET_Z,
    MEAS_Z,
    PHYSICAL_MOVE,
    RZ,
    SQRT_X,
)
from ...property_keys import (
    SURFACE_CODE_ONE_QUBIT_TIME_FACTOR,
    SURFACE_CODE_TWO_QUBIT_TIME_FACTOR,
    VELOCITY,
    ACCELERATION,
    ATOM_SPACING,
)


@dataclass
class SurfaceCodeLowMove(ISATransform):
    """
    This class models the gate-based rotated surface code.

    Attributes:
        crossing_prefactor: float
            The prefactor for logical error rate due to error correction
            crossings.  (Default is 0.03, see Eq. (11) in
            [arXiv:1208.0928](https://arxiv.org/abs/1208.0928))
        error_correction_threshold: float
            The error correction threshold for the surface code.  (Default is
            0.01 (1%), see [arXiv:1009.3686](https://arxiv.org/abs/1009.3686))
        one_qubit_gate_depth: int
            The depth of one-qubit gates in each syndrome extraction cycle.
            (Default is 1, see Fig. 2 in [arXiv:1009.3686](https://arxiv.org/abs/1009.3686))
        two_qubit_gate_depth: int
            The depth of two-qubit gates in each syndrome extraction cycle.
            (Default is 4, see Fig. 2 in [arXiv:1009.3686](https://arxiv.org/abs/1009.3686))
        code_cycle_override: Optional[int]
            If provided, this value will be used as the time for each syndrome
            extraction cycle instead of the default calculation based on gate
            times and depths. (Default is None)
        code_cycle_offset: int
            An additional time offset to add to the syndrome extraction cycle
            time. (Default is 0)

    Hyper parameters:
        distance: int
            The code distance of the surface code.

    References:

    - Dominic Horsman, Austin G. Fowler, Simon Devitt, Rodney Van Meter: Surface
      code quantum computing by lattice surgery,
      [arXiv:1111.4022](https://arxiv.org/abs/1111.4022)
    - Austin G. Fowler, Matteo Mariantoni, John M. Martinis, Andrew N. Cleland:
      Surface codes: Towards practical large-scale quantum computation,
      [arXiv:1208.0928](https://arxiv.org/abs/1208.0928)
    - David S. Wang, Austin G. Fowler, Lloyd C. L. Hollenberg: Quantum computing
      with nearest neighbor interactions and error rates over 1%,
      [arXiv:1009.3686](https://arxiv.org/abs/1009.3686)
    """

    crossing_prefactor: float = 0.03
    error_correction_threshold: float = 0.01
    one_qubit_gate_depth: int = 1
    two_qubit_gate_depth: int = 4
    code_cycle_override: Optional[int] = None
    code_cycle_offset: int = 0
    _: KW_ONLY
    distance: int = field(default=3, metadata={"domain": range(3, 26, 2)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(RZ, error_rate=ConstraintBound.lt(0.01)),
            constraint(SQRT_X, error_rate=ConstraintBound.lt(0.01)),
            constraint(CZ, arity=2, error_rate=ConstraintBound.lt(0.01)),
            constraint(MEAS_Z, error_rate=ConstraintBound.lt(0.01)),
            constraint(MEAS_RESET_Z, error_rate=ConstraintBound.lt(0.01)),
            constraint(PHYSICAL_MOVE, error_rate=ConstraintBound.lt(0.01)),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        cz = impl_isa[CZ]
        rz = impl_isa[RZ]
        sqrt_x = impl_isa[SQRT_X]
        reset = impl_isa[MEAS_RESET_Z]
        meas_z = impl_isa[MEAS_Z]

        move = impl_isa[PHYSICAL_MOVE]
        if (
            move.has_property(VELOCITY)
            and move.has_property(ACCELERATION)
            and move.has_property(ATOM_SPACING)
        ):
            max_vel = move.get_property_or(VELOCITY, 0)
            max_accel = move.get_property_or(ACCELERATION, 0)
            atom_spacing = move.get_property_or(ATOM_SPACING, 0)
            if atom_spacing < max_vel**2 / max_accel:
                hor_seg_time = math.sqrt(atom_spacing / max_accel)
            else:
                extra_distance = atom_spacing - max_vel**2 / max_accel
                hor_seg_time = max_vel / max_accel + extra_distance / max_vel
            if math.sqrt(2) * atom_spacing < max_vel**2 / max_accel:
                diag_seg_time = math.sqrt(math.sqrt(2) * atom_spacing / max_accel)
            else:
                extra_distance = math.sqrt(2) * atom_spacing - max_vel**2 / max_accel
                diag_seg_time = max_vel / max_accel + extra_distance / max_vel
            move_time = 3 * move.expect_time() + 2 * hor_seg_time + diag_seg_time
        else:
            move_time = move.expect_time()

        four_cz_time = math.ceil(4 * cz.expect_time() + move_time)
        h_time = sqrt_x.expect_time() + 2 * rz.expect_time()
        meas_time = meas_z.expect_time()
        reset_time = reset.expect_time()

        physical_error_rate = max(
            rz.expect_error_rate(),
            cz.expect_error_rate(),
            sqrt_x.expect_error_rate(),
            reset.expect_error_rate(),
            meas_z.expect_error_rate(),
        )

        # There are d^2 data qubits and (d^2 - 1) ancilla qubits in the rotated
        # surface code.  (See Section 7.1 in arXiv:1111.4022)
        # Unchanged from the original SurfaceCode.
        space_formula = linear_function(2 * self.distance**2 - 1)

        # Each standard syndrome extraction cycle consists of ancilla preparation, 4
        # rounds of CNOTs, and measurement.  (See Fig. 2 in arXiv:1009.3686).
        # But this must be modified to acount for the fact that the CNOTs are
        # implemented as CZ+sqrt(X). The syndrome extraction cycle
        # is repeated d times for a distance-d code.
        if self.code_cycle_override is not None:
            code_cycle_time = self.code_cycle_override + self.code_cycle_offset
        else:
            if reset_time > four_cz_time:
                code_cycle_time = (
                    max(reset_time, h_time)
                    + (self.distance + 1)
                    * (reset_time + h_time + self.code_cycle_offset)
                    + meas_time
                )
            else:
                code_cycle_time = (
                    max(reset_time, h_time)
                    + (self.distance + 1)
                    * (four_cz_time + h_time + self.code_cycle_offset)
                    + meas_time
                )
        time_value = code_cycle_time * self.distance

        # See Eqs. (10) and (11) in arXiv:1208.0928
        error_formula = linear_function(
            self.crossing_prefactor
            * (
                (physical_error_rate / self.error_correction_threshold)
                ** ((self.distance + 1) // 2)
            )
        )

        # We provide a generic lattice surgery instruction (See Section 3 in
        # arXiv:1111.4022)
        yield ctx.make_isa(
            ctx.add_instruction(
                LATTICE_SURGERY,
                encoding=LOGICAL,
                arity=None,
                space=space_formula,
                time=time_value,
                error_rate=error_formula,
                transform=self,
                source=[cz, rz, sqrt_x, reset, meas_z, move],
                distance=self.distance,
                code_cycle_time=code_cycle_time,
            ),
        )
