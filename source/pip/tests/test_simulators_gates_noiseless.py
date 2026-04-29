# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from collections import Counter
import os
import pytest
import qdk
from qdk import compile, Result, TargetProfile
from qdk._simulation import (
    GpuSimulator,
    run_qir as _run_qir,
    NoiseConfig,
    try_create_gpu_adapter,
)
from typing import Literal, List, Optional, TypeAlias


@pytest.fixture(autouse=True, scope="module")
def _init_base_profile():
    """
    Initialize the Q# interpreter once per module.

    We need a pytest.fixture instead of just a global statement
    because global statements are evaluated at test-collection time,
    which means this file would inherit the interpreter state of
    another file.
    """
    qdk.init(target_profile=TargetProfile.Base)


SEED = 42

try:
    try_create_gpu_adapter()
    gpu_sim = GpuSimulator()
except Exception:
    pass


# ---------------------------------------------------------------------------
# Simulator-type parametrization
# ---------------------------------------------------------------------------


def gpu_param():
    skip_reason = ""
    try:
        try_create_gpu_adapter()
        if not os.environ.get("QDK_GPU_TESTS"):
            skip_reason = "Env variable QDK_GPU_TESTS is not set"
    except Exception:
        skip_reason = "No GPU available"

    return pytest.param(
        "gpu",
        marks=pytest.mark.skipif(bool(skip_reason), reason=skip_reason),
    )


SIM_TYPES = ["cpu", "clifford", gpu_param()]
NON_CLIFFORD_SIM_TYPES = ["cpu", gpu_param()]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


SimType: TypeAlias = Literal["clifford", "cpu", "gpu"]


def str_to_result(str):
    return [Result.One if c == "1" else Result.Zero for c in str]


def result_list_to_str(result_list):
    if isinstance(result_list, (list, tuple)):
        return "".join("1" if r == Result.One else "0" for r in result_list)
    return "1" if result_list == Result.One else "0"


def run_qir(
    input,
    shots: int,
    noise: Optional[NoiseConfig],
    seed: Optional[int],
    type: SimType,
) -> List:
    global gpu_sim
    if type == "gpu":
        gpu_sim.set_program(input)
        return gpu_sim.run_shots(shots, seed)["shot_results"]
    else:
        results = _run_qir(input, shots, noise, seed, type)
        return [result_list_to_str(r) for r in results]


def compile_and_run(
    source,
    shots=1,
    noise=None,
    seed=None,
    sim_type: SimType = "cpu",
):
    """Compile a Q# expression and run it through run_qir."""
    qir = compile(source)
    return run_qir(qir, shots=shots, noise=noise, seed=seed, type=sim_type)


def check_qsharp(
    source,
    expected,
    *,
    shots=1,
    noise=None,
    seed=None,
    sim_type: SimType = "cpu",
):
    """
    Compile *source*, run it, and assert the result list equals *expected*.
    *expected* should be a list of shot results (each shot is a single
    value or a tuple/list of Result values).
    """
    results = compile_and_run(
        source, shots=shots, noise=noise, seed=seed, sim_type=sim_type
    )
    assert results == expected, f"Expected {expected}, got {results}"


def check_programs_are_eq(programs, num_qubits, sim_type: SimType, shots=1):
    """
    Verify that all *programs* (list of Q# operation-body strings) produce
    the same measurement outcomes on every computational basis state.

    For each basis state |b⟩ (0 .. 2^n - 1), we:
      1. Prepare the state with X gates on the appropriate qubits.
      2. Apply the program body.
      3. Measure every qubit with MResetZ.
      4. Run many shots and compare the outcome distributions.
    """
    seed = SEED
    for basis in range(1 << num_qubits):
        prep = "".join(f"X(qs[{q}]);" for q in range(num_qubits) if (basis >> q) & 1)
        measure = "[" + ", ".join(f"MResetZ(qs[{q}])" for q in range(num_qubits)) + "]"
        distributions = []
        for body in programs:
            source = (
                "{"
                f"use qs = Qubit[{num_qubits}];"
                f"{prep}"
                f"{body}"
                f"{measure}"
                "}"
            )
            results = compile_and_run(source, shots=shots, seed=seed, sim_type=sim_type)
            dist = Counter(results)
            distributions.append(dist)
        for i in range(1, len(distributions)):
            all_keys = set(distributions[0].keys()) | set(distributions[i].keys())
            for key in all_keys:
                p0 = distributions[0].get(key, 0) / shots
                pi = distributions[i].get(key, 0) / shots
                assert abs(p0 - pi) <= 0.15, (
                    f"Programs differ on basis |{basis:0{num_qubits}b}⟩ "
                    f"for outcome {key}: "
                    f"program 0 = {p0:.2f}, program {i} = {pi:.2f}"
                )


