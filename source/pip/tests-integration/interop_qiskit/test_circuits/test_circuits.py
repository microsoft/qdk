# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

import numpy as np
from typing import Tuple, List

from interop_qiskit import QISKIT_AVAILABLE, SKIP_REASON

if QISKIT_AVAILABLE:
    from qiskit.circuit import QuantumCircuit
    from qsharp.interop.qiskit import QSharpBackend


def random_bit() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (?)
    """
    circuit = QuantumCircuit(1, 1)
    circuit.name = "Single qubit random"
    circuit.h(0)
    circuit.measure(0, 0)

    return circuit, ["0", "1"]


def intrinsic_hixyz() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1, 1, 1, 1, 1, 1)
    """
    circuit = QuantumCircuit(6, 6)
    circuit.name = "HIXYZ"

    # h in qs is h in qiskit
    circuit.h(0)
    circuit.z(0)
    circuit.h(0)
    # mresetz == measure()
    circuit.measure(0, 0)

    # i target
    circuit.x(1)
    circuit.id(1)
    # mresetz == measure()
    circuit.measure(1, 1)

    # x
    circuit.x(2)
    circuit.measure(2, 2)

    # ya
    circuit.y(3)
    circuit.measure(3, 3)

    # yb
    circuit.h(4)
    circuit.y(4)
    circuit.h(4)
    circuit.measure(4, 4)

    # z
    circuit.h(5)
    circuit.z(5)
    circuit.h(5)
    circuit.measure(5, 5)

    return circuit, ["111111"]


def intrinsic_ccnot() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1, 1, 1, 0, 0, 1, 0, 0, 0)
    """
    circuit = QuantumCircuit(9, 9)
    circuit.name = "_CCNOT"

    circuit.ccx(0, 1, 2)

    circuit.measure(0, 0)
    circuit.measure(1, 1)
    circuit.measure(2, 2)

    circuit.x(3)
    circuit.ccx(3, 4, 5)

    circuit.measure(3, 3)
    circuit.measure(4, 4)
    circuit.measure(5, 5)

    circuit.x(6)
    circuit.x(7)
    circuit.ccx(6, 7, 8)

    circuit.measure(6, 6)
    circuit.measure(7, 7)
    circuit.measure(8, 8)

    return circuit, ["111001000"]


def intrinsic_cnot() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1, 1, 0, 0)
    """
    circuit = QuantumCircuit(4, 4)
    circuit.name = "_CNOT"

    circuit.cx(0, 1)

    circuit.measure(0, 0)
    circuit.measure(1, 1)

    circuit.x(2)
    circuit.cx(2, 3)

    circuit.measure(2, 2)
    circuit.measure(3, 3)

    return circuit, ["1100"]


def intrinsic_measure() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1, 0)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Intrinsic_M"

    circuit.measure(0, 0)

    circuit.x(1)

    circuit.measure(1, 1)

    return circuit, ["10"]


def intrinsic_stswap() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1, 0, 1, 1)
    """
    circuit = QuantumCircuit(4, 4)
    circuit.name = "STSWAP"

    circuit.h(0)
    circuit.s(0)
    circuit.s(0)
    circuit.h(0)
    circuit.measure(0, 0)

    circuit.h(1)
    circuit.t(1)
    circuit.t(1)
    circuit.t(1)
    circuit.t(1)
    circuit.h(1)
    circuit.measure(1, 1)

    circuit.x(2)
    circuit.swap(2, 3)
    circuit.measure(2, 2)
    circuit.measure(3, 3)

    return circuit, ["1011"]


def exercise_reset() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0, 0)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test reset"

    circuit.measure(0, 0)

    circuit.x(1)
    circuit.reset(1)
    circuit.measure(1, 1)

    return circuit, ["00"]


def exercise_rx_ry_rz() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0, 0, 0)
    """
    # Create a quantum circuit with one qubit and one classical bit
    circuit = QuantumCircuit(3, 3)
    circuit.name = "Test_rx_ry_rz"

    # Apply RX, RY, and RZ gates with some arbitrary angles
    for _ in range(4):
        circuit.rx(np.pi, 0)  # Rotate around the X-axis by pi radians
        circuit.ry(np.pi, 1)  # Rotate around the Y-axis by pi radians
        circuit.rz(np.pi, 2)  # Rotate around the Z-axis by pi radians

    circuit.measure(0, 0)
    circuit.measure(1, 1)
    circuit.measure(2, 2)

    return circuit, ["000"]


def exercise_tdg_sdg() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1, 1)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_tdg"

    circuit.h(0)  # changes to + state
    circuit.tdg(0)
    circuit.tdg(0)
    circuit.tdg(0)
    circuit.tdg(0)
    circuit.h(0)
    circuit.measure(0, 0)

    circuit.h(1)
    circuit.sdg(1)
    circuit.sdg(1)
    circuit.h(1)
    circuit.measure(1, 1)

    return circuit, ["11"]


