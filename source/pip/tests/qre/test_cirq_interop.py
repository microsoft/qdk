# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import cirq
from qsharp.qre.application import CirqApplication


def test_with_qft():
    """Test trace generation from a 1025-qubit QFT circuit."""
    _test_one_circuit(cirq.qft(*cirq.LineQubit.range(1025)), 1025, 212602, 266007)


def test_h():
    """Test trace generation from Hadamard and fractional Hadamard gates."""
    _test_one_circuit(cirq.H, 1, 1, 1)
    _test_one_circuit(cirq.H**0.5, 1, 3, 3)


def test_cx():
    """Test trace generation from CX and fractional CX gates."""
    _test_one_circuit(cirq.CX, 2, 1, 1)
    _test_one_circuit(cirq.CX**0.5, 2, 6, 7)
    _test_one_circuit(cirq.CX**0.25, 2, 6, 7)


def test_cz():
    """Test trace generation from CZ and fractional CZ gates."""
    _test_one_circuit(cirq.CZ, 2, 1, 1)
    _test_one_circuit(cirq.CZ**0.5, 2, 4, 5)
    _test_one_circuit(cirq.CZ**0.25, 2, 4, 5)


def test_swap():
    """Test trace generation from SWAP and fractional SWAP gates."""
    _test_one_circuit(cirq.SWAP, 2, 1, 1)
    _test_one_circuit(cirq.SWAP**0.5, 2, 8, 9)


def test_ccx():
    """Test trace generation from CCX and fractional CCX gates."""
    _test_one_circuit(cirq.CCX, 3, 1, 1)
    _test_one_circuit(cirq.CCX**0.5, 3, 11, 17)


def test_ccz():
    """Test trace generation from CCZ and fractional CCZ gates."""
    _test_one_circuit(cirq.CCZ, 3, 1, 1)
    _test_one_circuit(cirq.CCZ**0.5, 3, 10, 15)


def test_circuit_with_block():
    """Test trace generation from a circuit with a custom decomposable gate."""
    class CustomGate(cirq.Gate):
        def num_qubits(self) -> int:
            return 2

        def _decompose_(self, qubits):
            a, b = qubits
            yield cirq.CX(a, b)
            yield cirq.CX(b, a)
            yield cirq.CX(a, b)

    q0, q1 = cirq.LineQubit.range(2)
    _test_one_circuit(
        [
            cirq.H.on_each(q0, q1),
            CustomGate().on(q0, q1),
        ],
        2,
        4,
        5,
    )


def _test_one_circuit(
    circuit: cirq.CIRCUIT_LIKE,
    expected_qubits: int,
    expected_depth: int,
    expected_gates: int,
):
    """Assert that a Cirq circuit produces a trace with the expected qubits, depth, and gates."""
    app = CirqApplication(circuit)
    trace = app.get_trace()

    assert trace.total_qubits == expected_qubits, "unexpected number of qubits in trace"
    assert trace.depth == expected_depth, "unexpected depth of trace"
    assert trace.num_gates == expected_gates, "unexpected number of gates in trace"