def check_basis_table(num_qubits, table, sim_type: SimType = "cpu"):
    """
    Verify a truth-table of gate mappings.

    *table* is a list of ``(body, input_bits, expected_bits)`` where *body*
    is a Q# statement string to apply, and the bit values encode qubit
    states (bit i → qubit i).
    """
    for body, inp, exp in table:
        prep_actual = "".join(
            f"X(qs[{q}]);" for q in range(num_qubits) if (inp >> q) & 1
        )
        measure = "[" + ", ".join(f"MResetZ(qs[{q}])" for q in range(num_qubits)) + "]"
        src_actual = (
            "{"
            f"use qs = Qubit[{num_qubits}];"
            f"{prep_actual}"
            f"{body}"
            f"{measure}"
            "}"
        )
        r_actual = compile_and_run(src_actual, sim_type=sim_type)
        r_expected = [
            "".join("1" if (exp >> q) & 1 else "0" for q in range(num_qubits))
        ]
        assert r_actual == r_expected, (
            f"Basis table mismatch for '{body}' on input "
            f"|{inp:0{num_qubits}b}⟩: got {r_actual}, expected {r_expected}"
        )


# ===========================================================================
# Generic simulator tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_simulator_completes_all_shots(sim_type):
    results = compile_and_run(
        "{use qs = Qubit[1]; X(qs[0]); MResetEachZ(qs)}",
        shots=10,
        sim_type=sim_type,
    )
    assert len(results) == 10
    assert all(r == "1" for r in results)


# ===========================================================================
# Gate truth-table tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_single_qubit_gate_truth_tables(sim_type):
    # fmt: off
    table = [
        # I gate: identity
        ("I(qs[0]);",          0b0, 0b0),
        ("I(qs[0]);",          0b1, 0b1),
        # X gate: bit flip
        ("X(qs[0]);",          0b0, 0b1),
        ("X(qs[0]);",          0b1, 0b0),
        # Y gate: bit flip (phase differs but same basis state)
        ("Y(qs[0]);",          0b0, 0b1),
        ("Y(qs[0]);",          0b1, 0b0),
        # Z gate: phase only, no bit change
        ("Z(qs[0]);",          0b0, 0b0),
        ("Z(qs[0]);",          0b1, 0b1),
        # Z within H basis: acts like X
        ("H(qs[0]); Z(qs[0]); H(qs[0]);", 0b0, 0b1),
        ("H(qs[0]); Z(qs[0]); H(qs[0]);", 0b1, 0b0),
        # S gate: phase only
        ("S(qs[0]);",          0b0, 0b0),
        ("S(qs[0]);",          0b1, 0b1),
        # Adjoint S gate: phase only
        ("Adjoint S(qs[0]);",  0b0, 0b0),
        ("Adjoint S(qs[0]);",  0b1, 0b1),
    ]
    # fmt: on
    check_basis_table(1, table, sim_type)


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_single_qubit_non_clifford_gate_truth_tables(sim_type):
    # fmt: off
    table = [
        # T gate: phase only
        ("T(qs[0]);",          0b0, 0b0),
        ("T(qs[0]);",          0b1, 0b1),
        # Adjoint T gate: phase only
        ("Adjoint T(qs[0]);",  0b0, 0b0),
        ("Adjoint T(qs[0]);",  0b1, 0b1),
    ]
    # fmt: on
    check_basis_table(1, table, sim_type)


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_two_qubit_gate_truth_tables(sim_type):
    # fmt: off
    table = [
        # CX(control=q0, target=q1): flips q1 when q0=|1⟩
        ("CNOT(qs[0], qs[1]);", 0b00, 0b00),
        ("CNOT(qs[0], qs[1]);", 0b01, 0b11),
        ("CNOT(qs[0], qs[1]);", 0b10, 0b10),
        ("CNOT(qs[0], qs[1]);", 0b11, 0b01),
        # CZ gate: phase only, no bit changes
        ("CZ(qs[0], qs[1]);", 0b00, 0b00),
        ("CZ(qs[0], qs[1]);", 0b01, 0b01),
        ("CZ(qs[0], qs[1]);", 0b10, 0b10),
        ("CZ(qs[0], qs[1]);", 0b11, 0b11),
        # CZ within H on target: acts like CX
        ("H(qs[1]); CZ(qs[0], qs[1]); H(qs[1]);", 0b00, 0b00),
        ("H(qs[1]); CZ(qs[0], qs[1]); H(qs[1]);", 0b01, 0b11),
        ("H(qs[1]); CZ(qs[0], qs[1]); H(qs[1]);", 0b10, 0b10),
        ("H(qs[1]); CZ(qs[0], qs[1]); H(qs[1]);", 0b11, 0b01),
        # SWAP gate: exchanges qubit states
        ("SWAP(qs[0], qs[1]);", 0b00, 0b00),
        ("SWAP(qs[0], qs[1]);", 0b01, 0b10),
        ("SWAP(qs[0], qs[1]);", 0b10, 0b01),
        ("SWAP(qs[0], qs[1]);", 0b11, 0b11),
    ]
    # fmt: on
    check_basis_table(2, table, sim_type)


