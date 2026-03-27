# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field
from typing import Generator

from qsharp.qre import (
    ISA,
    LOGICAL,
    ISARequirements,
    ISATransform,
    constraint,
)
from qsharp.qre._architecture import ISAContext
from qsharp.qre.instruction_ids import LATTICE_SURGERY, T


# NOTE These classes will be generalized as part of the QRE API in the following
# pull requests and then moved out of the tests.


@dataclass
class ExampleFactory(ISATransform):
    _: KW_ONLY
    level: int = field(default=1, metadata={"domain": range(1, 4)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(T),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        yield ctx.make_isa(
            ctx.add_instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-8),
        )


@dataclass
class ExampleLogicalFactory(ISATransform):
    _: KW_ONLY
    level: int = field(default=1, metadata={"domain": range(1, 4)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(LATTICE_SURGERY, encoding=LOGICAL),
            constraint(T, encoding=LOGICAL),
        )

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        yield ctx.make_isa(
            ctx.add_instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-10),
        )