def exercise_rxx() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0, 1)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_rxx"

    circuit.rxx(np.pi / 2, 0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["00", "11"]


def exercise_ryy() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0, 1)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_ryy"

    circuit.ryy(np.pi / 2, 0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["00", "11"]


def exercise_rzz() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0, 1)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_rzz"

    circuit.h(0)
    circuit.h(1)
    circuit.rzz(np.pi / 2, 0, 1)
    circuit.h(0)
    circuit.h(1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["00", "11"]


def exercise_barrier() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1)
    """
    circuit = QuantumCircuit(1, 1)
    circuit.name = "Test_barrier_delay"

    circuit.x(0)
    circuit.barrier()
    circuit.x(0)
    circuit.barrier()
    circuit.x(0)

    circuit.measure(0, 0)

    return circuit, ["1"]


def exercise_delay() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0)
    """
    circuit = QuantumCircuit(1, 1)
    circuit.name = "Test_delay"

    circuit.delay(100, 0, unit="ns")  # Introducing a delay of 100 nanoseconds

    circuit.measure(0, 0)

    return circuit, ["0"]


def exercise_initialize_prepare_state() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: The qubits are initialized to the state [1/√2, 1/√2, 0, 0]."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test initialize and prepare state"

    # State vector to initialize: |ψ⟩ = (|0⟩ - |1⟩) / √2
    circuit.initialize([1 / np.sqrt(2), -1 / np.sqrt(2)], 0)
    circuit.h(0)
    circuit.measure(0, 0)

    circuit.prepare_state([1 / np.sqrt(2), -1 / np.sqrt(2)], 1)
    circuit.h(1)
    circuit.measure(1, 1)

    return circuit, ["11"]


