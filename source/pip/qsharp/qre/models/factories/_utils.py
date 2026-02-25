# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Generator

from ..._architecture import _Context
from ..._qre import ISARequirements, ISA
from ..._instruction import ISATransform
from ...instruction_ids import (
    SQRT_SQRT_X,
    SQRT_SQRT_X_DAG,
    SQRT_SQRT_Y,
    SQRT_SQRT_Y_DAG,
    SQRT_SQRT_Z,
    SQRT_SQRT_Z_DAG,
    CCX,
    CCY,
    CCZ,
)


class MagicUpToClifford(ISATransform):
    """
    An ISA transform that adds Clifford equivalent representations of magic
    states.  For example, if the input ISA contains a T gate, the provided ISA
    will also contain `SQRT_SQRT_X`, `SQRT_SQRT_X_DAG`, `SQRT_SQRT_Y`,
    `SQRT_SQRT_Y_DAG`, and `T_DAG`.  The same is applied for `CCZ` gates and
    their Clifford equivalents.

    Example:

    .. code-block:: python
        app = SomeApplication()
        arch = SomeArchitecture()

        # This will contain CCX states
        trace_query = PSSPC.q(ccx_magic_states=True) * LatticeSurgery.q()

        # This will contain CCZ states
        isa_query = SurfaceCode.q() * Litinski19Factory.q()

        # There will be no results from the estimation because there is no
        # instruction to support CCX magic states in the query
        results = estimate(app, arch, isa_query, trace_query)
        assert len(results) == 0

        # We solve this by wrapping the Litinski19Factory with the
        # MagicUpToClifford transform, which transforms the CCZ states in the
        # provided ISA into CCX states.
        isa_query = SurfaceCode.q() * MagicUpToClifford.q(source=Litinski19Factory.q())

        # Now we will get results
        results = estimate(app, arch, isa_query, trace_query)
        assert len(results) != 0
    """

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements()

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        # Families of equivalent gates under Clifford conjugation.
        families = [
            [
                SQRT_SQRT_X,
                SQRT_SQRT_X_DAG,
                SQRT_SQRT_Y,
                SQRT_SQRT_Y_DAG,
                SQRT_SQRT_Z,
                SQRT_SQRT_Z_DAG,
            ],
            [CCX, CCY, CCZ],
        ]

        # For each family, if any member of the family is present in the input ISA, add all members of the family to the provided ISA.
        for family in families:
            for id in family:
                if id in impl_isa:
                    instr = impl_isa[id]
                    for equivalent_id in family:
                        if equivalent_id != id:
                            impl_isa.append(
                                ctx.set_source(
                                    self, instr.with_id(equivalent_id), [instr]
                                )
                            )
                    break  # Check next family

        yield impl_isa
