# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from dataclasses import dataclass
from math import isfinite
from typing import Generator

from ..._architecture import ISAContext
from ..._qre import ISARequirements, ISA
from ..._instruction import ISATransform, LOGICAL
from ...instruction_ids import T


@dataclass
class BlackBoxFactory(ISATransform):
    """A T-state factory with user-supplied physical qubits, time (ns), and error rate.

    Space and time must be positive integers; error rate must be finite and
    between zero and one.
    """

    space: int
    time: int
    error_rate: float

    def __post_init__(self) -> None:
        if (
            not isinstance(self.space, int)
            or isinstance(self.space, bool)
            or self.space <= 0
        ):
            raise ValueError("space must be a positive integer")
        if (
            not isinstance(self.time, int)
            or isinstance(self.time, bool)
            or self.time <= 0
        ):
            raise ValueError("time must be a positive integer")
        if (
            not isinstance(self.error_rate, (int, float))
            or isinstance(self.error_rate, bool)
            or not isfinite(self.error_rate)
            or not 0 <= self.error_rate <= 1
        ):
            raise ValueError("error_rate must be a finite number between 0 and 1")

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements()

    def provided_isa(
        self, impl_isa: ISA, ctx: ISAContext
    ) -> Generator[ISA, None, None]:
        yield ctx.make_isa(
            ctx.add_instruction(
                T,
                arity=1,
                encoding=LOGICAL,
                length=None,
                space=self.space,
                time=self.time,
                error_rate=self.error_rate,
                transform=self,
                source=[],
            )
        )
