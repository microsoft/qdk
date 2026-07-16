# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import dataclass
from math import ceil
from typing import Generator

from ..._instruction import ISATransform, constraint, LOGICAL
from ..._qre import ISA, ISARequirements, generic_function
from ..._architecture import ISAContext
from ...instruction_ids import LATTICE_SURGERY, MEMORY
from ...property_keys import DISTANCE


@dataclass
class OneDimensionalYokedSurfaceCode(ISATransform):
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

    # NOTE: The crossing_prefactor is relative to that of the underlying surface
    # code. That is if the surface code model is p(SC) =
    # A*(p(phy)/th(SC))^((d+1)/2), then multiplier for its yoked extension is
    # crossing_prefactor*A
    crossing_prefactor: float = 8 / 15

    # NOTE: The threshold is relative to that of the underlying surface code.
    # Namely, as the yoking doubles the distance, one would expect the yoked
    # surface code to have a threshold of sqrt(th(SC)). However modeling shows
    # it falls short of this.
    error_correction_threshold: float = 64 / 10

    @staticmethod
    def required_isa() -> ISARequirements:
        # We require a lattice surgery instruction that also provides the code
        # distance as a property. This is necessary to compute the time
        # and error rate formulas for the provided memory instruction.
        return ISARequirements(
            constraint(LATTICE_SURGERY, LOGICAL, arity=None, distance=True),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        lattice_surgery = impl_isa[LATTICE_SURGERY]
        distance = lattice_surgery.get_property(DISTANCE)
        assert distance is not None

        def space(arity: int) -> int:
            a, b = self._min_area_shape(arity)
            return lattice_surgery.expect_space(a * b)

        space_fn = generic_function(space)

        def time(arity: int) -> int:
            a, b = self._min_area_shape(arity)
            s = lattice_surgery.expect_time(a * b)
            return s * (8 * distance * (a - 1) + 2 * distance)

        time_fn = generic_function(time)

        def error_rate(arity: int) -> float:
            a, b = self._min_area_shape(arity)
            rounds = 2 * (a - 2)
            # logical error rate on a single surface code patch
            p = lattice_surgery.expect_error_rate(1)
            return (
                rounds**2
                * (a * b) ** 2
                * self.crossing_prefactor
                * p
                * (1 / self.error_correction_threshold) ** ((distance + 1) // 2)
            )

        error_rate_fn = generic_function(error_rate)

        yield ctx.make_isa(
            ctx.add_instruction(
                MEMORY,
                arity=None,
                encoding=LOGICAL,
                space=space_fn,
                time=time_fn,
                error_rate=error_rate_fn,
                transform=self,
                source=[lattice_surgery],
                distance=distance,
            )
        )

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


@dataclass
class TwoDimensionalYokedSurfaceCode(ISATransform):
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

    # NOTE: The crossing_prefactor is relative to that of the underlying surface
    # code. That is if the surface code model is p(SC) =
    # A*(p(phy)/th(SC))^((d+1)/2), then multiplier for its yoked extension is
    # crossing_prefactor*A
    crossing_prefactor: float = 5 / 600

    # NOTE: The threshold is relative to that of the underlying surface code.
    # Namely, as the yoking doubles the distance, one would expect the yoked
    # surface code to have a threshold of sqrt(th(SC)). However modeling shows
    # it falls short of this.
    error_correction_threshold: float = 2500 / 10

    @staticmethod
    def required_isa() -> ISARequirements:
        # We require a lattice surgery instruction that also provides the code
        # distance as a property. This is necessary to compute the time
        # and error rate formulas for the provided memory instruction.
        return ISARequirements(
            constraint(LATTICE_SURGERY, LOGICAL, arity=None, distance=True),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        lattice_surgery = impl_isa[LATTICE_SURGERY]
        distance = lattice_surgery.get_property(DISTANCE)
        assert distance is not None

        def space(arity: int) -> int:
            a, b = self._square_shape(arity)
            return lattice_surgery.expect_space(a * b)

        space_fn = generic_function(space)

        def time(arity: int) -> int:
            a, b = self._square_shape(arity)
            s = lattice_surgery.expect_time(a * b)
            return s * (8 * distance * max(a - 2, b - 2) + 2 * distance)

        time_fn = generic_function(time)

        def error_rate(arity: int) -> float:
            a, b = self._square_shape(arity)
            rounds = 2 * max(a - 3, b - 3)
            # logical error rate on a single surface code patch
            p = lattice_surgery.expect_error_rate(1)
            return (
                rounds**4
                * (a * b) ** 2
                * self.crossing_prefactor
                * p
                * (1 / self.error_correction_threshold) ** ((distance + 1) // 2)
            )

        error_rate_fn = generic_function(error_rate)

        yield ctx.make_isa(
            ctx.add_instruction(
                MEMORY,
                arity=None,
                encoding=LOGICAL,
                space=space_fn,
                time=time_fn,
                error_rate=error_rate_fn,
                transform=self,
                source=[lattice_surgery],
                distance=distance,
            )
        )

    @staticmethod
    def _square_shape(num_qubits: int) -> tuple[int, int]:
        """
        Given a number of qubits num_qubits, returns numbers (a + 2) and (b + 2)
        such that a * b >= num_qubits and a and b are as close as possible.
        """

        a = int(num_qubits**0.5)
        while num_qubits % a != 0:
            a -= 1
        b = num_qubits // a
        return a + 2, b + 2
