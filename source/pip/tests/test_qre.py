# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import dataclass, KW_ONLY
from typing import Generator


from qsharp.qre import (
    constraint,
    ConstraintBound,
    ISA,
    ISARequirements,
    instruction,
    linear_function,
    LOGICAL,
)
from qsharp.qre.instruction_ids import (
    T,
    TWO_QUBIT_CLIFFORD,
    H,
    CNOT,
    MEAS_Z,
    GENERIC,
    LATTICE_SURGERY,
)


# NOTE These classes will be generalized as part of the QRE API in the following
# pull requests and then moved out of the tests.


class Architecture:
    @property
    def provided_isa(self) -> ISA:
        return ISA(
            instruction(H, time=50, error_rate=1e-3),
            instruction(CNOT, arity=2, time=50, error_rate=1e-3),
            instruction(MEAS_Z, time=100, error_rate=1e-3),
            instruction(T, time=40, error_rate=1e-4),
            instruction(TWO_QUBIT_CLIFFORD, arity=2, time=50, error_rate=1e-3),
        )


@dataclass
class SurfaceCode:
    _: KW_ONLY
    distance: int = 7

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(H, error_rate=ConstraintBound.lt(0.01)),
            constraint(CNOT, arity=2, error_rate=ConstraintBound.lt(0.01)),
            constraint(MEAS_Z, error_rate=ConstraintBound.lt(0.01)),
        )

    def provided_isa(self, impl_isa: ISA) -> Generator[ISA, None, None]:
        crossing_prefactor: float = 0.03
        error_correction_threshold: float = 0.01

        cnot_time = impl_isa[CNOT].expect_time()
        h_time = impl_isa[H].expect_time()
        meas_time = impl_isa[MEAS_Z].expect_time()

        physical_error_rate = max(
            impl_isa[CNOT].expect_error_rate(),
            impl_isa[H].expect_error_rate(),
            impl_isa[MEAS_Z].expect_error_rate(),
        )

        space_formula = linear_function(2 * self.distance**2)

        time_value = (h_time + meas_time + cnot_time * 4) * self.distance

        error_formula = linear_function(
            crossing_prefactor
            * (
                (physical_error_rate / error_correction_threshold)
                ** ((self.distance + 1) // 2)
            )
        )

        yield ISA(
            instruction(
                GENERIC,
                encoding=LOGICAL,
                arity=None,
                space=space_formula,
                time=time_value,
                error_rate=error_formula,
            ),
            instruction(
                LATTICE_SURGERY,
                encoding=LOGICAL,
                arity=None,
                space=space_formula,
                time=time_value,
                error_rate=error_formula,
            ),
        )


def test_isa_from_architecture():
    arch = Architecture()
    code = SurfaceCode()

    # Verify that the architecture satisfies the code requirements
    assert arch.provided_isa.satisfies(SurfaceCode.required_isa())

    # Generate logical ISAs
    isas = list(code.provided_isa(arch.provided_isa))

    # There is one ISA with two instructions
    assert len(isas) == 1
    assert len(isas[0]) == 2
