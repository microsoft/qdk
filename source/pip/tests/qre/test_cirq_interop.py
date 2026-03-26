# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Iterable

import cirq
from qsharp.qre.application import CirqApplication


def test_with_qft():
    _test_one_circuit(cirq.qft(*cirq.LineQubit.range(1025)), 1025, 265498, 319925)


def test_h():
    _test_one_circuit(cirq.H(cirq.LineQubit(0)), 1, 1, 1)
    _test_one_circuit(cirq.H(cirq.LineQubit(0)) ** 0.5, 1, 3, 3)


def test_cz():
    _test_one_circuit(cirq.CZ(*cirq.LineQubit.range(2)), 2, 1, 1)
    _test_one_circuit(cirq.CZ(*cirq.LineQubit.range(2)) ** 0.5, 2, 4, 5)


def test_ccx():
    _test_one_circuit(cirq.CCX(*cirq.LineQubit.range(3)), 3, 1, 1)
    _test_one_circuit(cirq.CCX(*cirq.LineQubit.range(3)) ** 0.5, 3, 11, 17)


def test_ccz():
    _test_one_circuit(cirq.CCZ(*cirq.LineQubit.range(3)), 3, 1, 1)
    _test_one_circuit(cirq.CCZ(*cirq.LineQubit.range(3)) ** 0.5, 3, 10, 15)


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
    circuit: cirq.Circuit | cirq.Operation | Iterable[cirq.Operation],
    expected_qubits: int,
    expected_depth: int,
    expected_gates: int,
):
    if not isinstance(circuit, cirq.Circuit):
        circuit = cirq.Circuit(circuit)

    app = CirqApplication(circuit)
    trace = app.get_trace()

    print(trace)

    assert trace.total_qubits == expected_qubits, "unexpected number of qubits in trace"
    assert trace.depth == expected_depth, "unexpected depth of trace"
    assert trace.num_gates == expected_gates, "unexpected number of gates in trace"
