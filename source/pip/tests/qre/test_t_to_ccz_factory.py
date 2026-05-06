# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from math import ceil, isclose

from qsharp.qre import LOGICAL, linear_function
from qsharp.qre.instruction_ids import CCX, LATTICE_SURGERY, T
from qsharp.qre.models import GSJ24CCXFactory, GateBased


def _make_logical_input_isa(
    gate_time: int = 1_000,
    t_space: int = 31,
    t_error: float = 1e-6,
    lattice_surgery_space: int = 17,
    lattice_surgery_time: int = 350,
    lattice_surgery_error: float = 1e-10,
):
    ctx = GateBased(gate_time=50, measurement_time=100).context()
    isa = ctx.make_isa(
        ctx.add_instruction(
            T,
            encoding=LOGICAL,
            time=gate_time,
            space=t_space,
            error_rate=t_error,
        ),
        ctx.add_instruction(
            LATTICE_SURGERY,
            encoding=LOGICAL,
            arity=None,
            time=linear_function(lattice_surgery_time),
            space=linear_function(lattice_surgery_space),
            error_rate=linear_function(lattice_surgery_error),
        ),
    )
    return ctx, isa


def test_required_isa_matches_logical_t_and_lattice_surgery():
    ctx, impl_isa = _make_logical_input_isa()

    assert impl_isa.satisfies(GSJ24CCXFactory.required_isa())
    assert ctx.isa is not None


def test_provided_isa_produces_logical_ccx_with_expected_costs():
    ctx, impl_isa = _make_logical_input_isa()
    factory = GSJ24CCXFactory()

    isas = list(factory.provided_isa(impl_isa, ctx))

    assert len(isas) == 1
    assert len(isas[0]) == 1
    assert CCX in isas[0]

    ccx = isas[0][CCX]
    assert ccx.encoding == LOGICAL
    assert ccx.arity == 3

    num_logical_qubits = 12
    t_space = 31
    t_time = 1_000
    t_error = 1e-6
    lattice_surgery_space = 17
    lattice_surgery_time = 350
    lattice_surgery_error = 1e-10

    expected_space = lattice_surgery_space * num_logical_qubits
    expected_time = ceil(
        (t_space * t_time) * 8 / expected_space
        + (1 + 8 * t_error) * 6 * (lattice_surgery_time * num_logical_qubits)
    )
    expected_error = 28 * (t_error**2) + 6 * (
        lattice_surgery_error * num_logical_qubits
    )

    assert ccx.expect_space() == expected_space
    assert ccx.expect_time() == expected_time
    assert isclose(ccx.expect_error_rate(), expected_error)
