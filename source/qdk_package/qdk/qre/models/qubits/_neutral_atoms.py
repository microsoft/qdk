# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass
from typing import Optional

from ..._qre import _float_to_bits
from ..._architecture import Architecture, ISAContext
from ..._instruction import ISA, Encoding
from ...instruction_ids import (
    CZ,
    MEAS_RESET_Z,
    MEAS_Z,
    PHYSICAL_MOVE,
    RZ,
    SQRT_X,
    H,
    CNOT,
    T,
)


@dataclass
class NeutralAtom(Architecture):
    """
    A movement-aware neutral-atom architecture with explicit atom transport.

    This model captures a neutral-atom device with native single-qubit
    operations, Rydberg-mediated entangling gates, Z-basis measurement, and a
    physical move instruction that carries hardware motion constraints. The
    instruction set includes free virtual ``RZ`` rotations, single-qubit
    ``SQRT_X`` and ``H`` gates, ``CZ`` as the native two-qubit interaction,
    ``CNOT`` with a duration derived from one Rydberg interaction plus two
    single-qubit operations, and ``MEAS_Z``/``MEAS_RESET_Z`` for readout.

    The motion model is exposed through ``PHYSICAL_MOVE`` and parameterized by
    atom spacing, maximum velocity, maximum acceleration, and an optional
    handoff time used when atoms enter or leave an interaction or measurement
    zone.

    Args:
        rydberg_time: The time (in ns) for native Rydberg-mediated two-qubit
            interactions.
        rydberg_error: The error rate for native two-qubit interactions.
        one_qubit_time: The time (in ns) for one-qubit physical gates such as
            ``SQRT_X`` and ``H``.
        one_qubit_error: The error rate for one-qubit physical gates.
        measurement_time: The time (in ns) for ``MEAS_Z`` and
            ``MEAS_RESET_Z`` operations.
        measurement_error: The error rate for measurement and measurement-reset
            operations.
        handoff_time: The time (in ns) for each handoff at the boundary of a
            move operation. The ``PHYSICAL_MOVE`` instruction duration is
            modeled as twice this value.
        atom_spacing: The nominal spacing (in microns) between atoms during
            transport or placement (based on atoms being in storage).
        data_qubit_spacing: The nominal spacing (in microns) between data qubits
            during transport or placement.
        max_velocity: The maximum atom transport velocity (in m/s).
        max_acceleration: The maximum atom transport acceleration (in m/s^2).
        surface_code_two_qubit_time_factor: A factor by which to multiply the time of
            two-qubit gates when performing syndrome extraction in a surface code.
        surface_code_one_qubit_time_factor: A factor by which to multiply the time of
            one-qubit gates when performing syndrome extraction in a surface code.
        target_year: If set, this target year is assigned to the 2-qubit gates
            in the provided ISA.  This can be used by transforms that select
            gates based on target year.

    References:

    - M. Saffman, T. G. Walker, K. Molmer: Quantum information with Rydberg
        atoms,
        [arXiv:0909.4777](https://arxiv.org/abs/0909.4777)
    - H. Bernien, S. Schwartz, A. Keesling, et al.: Probing many-body
        dynamics on a 51-atom quantum simulator,
        [arXiv:1707.04344](https://arxiv.org/abs/1707.04344)
    - D. Bluvstein, H. Levine, G. Semeghini, et al.: A quantum processor
        based on coherent transport of entangled atom arrays,
        [arXiv:2112.03923](https://arxiv.org/abs/2112.03923)
    - W. Tian, W. J. Wee, A. Qu, et al.: Parallel assembly of arbitrary
        defect-free atom arrays with a multi-tweezer algorithm,
        [arXiv:2209.08038](https://arxiv.org/abs/2209.08038)
    - S. J. Evered, D. Bluvstein, M. Kalinowski, et al.: High-fidelity
        parallel entangling gates on a neutral atom quantum computer,
        [arXiv:2304.05420](https://arxiv.org/abs/2304.05420)
    - K. Wintersperger, F. Dommert, T. Ehmer, et al.: Neutral atom quantum
        computing hardware: performance and end-user perspective,
        [arXiv:2304.14360](https://arxiv.org/abs/2304.14360)
    - H. Wang, P. Liu, D. B. Tan, et al.: Atomique: A Quantum Compiler for
        Reconfigurable Neutral Atom Arrays,
        [arXiv:2311.15123](https://arxiv.org/abs/2311.15123)
    - D. Bluvstein, S. J. Evered, A. A. Geim, et al.: Logical quantum
        processor based on reconfigurable atom arrays,
        [arXiv:2312.03982](https://arxiv.org/abs/2312.03982)
    - W.-H. Lin, D. B. Tan, J. Cong: Reuse-Aware Compilation for Zoned
        Quantum Architectures Based on Neutral Atoms,
        [arXiv:2411.11784](https://arxiv.org/abs/2411.11784)
    - O. Savola, A. Paler: ATLAS: Efficient Atom Rearrangement for
        Defect-Free Neutral-Atom Quantum Arrays Under Transport Loss,
        [arXiv:2511.16303](https://arxiv.org/abs/2511.16303)
    """

    _: KW_ONLY
    rydberg_time: int = 500  # In units of ns.
    rydberg_error: float = 1e-3
    one_qubit_time: int = 1000  # In units of ns.
    one_qubit_error: float = 1e-4
    measurement_time: int = 10_000  # In units of ns.
    measurement_error: float = 1e-4
    handoff_time: int = 0  # In units of ns.
    # These transport defaults are optimistic representative values and are
    # not intended to model any specific neutral-atom platform.
    atom_spacing: float = 3.0  # In units of microns.
    data_qubit_spacing: float = 12.0  # In units of microns
    max_velocity: float = 0.25  # In units m/s.
    max_acceleration: float = 5000.0  # In units m/s^2.
    # These properties can modify syndrome measurement in surface codes by
    # assumining a larger depth required to perform 2-qubit and 1-qubit gates.
    surface_code_two_qubit_time_factor: int = 1
    surface_code_one_qubit_time_factor: int = 1
    # If set, this target_year is assigned to the 2-qubit gates
    target_year: Optional[int] = None

    def provided_isa(self, ctx: ISAContext) -> ISA:
        return ctx.make_isa(
            ctx.add_instruction(
                RZ,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=0,
                error_rate=0.0,
            ),
            ctx.add_instruction(
                T,
                encoding=Encoding.PHYSICAL,
                arity=1,
                # NOTE: We assume a time of 0, however, some transforms may use
                # the time in arithmetic expressions, which require its value to
                # be non-zero.  Setting it to 1 leads to no or at most
                # negligible contributions to the overall resource estimates.
                time=1,
                error_rate=0.00001,
            ),
            ctx.add_instruction(
                SQRT_X,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.one_qubit_time,
                error_rate=self.one_qubit_error,
                surface_code_one_qubit_time_factor=self.surface_code_one_qubit_time_factor,
            ),
            ctx.add_instruction(
                H,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.one_qubit_time,
                error_rate=self.one_qubit_error,
                surface_code_one_qubit_time_factor=self.surface_code_one_qubit_time_factor,
            ),
            ctx.add_instruction(
                CZ,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=self.rydberg_time,
                error_rate=self.rydberg_error,
                surface_code_two_qubit_time_factor=self.surface_code_two_qubit_time_factor,
                target_year=self.target_year,
            ),
            ctx.add_instruction(
                CNOT,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=self.rydberg_time + 2 * self.one_qubit_time,
                error_rate=self.rydberg_error,
                surface_code_two_qubit_time_factor=self.surface_code_two_qubit_time_factor,
                target_year=self.target_year,
            ),
            ctx.add_instruction(
                MEAS_Z,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.measurement_time,
                error_rate=self.measurement_error,
            ),
            ctx.add_instruction(
                MEAS_RESET_Z,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.measurement_time,
                error_rate=self.measurement_error,
            ),
            ctx.add_instruction(
                PHYSICAL_MOVE,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=2 * self.handoff_time,
                error_rate=1e-4,
                acceleration=_float_to_bits(self.max_acceleration),
                atom_spacing=_float_to_bits(self.atom_spacing),
                data_qubit_spacing=_float_to_bits(self.data_qubit_spacing),
                velocity=_float_to_bits(self.max_velocity),
            ),
        )


__all__ = ["NeutralAtom"]
