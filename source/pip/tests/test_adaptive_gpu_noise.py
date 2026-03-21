# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Per-opcode tests for the adaptive GPU bytecode interpreter.

Each test targets one (or a small family of) bytecode instruction(s) by
supplying hand-written Adaptive Profile QIR that exercises the instruction
and encodes the expected result into a measurement outcome.

Tests are ordered to match the opcode definitions in ``_adaptive_opcodes.py``
so that coverage can be verified by reading both files side by side.

Requires QDK_GPU_TESTS env var and a GPU adapter.
"""

import os
import sys
from collections import Counter
import pytest
from typing import Optional, List
import qsharp.openqasm

# Skip the whole module when GPU tests aren't requested.
if not os.environ.get("QDK_GPU_TESTS"):
    pytest.skip("Skipping GPU tests (QDK_GPU_TESTS not set)", allow_module_level=True)

SKIP_REASON = "GPU is not available"
GPU_AVAILABLE = False

try:
    from qsharp._native import try_create_gpu_adapter

    gpu_info = try_create_gpu_adapter()
    print(f"*** USING GPU: {gpu_info}", file=sys.stderr)
    GPU_AVAILABLE = True
except OSError as e:
    SKIP_REASON = str(e)

from qsharp._simulation import run_qir, NoiseConfig, Result

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Deterministic programs need a single shot but we run multiple shots
# to verify that multiple shots yield the same result.
SHOTS = 100


def map_result_list_to_str(results: List[Result]):
    results_str = ""
    for r in results:
        match r:
            case Result.Zero:
                results_str += "0"
            case Result.One:
                results_str += "1"
            case Result.Loss:
                results_str += "L"
    return results_str


def get_histogram(
    qir_fragment: str,
    *,
    extra_decls: str = "",
    num_qubits: int = 1,
    num_results: int = 1,
    noise: Optional[NoiseConfig] = None,
    record: Optional[List[int]] = None,
    shots=SHOTS,
):
    qir = format_qir(
        qir_fragment,
        extra_decls=extra_decls,
        num_qubits=num_qubits,
        num_results=num_results,
        record=record,
    )
    results = map(
        map_result_list_to_str, run_qir(qir, shots, noise, seed=42, type="gpu")
    )
    return Counter(results)


def check_result(
    qir_fragment: str,
    expected: str,
    *,
    extra_decls: str = "",
    num_qubits: int = 1,
    num_results: int = 1,
    noise: Optional[NoiseConfig] = None,
    record: Optional[List[int]] = None,
):
    """Assert every shot produces *expected*."""
    counts = get_histogram(
        qir_fragment,
        extra_decls=extra_decls,
        num_qubits=num_qubits,
        num_results=num_results,
        noise=noise,
        record=record,
    )

    assert counts == {
        expected: SHOTS
    }, f"Expected all {SHOTS} shots to be '{expected}', got {counts}"


_DECLS = """\
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare void @__quantum__qis__reset__body(%Qubit*)
declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__z__body(%Qubit*)
declare void @__quantum__qis__s__body(%Qubit*)
declare void @__quantum__qis__t__body(%Qubit*)
declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__rz__body(double, %Qubit*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)
declare void @__quantum__rt__initialize(i8*)
"""


def format_qir(
    body: str,
    *,
    extra_decls: str = "",
    num_qubits: int = 1,
    num_results: int = 1,
    record=None,
):
    if record is None:
        record = range(num_results)
    output_recording = (
        f"  call void @__quantum__rt__tuple_record_output(i64 {len(record)}, i8* null)"
    )
    for result_id in record:
        output_recording += f"\n  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 {result_id} to %Result*), i8* null)"

    return f"""\
%Result = type opaque
%Qubit = type opaque

define i64 @ENTRYPOINT__main() #0 {{
{body}
{output_recording}
  ret i64 0
}}

{_DECLS}
{extra_decls}
attributes #0 = {{ "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="{num_qubits}" "required_num_results"="{num_results}" }}
attributes #1 = {{ "irreversible" }}
"""


# The purpose of this test is to inject noise in an identity gate, and assert its behavior.
# Since QIS does not specify an identity gate, we use CNOT and inject noise in the target qubit.
I_QIR = """
entry:
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""

H_I_H_QIR = """
entry:
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
  call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
"""


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_no_noise_on_i_yields_0():
    check_result(I_QIR, "0", num_qubits=2)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_x_noise_on_i_yields_1():
    noise = NoiseConfig()
    noise.cx.ix = 1.0
    check_result(I_QIR, "1", num_qubits=2, noise=noise)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_y_noise_on_i_yields_1():
    noise = NoiseConfig()
    noise.cx.iy = 1.0
    check_result(I_QIR, "1", num_qubits=2, noise=noise)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_z_noise_on_i_yields_0():
    noise = NoiseConfig()
    noise.cx.iz = 1.0
    check_result(I_QIR, "0", num_qubits=2, noise=noise)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_x_noise_on_h_i_h_yields_0():
    noise = NoiseConfig()
    noise.cx.ix = 1.0
    check_result(H_I_H_QIR, "0", num_qubits=2, noise=noise)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_y_noise_on_h_i_h_yields_1():
    noise = NoiseConfig()
    noise.cx.iy = 1.0
    check_result(H_I_H_QIR, "1", num_qubits=2, noise=noise)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_z_noise_on_h_i_h_yields_1():
    noise = NoiseConfig()
    noise.cx.iz = 1.0
    check_result(H_I_H_QIR, "1", num_qubits=2, noise=noise)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_probabilistic_x_noise():
    noise = NoiseConfig()
    noise.cx.ix = 0.5
    counts = get_histogram(I_QIR, shots=1000, noise=noise)

    assert counts["0"] > 400, f"Expected ~500 '0' results, got {counts['0']}"
    assert counts["1"] > 400, f"Expected ~500 '1' results, got {counts['1']}"


QASM_WITH_CORRELATED_NOISE = """
OPENQASM 3.0;
include "stdgates.inc";

@qdk.qir.noise_intrinsic
gate test_noise_intrinsic q0, q1, q2 {}

qubit[3] qs;
x qs[1];
test_noise_intrinsic qs[0], qs[1], qs[2];
bit[3] res = measure qs;
"""

QIR_WITH_CORRELATED_NOISE = qsharp.openqasm.compile(
    QASM_WITH_CORRELATED_NOISE,
    output_semantics=qsharp.openqasm.OutputSemantics.OpenQasm,
    target_profile=qsharp.TargetProfile.Adaptive_RIF,
)


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_noiseless_simulation_gpu():
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=None, type="gpu")
    assert output == [[Result.Zero, Result.One, Result.Zero]]


@pytest.mark.skipif(not GPU_AVAILABLE, reason=SKIP_REASON)
def test_noisy_simulation_gpu():
    noise = NoiseConfig()
    table = noise.intrinsic("test_noise_intrinsic", 3)
    table.yyy = 1.0
    output = run_qir(QIR_WITH_CORRELATED_NOISE, shots=1, noise=noise, type="gpu")
    assert output == [[Result.One, Result.Zero, Result.One]]
