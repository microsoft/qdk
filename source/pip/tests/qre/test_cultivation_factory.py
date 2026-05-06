# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from math import ceil, isclose

from qsharp.qre import LOGICAL, PHYSICAL
from qsharp.qre.instruction_ids import CNOT, T
from qsharp.qre.models import GSJ24Factory, GateBased


def test_required_isa_matches_gate_based_physical_inputs():
    ctx = GateBased(gate_time=50, measurement_time=100).context()

    assert ctx.isa.satisfies(GSJ24Factory.required_isa())


def test_provided_isa_produces_expected_logical_t_entries():
    ctx = GateBased(gate_time=50, measurement_time=100).context()
    factory = GSJ24Factory()

    isas = list(factory.provided_isa(ctx.isa, ctx))

    assert len(isas) == 4

    first_t = isas[0][T]
    assert first_t.encoding == LOGICAL
    assert first_t.arity == 1
    assert first_t.expect_space() == 454
    assert first_t.expect_time() == ceil(50 * 4 * (4433.630050313343 / 454))
    assert isclose(first_t.expect_error_rate(), 2.9973593146121454e-07)

    last_t = isas[-1][T]
    assert last_t.encoding == LOGICAL
    assert last_t.arity == 1
    assert last_t.expect_space() == 454
    assert last_t.expect_time() == ceil(50 * 4 * (3199.812603871103 / 454))
    assert isclose(last_t.expect_error_rate(), 1.292533506995642e-05)


def test_passthrough_keeps_physical_isa_alongside_logical_t_entries():
    ctx = GateBased(gate_time=50, measurement_time=100).context()
    factory = GSJ24Factory(passthrough=True)

    isa = list(factory.provided_isa(ctx.isa, ctx))[0]

    assert CNOT in isa
    assert isa[CNOT].encoding == PHYSICAL
    assert T in isa
    assert isa[T].encoding == LOGICAL
