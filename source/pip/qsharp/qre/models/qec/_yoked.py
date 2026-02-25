# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import dataclass, KW_ONLY, field
from enum import IntEnum
from math import ceil
from typing import Generator

from ..._instruction import ISATransform, constraint, LOGICAL, PropertyKey, instruction
from ..._qre import ISA, ISARequirements, generic_function
from ..._architecture import _Context
from ...instruction_ids import LATTICE_SURGERY, MEMORY


class ShapeHeuristic(IntEnum):
    """
    The heuristic to determine the shape of the memory qubits with respect to
    the number of required rows and columns.

    Attributes:
        MIN_AREA: The shape that minimizes the total number of qubits.
        SQUARE: The shape that minimizes the difference between the number of rows
            and columns.
    """

    MIN_AREA = 0
    SQUARE = 1


@dataclass
class YokedSurfaceCode(ISATransform):
    """
    This class models the Yoked surface code to provide a generic memory
    instruction based on lattice surgery instructions from a surface code like
    error correction code.

    Attributes:
        crossing_prefactor: float
            The prefactor for logical error rate (Default is 0.016)
        error_correction_threshold: float
            The error correction threshold for the surface code (Default is
            0.064)

    Hyper parameters:
        shape_heuristic: ShapeHeuristic
            The heuristic to determine the shape of the surface code patch for a
            given number of logical qubits.  (Default is ShapeHeuristic.MIN_AREA)

    References:

    - Craig Gidney, Michael Newman, Peter Brooks, Cody Jones: Yoked surface
      codes, [arXiv:2312.04522](https://arxiv.org/abs/2312.04522)
    """

    crossing_prefactor: float = 0.016
    error_correction_threshold: float = 0.064
    _: KW_ONLY
    shape_heuristic: ShapeHeuristic = field(
        default=ShapeHeuristic.MIN_AREA, metadata={"domain": list(ShapeHeuristic)}
    )

    @staticmethod
    def required_isa() -> ISARequirements:
        # We require a lattice surgery instruction that also provides the code
        # distance as a property. This is necessary to compute the time
        # and error rate formulas for the provided memory instruction.
        return ISARequirements(
            constraint(LATTICE_SURGERY, LOGICAL, arity=None, distance=True),
        )

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        lattice_surgery = impl_isa[LATTICE_SURGERY]
        distance = lattice_surgery.get_property(PropertyKey.DISTANCE)
        assert distance is not None

        shape_fn = [self._min_area_shape, self._square_shape][self.shape_heuristic]

        def space(arity: int) -> int:
            a, b = shape_fn(arity)
            return lattice_surgery.expect_space(a * b)

        space_fn = generic_function(space)

        def time(arity: int) -> int:
            a, b = shape_fn(arity)
            s = lattice_surgery.expect_time(a * b)
            return s * (8 * distance * (a - 1) + 2 * distance)

        time_fn = generic_function(time)

        def error_rate(arity: int) -> float:
            a, b = shape_fn(arity)
            rounds = 2 * (a - 2)
            # logical error rate on a single surface code patch
            p = lattice_surgery.expect_error_rate(1)
            return (
                rounds**2
                * (a * b) ** 2
                * self.crossing_prefactor
                * (p / self.error_correction_threshold) ** ((distance + 1) // 2)
            )

        error_rate_fn = generic_function(error_rate)

        yield ISA(
            ctx.set_source(
                self,
                instruction(
                    MEMORY,
                    arity=None,
                    encoding=LOGICAL,
                    space=space_fn,
                    time=time_fn,
                    error_rate=error_rate_fn,
                    distance=distance,
                ),
                [lattice_surgery],
            )
        )

    @staticmethod
    def _square_shape(num_qubits: int) -> tuple[int, int]:
        """
        Given a number of qubits num_qubits, returns numbers (a + 1) and (b + 2)
        such that a * b >= num_qubits and a and b are as close as possible.
        """

        a = int(num_qubits**0.5)
        while num_qubits % a != 0:
            a -= 1
        b = num_qubits // a
        return a + 1, b + 2

    @staticmethod
    def _min_area_shape(num_qubits: int) -> tuple[int, int]:
        """
        Given a number of qubits num_qubits, returns numbers (a + 1) and (b + 2)
        such that a * b >= num_qubits and a * b is as small as possible.
        """

        best_a = None
        best_b = None
        best_qubits = num_qubits**2

        for a in range(1, num_qubits):
            # Compute required number of columns to reach the required number
            # of logical qubits
            b = ceil(num_qubits / a)

            qubits = (a + 1) * (b + 2)
            if qubits < best_qubits:
                best_qubits = qubits
                best_a = a
                best_b = b

        assert best_a is not None
        assert best_b is not None
        return best_a + 1, best_b + 2
