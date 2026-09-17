# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import cmath
import math
from io import BytesIO
from pathlib import Path

import numpy as np
import pytest
from qdk._native import Result
from qdk.simulation import run_qir

from build_measured_circuit import (
    ParsedCircuit,
    build_ising_2d_qir,
    compile_measured_qir,
    parse_gates,
    qsharp_source,
)
from reference import (
    capture_reference,
    check_case_a_schedule,
    compare,
    generate,
    npy_bytes,
    probabilities,
    verify,
    verify_conversion,
)


def unmeasured_qir(calls: str, width: int = 3) -> str:
    return f"""\
%Qubit = type opaque
define i64 @ENTRYPOINT__main() #0 {{
entry:
  call void @__quantum__rt__initialize(i8* null)
{calls}
  call void @__quantum__rt__tuple_record_output(i64 0, i8* null)
  ret i64 0
}}
declare void @__quantum__rt__initialize(i8*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)
attributes #0 = {{ "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="{width}" "required_num_results"="0" }}
"""


RX = "call void @__quantum__qis__rx__body(double 0.7, %Qubit* inttoptr (i64 0 to %Qubit*))"
RZZ = "call void @__quantum__qis__rzz__body(double -0.4, %Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))"


def test_parser_preserves_coefficients_operands_and_order():
    parsed = parse_gates(unmeasured_qir(f"{RX}\n{RZZ}\n{RX}"))
    assert parsed == ParsedCircuit(3, [("rx", 0.7, 0), ("rzz", -0.4, 0, 2), ("rx", 0.7, 0)])


@pytest.mark.parametrize("call", [
    RX.replace("inttoptr (i64 0 to %Qubit*)", "null"),
    RX.replace("double 0.7", "double %angle"),
    RX.replace("double 0.7", "double nan"),
    RX.replace("double 0.7", "double inf"),
    RX.replace("i64 0", "i64 3"),
    RX.replace("rx__body", "rx__adj"),
    RX.replace("rx__body", "ry__body"),
    RX.replace("rx__body", "rzz__body"),
    RZZ.replace("rzz__body", "rx__body"),
    RZZ.replace("i64 2", "i64 0"),
    RX.replace("call void", "call\nvoid"),
])
def test_parser_rejects_unhandled_calls_instead_of_omitting_them(call):
    with pytest.raises(ValueError):
        parse_gates(unmeasured_qir(f"{RX}\n{call}\n{RZZ}"))


def test_compiled_qir_preserves_the_entire_prefix_and_terminal_order():
    raw = unmeasured_qir(f"{RX}\n{RZZ}")
    parsed = parse_gates(raw)
    measured = compile_measured_qir(qsharp_source(parsed))
    report = verify_conversion(raw, measured, parsed)
    assert report["unitary_gate_count"] == 2
    assert report["measurement_qubits"] == report["output_result_ids"] == [0, 1, 2]


@pytest.mark.parametrize("changed", [
    [("rzz", -0.4, 0, 2), ("rx", 0.7, 0)],
    [("rx", 0.8, 0), ("rzz", -0.4, 0, 2)],
    [("rx", 0.7, 1), ("rzz", -0.4, 0, 2)],
    [("rx", 0.7, 0)],
])
def test_conversion_rejects_changed_order_angle_operand_or_gate_count(changed):
    raw = unmeasured_qir(f"{RX}\n{RZZ}")
    measured = compile_measured_qir(qsharp_source(ParsedCircuit(3, changed)))
    with pytest.raises(ValueError, match="unitary prefix"):
        verify_conversion(raw, measured, parse_gates(raw))


def test_conversion_rejects_dropped_measurement_or_reordered_output():
    raw = unmeasured_qir(f"{RX}\n{RZZ}")
    parsed = parse_gates(raw)
    measured = compile_measured_qir(qsharp_source(parsed))
    for changed in (
        measured.replace(
            "call void @__quantum__qis__m__body",
            "; call void @__quantum__qis__m__body",
            1,
        ),
        measured.replace(
            "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0",
            "call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1",
            1,
        ),
    ):
        assert changed != measured
        with pytest.raises(ValueError):
            verify_conversion(raw, changed, parsed)


@pytest.mark.parametrize("old,new", [
    ('"base_profile"', '"adaptive_profile"'),
    ('"qir_profiles"="base_profile"', ""),
    ("block_0:", "block_0:\n  br label %next\nnext:"),
    (
        "  ret i64 0",
        "  call void @__quantum__rt__initialize(i8* null)\n  ret i64 0",
    ),
])
def test_conversion_rejects_wrong_profile_control_flow_or_initialization(old, new):
    raw = unmeasured_qir(RX)
    parsed = parse_gates(raw)
    measured = compile_measured_qir(qsharp_source(parsed))
    assert old in measured
    with pytest.raises(ValueError):
        verify_conversion(raw, measured.replace(old, new), parsed)


@pytest.mark.parametrize("angle", [0.7, -0.7, math.pi])
@pytest.mark.parametrize("target", [0, 2])
def test_signed_rx_and_asymmetric_bit_order_before_reset(angle, target):
    circuit = ParsedCircuit(3, [("rx", angle, target)])
    actual = capture_reference(qsharp_source(circuit, dump_state=True), 3)
    expected = np.zeros(8, dtype=complex)
    expected[0] = math.cos(angle / 2)
    expected[1 << target] = -1j * math.sin(angle / 2)
    np.testing.assert_allclose(actual, expected, rtol=0, atol=1e-12)


