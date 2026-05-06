# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field

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
from ...property_keys import ACCELERATION, ATOM_SPACING, VELOCITY


@dataclass
class NeutralAtom(Architecture):
    """A movement-aware neutral-atom architecture."""

    _: KW_ONLY
    rydberg_time: int = field(default=500)  # In units of ns.
    rydberg_error: float = field(default=1e-3)
    one_qubit_time: int = field(default=1000)  # In units of ns.
    one_qubit_error: float = field(default=1e-4)
    measurement_time: int = field(default=10000)  # In units of ns.
    measurement_error: float = field(default=1e-4)
    handoff_time: int = field(default=0)  # In units of ns.
    atom_spacing: int = field(default=3)  # In units of microns.
    max_velocity: int = field(default=1)  # In units m/s.
    max_acceleration: int = field(default=5000)  # In units m/s^2.

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
                time=0,
                error_rate=0.00001,
            ),
            ctx.add_instruction(
                SQRT_X,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.one_qubit_time,
                error_rate=self.one_qubit_error,
            ),
            ctx.add_instruction(
                H,
                encoding=Encoding.PHYSICAL,
                arity=1,
                time=self.one_qubit_time,
                error_rate=self.one_qubit_error,
            ),
            ctx.add_instruction(
                CZ,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=self.rydberg_time,
                error_rate=self.rydberg_error,
            ),
            ctx.add_instruction(
                CNOT,
                encoding=Encoding.PHYSICAL,
                arity=2,
                time=self.rydberg_time + 2 * self.one_qubit_time,
                error_rate=self.rydberg_error,
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
                acceleration=self.max_acceleration,
                atom_spacing=self.atom_spacing,
                velocity=self.max_velocity,
            ),
        )


__all__ = ["NeutralAtom"]