def exercise_dcx() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0, 0) or (1, 1)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_DCX"

    circuit.h(0)
    circuit.dcx(0, 1)
    circuit.h(1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["00"]


def exercise_ecr() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the ECR gate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_ECR"

    circuit.ecr(0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["01", "11"]


def exercise_iswap() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the iSwap gate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_iSwap"

    circuit.x(0)
    circuit.iswap(0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["10"]


def exercise_ms() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the MSGate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_MS"

    circuit = QuantumCircuit(2, 2)
    circuit.ms(np.pi / 2, [0, 1])
    circuit.measure([0, 1], [0, 1])

    return circuit, ["00", "11"]


def exercise_p() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1)
    """
    circuit = QuantumCircuit(1, 1)
    circuit.name = "Test_Phase"

    circuit.h(0)
    circuit.p(np.pi, 0)
    circuit.h(0)
    circuit.measure(0, 0)

    return circuit, ["1"]


def exercise_pauli() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1, 1)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_Pauli"

    circuit.h(0)
    circuit.pauli("XZ", [0, 1])
    circuit.h(0)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["11"]


def exercise_r_rccx_rcccx() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the RC3XGate to qubits."""
    circuit = QuantumCircuit(8, 8)
    circuit.name = "Test_R_RCCX_RC3X"

    circuit.r(np.pi, 0, 0)
    circuit.measure(0, 0)

    circuit.x(1)
    circuit.x(2)
    circuit.rccx(1, 2, 3)
    circuit.measure([1, 2, 3], [1, 2, 3])

    circuit.x(4)
    circuit.x(5)
    circuit.x(6)
    circuit.rcccx(4, 5, 6, 7)
    circuit.measure([4, 5, 6, 7], [4, 5, 6, 7])

    return circuit, ["11111111"]


def exercise_rzx() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the RZXGate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_RZX"

    circuit.rzx(np.pi / 2, 0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["00", "10"]


def exercise_sx_sxdg() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1)
    """
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_SX_SXDG"

    circuit.sx(0)
    circuit.sx(0)
    circuit.measure(0, 0)

    circuit.sxdg(1)
    circuit.sxdg(1)
    circuit.measure(1, 1)

    return circuit, ["11"]


def exercise_u() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (0)
    """
    circuit = QuantumCircuit(1, 1)
    circuit.name = "Test_U"

    for _ in range(4):
        circuit.u(np.pi, -np.pi / 2, np.pi / 2, 0)

    circuit.measure(0, 0)

    return circuit, ["0"]


def exercise_unitary() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result:
    (1)
    """
    circuit = QuantumCircuit(1, 1)
    circuit.name = "Test_Unitary"

    # this is the unitary matrix for the pauli x gate
    unitary_matrix = np.array([[0, 1], [1, 0]])
    circuit.unitary(unitary_matrix, [0])
    circuit.measure(0, 0)

    return circuit, ["1"]


def exercise_ccz() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CCZGate to qubits."""
    circuit = QuantumCircuit(3, 3)
    circuit.name = "Test_CCZ"

    circuit.x(0)
    circuit.x(1)
    circuit.h(2)
    circuit.ccz(0, 1, 2)
    circuit.h(2)
    circuit.measure([0, 1, 2], [0, 1, 2])

    return circuit, ["111"]


def exercise_ch() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CHGate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_CH"

    circuit.x(0)
    circuit.h(1)
    circuit.z(1)
    circuit.ch(0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["11"]


def exercise_cp() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CPhaseGate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_CP"

    circuit.x(0)
    circuit.h(1)
    circuit.cp(np.pi, 0, 1)
    circuit.h(1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["11"]


def exercise_crx_cry_crz() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CRXGate to qubits."""
    circuit = QuantumCircuit(6, 6)
    circuit.name = "Test_CRX_CRY_CRZ"

    circuit.x(0)
    circuit.crx(np.pi, 0, 1)
    circuit.measure([0, 1], [0, 1])

    circuit.x(2)
    circuit.cry(np.pi, 2, 3)
    circuit.measure([2, 3], [2, 3])

    circuit.x(4)
    circuit.h(5)
    circuit.crz(np.pi, 4, 5)
    circuit.h(5)
    circuit.measure([4, 5], [4, 5])

    return circuit, ["111111"]


def exercise_cs_csdg() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CSGate and CSdgGate to qubits."""
    circuit = QuantumCircuit(4, 4)
    circuit.name = "Test_CS_CSdg"

    circuit.x(0)
    circuit.h(1)
    circuit.cs(0, 1)
    circuit.cs(0, 1)
    circuit.h(1)
    circuit.measure([0, 1], [0, 1])

    circuit.x(2)
    circuit.h(3)
    circuit.csdg(2, 3)
    circuit.csdg(2, 3)
    circuit.h(3)
    circuit.measure([2, 3], [2, 3])

    return circuit, ["1111"]


def exercise_cswap() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CSwapGate to qubits."""
    circuit = QuantumCircuit(3, 3)
    circuit.name = "Test_CSwap"

    circuit.x(0)
    circuit.x(1)
    circuit.cswap(0, 1, 2)
    circuit.measure([0, 1, 2], [0, 1, 2])

    return circuit, ["101"]


def exercise_csx() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CSXGate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_CSX"

    circuit.x(0)
    circuit.csx(0, 1)
    circuit.csx(0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["11"]


def exercise_cu() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CUGate to qubits."""
    circuit = QuantumCircuit(2, 2)
    circuit.name = "Test_CU"

    circuit.u(np.pi, -np.pi / 2, np.pi / 2, 0)
    circuit.cu(np.pi, np.pi, -np.pi / 2, np.pi / 2, 0, 1)
    circuit.measure([0, 1], [0, 1])

    return circuit, ["11"]


def exercise_cx_cy_cz() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the CXGate to qubits."""
    circuit = QuantumCircuit(6, 6)
    circuit.name = "Test_CX_CY_CZ"

    circuit.x(0)
    circuit.cx(0, 1)
    circuit.measure([0, 1], [0, 1])

    circuit.x(2)
    circuit.cy(2, 3)
    circuit.measure([2, 3], [2, 3])

    circuit.x(4)
    circuit.h(5)
    circuit.cz(4, 5)
    circuit.h(5)
    circuit.measure([4, 5], [4, 5])

    return circuit, ["111111"]


def exercise_mcp() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the Multi-Controlled PhaseGate to qubits."""
    circuit = QuantumCircuit(3, 3)
    circuit.name = "Test_MCP"

    circuit.x(0)
    circuit.x(1)
    circuit.h(2)
    circuit.mcp(np.pi, [0, 1], 2)
    circuit.h(2)
    circuit.measure([0, 1, 2], [0, 1, 2])

    return circuit, ["111"]


def exercise_mcrx_mcry_mcrz() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the Multi-Controlled RXGate to qubits."""
    circuit = QuantumCircuit(9, 9)
    circuit.name = "Test_MCRX_MCRY_MCRZ"

    circuit.x(0)
    circuit.x(1)
    circuit.mcrx(np.pi, [0, 1], 2)
    circuit.measure([0, 1, 2], [0, 1, 2])

    circuit.x(3)
    circuit.x(4)
    circuit.mcry(np.pi, [3, 4], 5)
    circuit.measure([3, 4, 5], [3, 4, 5])

    circuit.x(6)
    circuit.x(7)
    circuit.h(8)
    circuit.mcrz(np.pi, [6, 7], 8)
    circuit.h(8)
    circuit.measure([6, 7, 8], [6, 7, 8])

    return circuit, ["111111111"]


def exercise_mcx() -> Tuple["QuantumCircuit", List[str]]:
    """Expected result: Applying the Multi-Controlled X gate to qubits."""
    circuit = QuantumCircuit(3, 3)
    circuit.name = "Test_MCX"

    circuit.x(0)
    circuit.x(1)
    circuit.mcx([0, 1], 2)
    circuit.measure([0, 1, 2], [0, 1, 2])

    return circuit, ["111"]