# ===========================================================================
# Single-qubit gate equivalence tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_x_is_self_adjoint(sim_type):
    check_programs_are_eq(
        ["", "X(qs[0]); X(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_x_eq_h_z_h(sim_type):
    check_programs_are_eq(
        ["X(qs[0]);", "H(qs[0]); Z(qs[0]); H(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_y_is_self_adjoint(sim_type):
    check_programs_are_eq(
        ["", "Y(qs[0]); Y(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_y_gate_eq_x_z_and_z_x(sim_type):
    check_programs_are_eq(
        ["Y(qs[0]);", "X(qs[0]); Z(qs[0]);", "Z(qs[0]); X(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_z_is_self_adjoint(sim_type):
    check_programs_are_eq(
        [
            "",
            "H(qs[0]); Z(qs[0]); Z(qs[0]); H(qs[0]);",
        ],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_z_eq_h_x_h(sim_type):
    check_programs_are_eq(
        ["Z(qs[0]);", "H(qs[0]); X(qs[0]); H(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_h_gate_creates_superposition(sim_type):
    results = compile_and_run(
        "{use q = Qubit(); H(q); MResetZ(q)}",
        shots=100,
        seed=SEED,
        sim_type=sim_type,
    )
    outcomes = set(results)
    assert "0" in outcomes and "1" in outcomes


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_h_is_self_adjoint(sim_type):
    check_programs_are_eq(
        ["", "H(qs[0]); H(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


# --- S gate ---


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_s_squared_eq_z(sim_type):
    check_programs_are_eq(
        ["Z(qs[0]);", "S(qs[0]); S(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_s_and_s_adj_cancel(sim_type):
    check_programs_are_eq(
        ["", "S(qs[0]); Adjoint S(qs[0]);", "Adjoint S(qs[0]); S(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_s_adj_squared_eq_z(sim_type):
    check_programs_are_eq(
        ["Z(qs[0]);", "Adjoint S(qs[0]); Adjoint S(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


# --- SX gate (expressed as Rx(π/2)) ---


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_sx_squared_eq_x(sim_type):
    check_programs_are_eq(
        [
            "X(qs[0]);",
            "Rx(Std.Math.PI() / 2.0, qs[0]); Rx(Std.Math.PI() / 2.0, qs[0]);",
        ],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_sx_and_sx_adj_cancel(sim_type):
    check_programs_are_eq(
        [
            "",
            "Rx(Std.Math.PI() / 2.0, qs[0]); Rx(-Std.Math.PI() / 2.0, qs[0]);",
            "Rx(-Std.Math.PI() / 2.0, qs[0]); Rx(Std.Math.PI() / 2.0, qs[0]);",
        ],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_sx_adj_squared_eq_x(sim_type):
    check_programs_are_eq(
        [
            "X(qs[0]);",
            "Rx(-Std.Math.PI() / 2.0, qs[0]); Rx(-Std.Math.PI() / 2.0, qs[0]);",
        ],
        num_qubits=1,
        sim_type=sim_type,
    )


# --- T gate ---


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_t_fourth_eq_z(sim_type):
    check_programs_are_eq(
        ["Z(qs[0]);", "T(qs[0]); T(qs[0]); T(qs[0]); T(qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_t_and_t_adj_cancel(sim_type):
    check_programs_are_eq(
        [
            "",
            "T(qs[0]); Adjoint T(qs[0]);",
            "Adjoint T(qs[0]); T(qs[0]);",
        ],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_t_adj_fourth_eq_z(sim_type):
    check_programs_are_eq(
        [
            "Z(qs[0]);",
            "Adjoint T(qs[0]); Adjoint T(qs[0]); Adjoint T(qs[0]); Adjoint T(qs[0]);",
        ],
        num_qubits=1,
        sim_type=sim_type,
    )


# ===========================================================================
# Two-qubit gate equivalence tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_cz_symmetric(sim_type):
    check_programs_are_eq(
        [
            "X(qs[0]); H(qs[1]); CZ(qs[0], qs[1]); H(qs[1]); X(qs[0]);",
            "X(qs[0]); H(qs[1]); CZ(qs[1], qs[0]); H(qs[1]); X(qs[0]);",
        ],
        num_qubits=2,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_swap_commutes_operands(sim_type):
    check_programs_are_eq(
        [
            "H(qs[0]); X(qs[1]);",
            "SWAP(qs[0], qs[1]); X(qs[0]); H(qs[1]); SWAP(qs[0], qs[1]);",
        ],
        num_qubits=2,
        sim_type=sim_type,
        shots=500,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_swap_exchanges_qubit_states(sim_type):
    check_qsharp(
        "{use qs = Qubit[2]; X(qs[0]); SWAP(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        expected=["01"],
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_swap_twice_eq_identity(sim_type):
    check_programs_are_eq(
        ["X(qs[0]);", "X(qs[0]); SWAP(qs[0], qs[1]); SWAP(qs[0], qs[1]);"],
        num_qubits=2,
        sim_type=sim_type,
    )


# ===========================================================================
# Rotation gate equivalence tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rx_zero_eq_identity(sim_type):
    check_programs_are_eq(["", "Rx(0.0, qs[0]);"], num_qubits=1, sim_type=sim_type)


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rx_two_pi_eq_identity(sim_type):
    check_programs_are_eq(
        ["", "Rx(2.0 * Std.Math.PI(), qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rx_pi_eq_x(sim_type):
    check_programs_are_eq(
        ["X(qs[0]);", "Rx(Std.Math.PI(), qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rx_half_pi_eq_sx(sim_type):
    """Rx(π/2) is equivalent to SX (up to global phase)."""
    check_programs_are_eq(
        [
            "Rx(Std.Math.PI() / 2.0, qs[0]);",
            "Rx(Std.Math.PI() / 2.0, qs[0]);",
        ],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_ry_zero_eq_identity(sim_type):
    check_programs_are_eq(["", "Ry(0.0, qs[0]);"], num_qubits=1, sim_type=sim_type)


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_ry_two_pi_eq_identity(sim_type):
    check_programs_are_eq(
        ["", "Ry(2.0 * Std.Math.PI(), qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_ry_pi_eq_y(sim_type):
    check_programs_are_eq(
        ["Y(qs[0]);", "Ry(Std.Math.PI(), qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_zero_eq_identity(sim_type):
    check_programs_are_eq(["", "Rz(0.0, qs[0]);"], num_qubits=1, sim_type=sim_type)


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_two_pi_eq_identity(sim_type):
    check_programs_are_eq(
        ["", "Rz(2.0 * Std.Math.PI(), qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_pi_eq_z(sim_type):
    check_programs_are_eq(
        ["Z(qs[0]);", "Rz(Std.Math.PI(), qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_half_pi_eq_s(sim_type):
    check_programs_are_eq(
        ["S(qs[0]);", "Rz(Std.Math.PI() / 2.0, qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_neg_half_pi_eq_s_adj(sim_type):
    check_programs_are_eq(
        ["Adjoint S(qs[0]);", "Rz(-Std.Math.PI() / 2.0, qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_quarter_pi_eq_t(sim_type):
    check_programs_are_eq(
        ["T(qs[0]);", "Rz(Std.Math.PI() / 4.0, qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rz_neg_quarter_pi_eq_t_adj(sim_type):
    check_programs_are_eq(
        ["Adjoint T(qs[0]);", "Rz(-Std.Math.PI() / 4.0, qs[0]);"],
        num_qubits=1,
        sim_type=sim_type,
    )


# --- Two-qubit rotations ---


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rxx_zero_eq_identity(sim_type):
    check_programs_are_eq(
        ["", "Rxx(0.0, qs[0], qs[1]);"],
        num_qubits=2,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rxx_pi_eq_x_tensor_x(sim_type):
    check_programs_are_eq(
        ["X(qs[0]); X(qs[1]);", "Rxx(Std.Math.PI(), qs[0], qs[1]);"],
        num_qubits=2,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_ryy_zero_eq_identity(sim_type):
    check_programs_are_eq(
        ["", "Ryy(0.0, qs[0], qs[1]);"],
        num_qubits=2,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_ryy_pi_eq_y_tensor_y(sim_type):
    check_programs_are_eq(
        ["Y(qs[0]); Y(qs[1]);", "Ryy(Std.Math.PI(), qs[0], qs[1]);"],
        num_qubits=2,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rzz_zero_eq_identity(sim_type):
    check_programs_are_eq(
        ["", "Rzz(0.0, qs[0], qs[1]);"],
        num_qubits=2,
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", NON_CLIFFORD_SIM_TYPES)
def test_rzz_pi_eq_z_tensor_z(sim_type):
    check_programs_are_eq(
        ["Z(qs[0]); Z(qs[1]);", "Rzz(Std.Math.PI(), qs[0], qs[1]);"],
        num_qubits=2,
        sim_type=sim_type,
    )


# ===========================================================================
# Reset and measurement tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_reset_takes_qubit_back_to_zero(sim_type):
    check_qsharp(
        "{use q = Qubit(); X(q); Reset(q); M(q)}",
        expected=["0"],
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_mresetz_resets_after_measurement(sim_type):
    check_qsharp(
        "{use q = Qubit(); X(q); let r1 = MResetZ(q); let r2 = MResetZ(q); (r1, r2)}",
        expected=["10"],
        sim_type=sim_type,
    )


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_mz_does_not_reset(sim_type):
    check_qsharp(
        "{use q = Qubit(); X(q); let r1 = M(q); let r2 = M(q); Reset(q); (r1, r2)}",
        expected=["11"],
        sim_type=sim_type,
    )


# ===========================================================================
# Multi-qubit state tests
# ===========================================================================


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_bell_state_produces_correlated_measurements(sim_type):
    results = compile_and_run(
        "{use qs = Qubit[2]; H(qs[0]); CNOT(qs[0], qs[1]); [MResetZ(qs[0]), MResetZ(qs[1])]}",
        shots=100,
        sim_type=sim_type,
    )
    allowed = {"00", "11"}
    for shot in results:
        assert shot in allowed, f"Unexpected Bell outcome: {shot}"
    outcomes = set(results)
    assert len(outcomes) == 2, "Expected both correlated outcomes"


@pytest.mark.parametrize("sim_type", SIM_TYPES)
def test_ghz_state_three_qubits(sim_type):
    results = compile_and_run(
        (
            "{use qs = Qubit[3]; H(qs[0]); CNOT(qs[0], qs[1]); CNOT(qs[1], qs[2]);"
            " [MResetZ(qs[0]), MResetZ(qs[1]), MResetZ(qs[2])]}"
        ),
        shots=100,
        sim_type=sim_type,
    )
    allowed = {"000", "111"}
    for shot in results:
        assert shot in allowed, f"Unexpected GHZ outcome: {shot}"
    outcomes = set(results)
    assert len(outcomes) == 2, "Expected both GHZ outcomes"
