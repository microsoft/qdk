# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Callable
import pytest

from interop_qiskit import QISKIT_AVAILABLE, SKIP_REASON

if QISKIT_AVAILABLE:
    from qiskit import QuantumCircuit
    from qsharp.interop.qiskit import QSharpBackend


def run_transpile_test(
    operation: Callable[["QuantumCircuit"], None], expected_output: str, **options
) -> None:
    circuit = QuantumCircuit(3, 3)
    operation(circuit)
    info = QSharpBackend()._qasm(circuit, **options)
    lines = info.splitlines()
    # remove the first four lines, which are the header
    # OPENQASM 3.0;
    # include "stdgates.inc";
    # bit[3] c;
    # qubit[3] q;
    remaining_lines = lines[4:]
    result = "\n".join(remaining_lines)
    assert result == expected_output


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_reset_instruction_transpiles() -> None:
    run_transpile_test(
        lambda circuit: circuit.reset(1),
        "reset q[1];",
        remove_reset_in_zero_state=False,
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_ccx_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.ccx(2, 0, 1), "ccx q[2], q[0], q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_cx_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.cx(2, 0), "cx q[2], q[0];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_cy_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.cy(1, 2), "cy q[1], q[2];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_cz_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.cz(0, 2), "cz q[0], q[2];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_rx_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.rx(0.5, 2), "rx(0.5) q[2];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_rxx_transpiles() -> None:
    run_transpile_test(
        lambda circuit: circuit.rxx(0.5, 2, 0),
        "rxx(0.5) q[2], q[0];",
        basis_gates=["rxx"],
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_ry_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.ry(0.5, 1), "ry(0.5) q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_ryy_transpiles() -> None:
    run_transpile_test(
        lambda circuit: circuit.ryy(0.5, 1, 2),
        "ryy(0.5) q[1], q[2];",
        basis_gates=["ryy"],
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_rz_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.rz(0.5, 1), "rz(0.5) q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_rzz_transpiles() -> None:
    run_transpile_test(
        lambda circuit: circuit.rzz(0.5, 0, 2),
        "rzz(0.5) q[0], q[2];",
        basis_gates=["rzz"],
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_h_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.h(1), "h q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_s_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.s(1), "s q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_sdg_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.sdg(1), "sdg q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_sx_transpiles_to_sx() -> None:
    run_transpile_test(
        lambda circuit: circuit.sx(1), "sx q[1];", disable_constants=False
    )


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_swap_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.swap(1, 0), "swap q[1], q[0];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_t_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.t(1), "t q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_tdg_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.tdg(1), "tdg q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_x_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.x(1), "x q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_y_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.y(1), "y q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_z_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.z(1), "z q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_crx_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.crx(0.5, 1, 2), "crx(0.5) q[1], q[2];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_cry_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.cry(0.5, 1, 2), "cry(0.5) q[1], q[2];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_crz_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.crz(0.5, 1, 2), "crz(0.5) q[1], q[2];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_id_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.id(1), "id q[1];")


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_gate_ch_transpiles() -> None:
    run_transpile_test(lambda circuit: circuit.ch(1, 0), "ch q[1], q[0];")