@pytest.mark.parametrize("angle", [0.4, -0.4])
def test_rzz_preserves_the_analytic_global_phase(angle):
    circuit = ParsedCircuit(3, [("rzz", angle, 0, 2)])
    actual = capture_reference(qsharp_source(circuit, dump_state=True), 3)
    expected = np.zeros(8, dtype=complex)
    expected[0] = cmath.exp(-0.5j * angle)
    np.testing.assert_allclose(actual, expected, rtol=0, atol=1e-12)


def test_nonadjacent_rzz_with_unequal_rotations_and_interference():
    a, b, phi, c = 0.7, -0.3, 0.41, 0.29
    circuit = ParsedCircuit(3, [
        ("rx", a, 0), ("rx", b, 2), ("rzz", phi, 0, 2), ("rx", c, 0),
    ])
    actual = capture_reference(qsharp_source(circuit, dump_state=True), 3)
    expected = np.zeros(8, dtype=complex)
    for bit0 in range(2):
        for bit2 in range(2):
            def before_last_rx(bit):
                rx0 = -1j * math.sin(a / 2) if bit else math.cos(a / 2)
                rx2 = -1j * math.sin(b / 2) if bit2 else math.cos(b / 2)
                return rx0 * rx2 * cmath.exp(-0.5j * phi * (-1) ** (bit + bit2))

            expected[bit0 + 4 * bit2] = (
                math.cos(c / 2) * before_last_rx(bit0)
                - 1j * math.sin(c / 2) * before_last_rx(1 - bit0)
            )
    np.testing.assert_allclose(actual, expected, rtol=0, atol=1e-12)
    actual_p, norm = probabilities(actual)
    np.testing.assert_allclose(actual_p, abs(expected) ** 2, rtol=0, atol=1e-12)
    assert abs(norm - 1) < 1e-12


def test_missing_state_dump_is_not_a_reference():
    circuit = ParsedCircuit(1, [("rx", 0.7, 0)])
    with pytest.raises(ValueError, match="exactly one"):
        capture_reference(qsharp_source(circuit), 1)


def test_reference_rejects_unbounded_width_before_execution():
    with pytest.raises(ValueError, match="bounded"):
        capture_reference("", 17)


@pytest.mark.parametrize("state", [
    np.array([0j, 0j]),
    np.array([2 + 0j, 0j]),
    np.array([complex(float("nan")), 0j]),
    np.array([complex(float("inf")), 0j]),
    np.array([[1 + 0j, 0j]]),
])
def test_invalid_states_are_not_silently_normalized(state):
    with pytest.raises(ValueError):
        probabilities(state)


def test_comparison_does_not_hide_global_phase_or_shape_errors():
    with pytest.raises(ValueError, match="comparison failed"):
        compare(np.array([1j, 0j]), np.array([1 + 0j, 0j]), 1e-8)
    with pytest.raises(ValueError, match="shape mismatch"):
        compare(np.array([1 + 0j]), np.array([1 + 0j, 0j]), 1e-8)


@pytest.mark.parametrize("size,counts", [(2, {"rx": 48, "rzz": 40}), (4, {"rx": 192, "rzz": 240})])
def test_chemistry_case_a_matches_hand_derived_suzuki_coefficients(size, counts):
    raw = build_ising_2d_qir(size, size, 1.0, 4, 2)
    parsed = parse_gates(raw)
    lattice = check_case_a_schedule(parsed, size, size)
    measured = compile_measured_qir(qsharp_source(parsed))
    report = verify_conversion(raw, measured, parsed)
    assert report["gate_counts"] == counts
    assert lattice["edge_count"] == 2 * size * (size - 1)
    assert lattice["maximum_coefficient_error"] < 1e-14
    reordered = sorted(parsed.gates, key=lambda gate: gate[0])
    with pytest.raises(ValueError, match="Suzuki layer"):
        check_case_a_schedule(ParsedCircuit(parsed.num_qubits, reordered), size, size)


def test_generation_refuses_to_overwrite_existing_artifacts(tmp_path):
    sentinel = tmp_path / "sentinel"
    sentinel.write_text("keep")
    with pytest.raises(FileExistsError, match="Refusing to overwrite"):
        generate(tmp_path)
    assert sentinel.read_text() == "keep"


@pytest.mark.parametrize("array", [
    np.array([complex(-0.0, 0.0), 1.23 - 0.045j], dtype="<c16"),
    np.array([-0.0, 0.12345678901234567], dtype="<f8"),
])
def test_numpy_storage_preserves_dtype_shape_and_exact_bytes(array):
    restored = np.load(BytesIO(npy_bytes(array)), allow_pickle=False)
    assert restored.dtype.str == array.dtype.str
    assert restored.shape == array.shape
    assert restored.tobytes() == array.tobytes()


def test_frozen_case_a_reference():
    report = verify(Path(__file__).parent / "fixtures" / "case_a_4x4")
    assert report["passed"]
    assert report["conversion"]["qubits"] == report["conversion"]["results"] == 16
    assert report["reference_comparison"]["maximum_amplitude_error"] <= 1e-12


def test_frozen_case_a_qir_is_accepted_by_public_cpu_entry_point():
    qir = (Path(__file__).parent / "fixtures" / "case_a_4x4" / "measured.ll").read_text()
    shots = run_qir(qir, shots=2, seed=42, type="cpu")
    assert len(shots) == 2
    assert all(len(shot) == 16 for shot in shots)
    assert all(result in (Result.Zero, Result.One) for shot in shots for result in shot)
