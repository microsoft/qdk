# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from __future__ import annotations

from dataclasses import dataclass
from math import ceil
from typing import Generator

from ..._architecture import _Context
from ..._qre import ISA, ISARequirements, ConstraintBound, _Instruction
from ..._instruction import ISATransform, constraint, instruction, LOGICAL
from ...instruction_ids import T, CNOT, H, MEAS_Z, CCZ


@dataclass
class Litinski19Factory(ISATransform):
    """
    T and CCZ factories based on the paper
    [arXiv:1905.06903](https://arxiv.org/abs/1905.06903).

    It contains two categories of estimates.  If the input T error rate is
    similar to the Clifford error, it produces magic state instructions based on
    Table 1 in the paper.  If the input T error rate is at most 10 times higher
    than the Clifford error rate, it produces magic state instructions based on
    Table 2 in the paper.

    It requires Clifford error rates of at most 0.1% for CNOT, H, and MEAS_Z
    instructions.  If these instructions have different error rates, the maximum
    error rate is assumed.

    References:

    - Daniel Litinski: Magic state distillation: not as costly as you think,
      [arXiv:1905.06903](https://arxiv.org/abs/1905.06903)
    """

    def __post_init__(self):
        self._initialize_entries()

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            # T error rate may be at least 10x higher than Clifford error rates
            constraint(T, error_rate=ConstraintBound.le(1e-2)),
            constraint(H, error_rate=ConstraintBound.le(1e-3)),
            constraint(CNOT, arity=2, error_rate=ConstraintBound.le(1e-3)),
            constraint(MEAS_Z, error_rate=ConstraintBound.le(1e-3)),
        )

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        h = impl_isa[H]
        cnot = impl_isa[CNOT]
        meas_z = impl_isa[MEAS_Z]
        t = impl_isa[T]

        clifford_error_rate = max(
            h.expect_error_rate(),
            cnot.expect_error_rate(),
            meas_z.expect_error_rate(),
        )

        t_error_rate = t.expect_error_rate()

        entries_by_state = None

        if clifford_error_rate <= 1e-4:
            if t_error_rate <= 1e-4:
                entries_by_state = self._entries[1e-4][0]
            elif t_error_rate <= 1e-3:
                entries_by_state = self._entries[1e-4][1]
        else:
            # NOTE: This assertion is valid due to the constraint bound in the
            # required_isa method
            assert clifford_error_rate <= 1e-3
            if t_error_rate <= 1e-3:
                entries_by_state = self._entries[1e-3][0]
            elif t_error_rate <= 1e-2:
                entries_by_state = self._entries[1e-3][1]

        if entries_by_state is None:
            return

        t_entries = entries_by_state.get(T, [])
        ccz_entries = entries_by_state.get(CCZ, [])

        syndrome_extraction_time = (
            4 * impl_isa[CNOT].expect_time()
            + impl_isa[H].expect_time()
            + impl_isa[MEAS_Z].expect_time()
        )

        def make_instruction(entry: _Entry) -> _Instruction:
            # Convert cycles (number of syndrome extraction cycles) to time
            # based on fast surface code
            time = ceil(syndrome_extraction_time * entry.cycles)

            # NOTE: If the protocol outputs multiple states, we assume that the
            # space cost is divided by the number of output states.  This is a
            # simplification that allows us to fit all protocols in the ISA, but
            # it may not be accurate for all protocols.
            inst = instruction(
                entry.state,
                arity=3 if entry.state == CCZ else 1,
                encoding=LOGICAL,
                space=ceil(entry.space / entry.output_states),
                time=time,
                error_rate=entry.error_rate,
            )
            return ctx.set_source(self, inst, [cnot, h, meas_z, t])

        # Yield combinations of T and CCZ entries
        if ccz_entries:
            for t_entry in t_entries:
                for ccz_entry in ccz_entries:
                    yield ISA(
                        make_instruction(t_entry),
                        make_instruction(ccz_entry),
                    )
        else:
            # Table 2 scenarios: only T gates available
            for t_entry in t_entries:
                yield ISA(make_instruction(t_entry))

    def _initialize_entries(self):
        self._entries = {
            # Assuming a Clifford error rate of at most 1e-4:
            1e-4: (
                # Assuming a T error rate of at most 1e-4 (Table 1):
                {
                    T: [
                        _Entry(_Protocol(15, 1, 7, 3, 3), 4.4e-8, 810, 18.1),
                        _Entry(_Protocol(15, 1, 9, 3, 3), 9.3e-10, 1_150, 18.1),
                        _Entry(_Protocol(15, 1, 11, 5, 5), 1.9e-11, 2_070, 30.0),
                        _Entry(
                            [
                                (_Protocol(15, 1, 9, 3, 3), 4),
                                (_Protocol(20, 4, 15, 7, 9), 1),
                            ],
                            2.4e-15,
                            16_400,
                            90.3,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 9, 3, 3), 4),
                                (_Protocol(15, 1, 25, 9, 9), 1),
                            ],
                            6.3e-25,
                            18_600,
                            67.8,
                        ),
                        _Entry(_Protocol(15, 1, 9, 3, 3), 1.5e-9, 762, 36.2),
                    ],
                    CCZ: [
                        _Entry(
                            [
                                (_Protocol(15, 1, 7, 3, 3), 4),
                                (_Protocol(8, 1, 15, 7, 9, CCZ), 1),
                            ],
                            7.2e-14,
                            12_400,
                            36.1,
                        ),
                    ],
                },
                # Assuming a T error rate of at most 1e-3 (10x higher than Clifford, Table 2):
                {
                    T: [
                        _Entry(_Protocol(15, 1, 9, 3, 3), 2.1e-8, 1_150, 18.2),
                        _Entry(
                            [
                                (_Protocol(15, 1, 7, 3, 3), 6),
                                (_Protocol(20, 4, 13, 5, 7), 1),
                            ],
                            1.4e-12,
                            13_200,
                            70,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 9, 3, 3), 4),
                                (_Protocol(20, 4, 15, 7, 9), 1),
                            ],
                            6.6e-15,
                            16_400,
                            91.2,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 9, 3, 3), 4),
                                (_Protocol(15, 1, 25, 9, 9), 1),
                            ],
                            4.2e-22,
                            18_600,
                            68.4,
                        ),
                    ],
                    CCZ: [],
                },
            ),
            # Assuming a Clifford error rate of at most 1e-3:
            1e-3: (
                # Assuming a T error rate of at most 1e-3 (Table 1):
                {
                    T: [
                        _Entry(_Protocol(15, 1, 17, 7, 7), 4.5e-8, 4_620, 42.6),
                        _Entry(
                            [
                                (_Protocol(15, 1, 13, 5, 5), 6),
                                (_Protocol(20, 4, 23, 11, 13), 1),
                            ],
                            1.4e-10,
                            43_300,
                            130,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 13, 5, 5), 4),
                                (_Protocol(20, 4, 27, 13, 15), 1),
                            ],
                            2.6e-11,
                            46_800,
                            157,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 11, 5, 5), 6),
                                (_Protocol(15, 1, 25, 11, 11), 1),
                            ],
                            2.7e-12,
                            30_700,
                            82.5,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 13, 5, 5), 6),
                                (_Protocol(15, 1, 29, 11, 13), 1),
                            ],
                            3.3e-14,
                            39_100,
                            97.5,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 15, 7, 7), 6),
                                (_Protocol(15, 1, 41, 17, 17), 1),
                            ],
                            4.5e-20,
                            73_400,
                            128,
                        ),
                    ],
                    CCZ: [
                        _Entry(
                            [
                                (_Protocol(15, 1, 13, 7, 7), 6),
                                (_Protocol(8, 1, 25, 15, 15, CCZ), 1),
                            ],
                            5.2e-11,
                            47_000,
                            60,
                        ),
                    ],
                },
                # Assuming a T error rate of at most 1e-2 (10x higher than Clifford, Table 2):
                {
                    T: [
                        _Entry(
                            [
                                (_Protocol(15, 1, 13, 5, 5), 6),
                                (_Protocol(20, 4, 21, 11, 13), 1),
                            ],
                            5.7e-9,
                            40_700,
                            130,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 11, 5, 5), 6),
                                (_Protocol(15, 1, 21, 9, 11), 1),
                            ],
                            2.1e-10,
                            27_400,
                            85.7,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 11, 5, 5), 6),
                                (_Protocol(15, 1, 23, 11, 11), 1),
                            ],
                            2.5e-11,
                            29_500,
                            85.7,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 11, 5, 5), 6),
                                (_Protocol(15, 1, 25, 11, 11), 1),
                            ],
                            6.4e-12,
                            30_700,
                            85.7,
                        ),
                        _Entry(
                            [
                                (_Protocol(15, 1, 13, 7, 7), 8),
                                (_Protocol(15, 1, 29, 13, 13), 1),
                            ],
                            1.5e-13,
                            52_400,
                            97.5,
                        ),
                    ],
                    CCZ: [],
                },
            ),
        }


@dataclass(frozen=True, slots=True)
class _Entry:
    protocol: list[tuple[_Protocol, int]] | _Protocol
    error_rate: float
    # Space estimation in number of physical qubits
    space: int
    # Number of code cycles to estimate time; a code cycle corresponds to
    # measuring all surface-code check operators exactly once.
    cycles: float

    @property
    def output_states(self) -> int:
        if isinstance(self.protocol, list):
            return self.protocol[-1][0].output_states
        else:
            return self.protocol.output_states

    @property
    def state(self) -> int:
        if isinstance(self.protocol, list):
            return self.protocol[-1][0].state
        else:
            return self.protocol.state


@dataclass(frozen=True, slots=True)
class _Protocol:
    # Number of input T states in protocol
    input_states: int
    # Number of output T states in protocol
    output_states: int
    # Spatial X distance (arXiv:1905.06903, Section 2)
    d_x: int
    # Spatial Z distance (arXiv:1905.06903, Section 2)
    d_z: int
    # Temporal distance (arXiv:1905.06903, Section 2)
    d_m: int
    # Magic state
    state: int = T
