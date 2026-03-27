# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import cirq
from qsharp.qre.application import CirqApplication


def test_with_qft():
    _test_one_circuit(cirq.qft(*cirq.LineQubit.range(1025)), 1025, 212602, 266007)


def test_h():
    _test_one_circuit(cirq.H, 1, 1, 1)
    _test_one_circuit(cirq.H**0.5, 1, 3, 3)


def test_cx():
    _test_one_circuit(cirq.CX, 2, 1, 1)
    _test_one_circuit(cirq.CX**0.5, 2, 6, 7)
    _test_one_circuit(cirq.CX**0.25, 2, 6, 7)


def test_cz():
    _test_one_circuit(cirq.CZ, 2, 1, 1)
    _test_one_circuit(cirq.CZ**0.5, 2, 4, 5)
    _test_one_circuit(cirq.CZ**0.25, 2, 4, 5)


def test_swap():
    _test_one_circuit(cirq.SWAP, 2, 1, 1)
    _test_one_circuit(cirq.SWAP**0.5, 2, 8, 9)


def test_ccx():
    _test_one_circuit(cirq.CCX, 3, 1, 1)
    _test_one_circuit(cirq.CCX**0.5, 3, 11, 17)


def test_ccz():
    _test_one_circuit(cirq.CCZ, 3, 1, 1)
    _test_one_circuit(cirq.CCZ**0.5, 3, 10, 15)


def test_circuit_with_block():
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
    app = CirqApplication(circuit)
    trace = app.get_trace()

    assert trace.total_qubits == expected_qubits, "unexpected number of qubits in trace"
    assert trace.depth == expected_depth, "unexpected depth of trace"
    assert trace.num_gates == expected_gates, "unexpected number of gates in trace"
