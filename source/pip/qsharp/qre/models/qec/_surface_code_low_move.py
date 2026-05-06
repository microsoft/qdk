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
    ATOM_SPACING,
    SURFACE_CODE_ONE_QUBIT_TIME_FACTOR,
    SURFACE_CODE_TWO_QUBIT_TIME_FACTOR,
    VELOCITY,
    ACCELERATION,
)


@dataclass
class SurfaceCodeLowMove(ISATransform):
    """
    This class models a rotated surface code tailored to a reconfigurable,
    zoned neutral-atom architecture with mobile ancillas.

    The syndrome-extraction schedule is based on a mobile-ancilla surface-code
    scheme in which a single ancilla visits the data qubits of each plaquette,
    combined with the atom-transport model used by ``NeutralAtom``. In this
    model, the ancilla is moved within the Rydberg interaction range of each
    data atom to execute the entangling sequence, while other atoms and gate
    sites remain separated by about 10 microns to suppress crosstalk. The time
    model therefore combines the single-ancilla plaquette circuit with explicit
    motion overhead from horizontal and diagonal transport segments.

    Attributes:
        crossing_prefactor: float
            The prefactor for logical error rate due to error correction
            crossings.  (Default is 0.03, see Eq. (11) in
            [arXiv:1208.0928](https://arxiv.org/abs/1208.0928))
        error_correction_threshold: float
            The error correction threshold for the surface code.  (Default is
            0.01 (1%), see [arXiv:1009.3686](https://arxiv.org/abs/1009.3686))
        code_cycle_override: Optional[int]
            If provided, this value will be used as the time for each syndrome
            extraction cycle instead of the default calculation based on gate
            times and transport overhead. (Default is None)
        code_cycle_offset: int
            An additional time offset to add to the syndrome extraction cycle
            time. (Default is 0)

    Hyper parameters:
        distance: int
            The code distance of the surface code.

    References:

        - D. S. Wang, A. G. Fowler, L. C. L. Hollenberg: Quantum computing with
            nearest neighbor interactions and error rates over 1%,
            [arXiv:1009.3686](https://arxiv.org/abs/1009.3686)
        - D. Horsman, A. G. Fowler, S. Devitt, R. Van Meter: Surface code quantum
            computing by lattice surgery,
            [arXiv:1111.4022](https://arxiv.org/abs/1111.4022)
        - A. G. Fowler, M. Mariantoni, J. M. Martinis, A. N. Cleland: Surface
            codes: Towards practical large-scale quantum computation,
            [arXiv:1208.0928](https://arxiv.org/abs/1208.0928)
        - D. Bluvstein, H. Levine, G. Semeghini, et al.: A quantum processor based
            on coherent transport of entangled atom arrays,
            [arXiv:2112.03923](https://arxiv.org/abs/2112.03923)
        - D. Bluvstein, S. J. Evered, A. A. Geim, et al.: Logical quantum
            processor based on reconfigurable atom arrays,
            [arXiv:2312.03982](https://arxiv.org/abs/2312.03982)
        - S. Jandura, L. Pecorari, G. Pupillo: Surface Code Stabilizer
            Measurements for Rydberg Atoms,
            [arXiv:2405.16621](https://arxiv.org/abs/2405.16621)
        - W.-H. Lin, D. B. Tan, J. Cong: Reuse-Aware Compilation for Zoned Quantum
            Architectures Based on Neutral Atoms,
            [arXiv:2411.11784](https://arxiv.org/abs/2411.11784)
        - D. Bluvstein, A. A. Geim, S. H. Li, et al.: Architectural mechanisms of
            a universal fault-tolerant quantum computer,
            [arXiv:2506.20661](https://arxiv.org/abs/2506.20661)
    """

    crossing_prefactor: float = 0.03
    error_correction_threshold: float = 0.01
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
            constraint(
                PHYSICAL_MOVE,
                error_rate=ConstraintBound.lt(0.01),
                atom_spacing=ConstraintBound.gt(9.9),
            ),
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
        atom_spacing_prop = move.get_property_or(ATOM_SPACING, 10.0)
        max_vel_prop = move.get_property_or(VELOCITY, 0.25)
        max_accel_prop = move.get_property_or(ACCELERATION, 5000.0)
        assert isinstance(atom_spacing_prop, (int, float))
        assert isinstance(max_vel_prop, (int, float))
        assert isinstance(max_accel_prop, (int, float))
        atom_spacing = float(atom_spacing_prop) * 1e-6  # Convert from microns to meters
        max_vel = float(max_vel_prop)
        max_accel = float(max_accel_prop)
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
        move_time = 3 * move.expect_time() + 1e9 * (2 * hor_seg_time + diag_seg_time)

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
