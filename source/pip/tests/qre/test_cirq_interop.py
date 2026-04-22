# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

cirq = pytest.importorskip("cirq")

from qsharp.qre import PSSPC
from qsharp.qre.application import CirqApplication
from qsharp.qre.interop import trace_from_cirq
from qsharp.qre.interop._cirq import (
    TypedQubit,
    QubitType,
    read_from_memory,
    write_to_memory,
)


def test_with_qft():
    """Test trace generation from a 1025-qubit QFT circuit."""
    _test_one_circuit(cirq.qft(*cirq.LineQubit.range(1025)), 1025, 74142, 92932)


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


def test_circuit_repetitions():
    """Test trace generation from a circuit operation with repetitions."""
    _test_one_circuit(
        cirq.CircuitOperation(
            cirq.Circuit(cirq.H.on_each(*cirq.LineQubit.range(3))).freeze()
        ).repeat(5),
        3,
        5,
        15,
    )


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
    circuit,
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


def _make_memory_circuit(*ops):
    """Build a circuit from operations on 2 memory + 2 compute TypedQubits."""
    mem = [TypedQubit(cirq.LineQubit(i), QubitType.MEMORY) for i in range(2)]
    comp = [TypedQubit(cirq.LineQubit(2 + i), QubitType.COMPUTE) for i in range(2)]
    moments = []
    for op_fn in ops:
        moments.append(op_fn(mem, comp))
    return cirq.Circuit(moments)


def test_write_to_memory_memory_compute_true():
    """Test WriteToMemoryGate produces WRITE_TO_MEMORY instructions when memory_compute is True."""
    circuit = _make_memory_circuit(write_to_memory)
    trace = trace_from_cirq(circuit, track_memory_qubits=True)

    assert trace.compute_qubits == 2
    assert trace.memory_qubits == 2
    assert trace.total_qubits == 4
    assert trace.depth == 1
    assert trace.num_gates == 2

    # 2 WRITE_TO_MEMORY ops × 2 logical cycles each = 4
    transformed = PSSPC(num_ts_per_rotation=16, ccx_magic_states=False).transform(trace)
    assert transformed is not None
    assert transformed.depth == 4


def test_write_to_memory_memory_compute_false():
    """Test WriteToMemoryGate decomposes into SWAPs when memory_compute is False."""
    circuit = _make_memory_circuit(write_to_memory)
    trace = trace_from_cirq(circuit, track_memory_qubits=False)

    assert trace.compute_qubits == 4
    assert trace.memory_qubits is None
    assert trace.total_qubits == 4
    assert trace.depth == 1
    assert trace.num_gates == 2

    # Decomposed SWAPs are Clifford — no logical cycles
    transformed = PSSPC(num_ts_per_rotation=16, ccx_magic_states=False).transform(trace)
    assert transformed is not None
    assert transformed.depth == 0


def test_read_from_memory_memory_compute_true():
    """Test ReadFromMemoryGate produces READ_FROM_MEMORY instructions when memory_compute is True."""
    circuit = _make_memory_circuit(read_from_memory)
    trace = trace_from_cirq(circuit, track_memory_qubits=True)

    assert trace.compute_qubits == 2
    assert trace.memory_qubits == 2
    assert trace.total_qubits == 4
    assert trace.depth == 1
    assert trace.num_gates == 2

    # 2 READ_FROM_MEMORY ops × 1 logical cycle each = 2
    transformed = PSSPC(num_ts_per_rotation=16, ccx_magic_states=False).transform(trace)
    assert transformed is not None
    assert transformed.depth == 2


def test_read_from_memory_memory_compute_false():
    """Test ReadFromMemoryGate decomposes into SWAPs when memory_compute is False."""
    circuit = _make_memory_circuit(read_from_memory)
    trace = trace_from_cirq(circuit, track_memory_qubits=False)

    assert trace.compute_qubits == 4
    assert trace.memory_qubits is None
    assert trace.total_qubits == 4
    assert trace.depth == 1
    assert trace.num_gates == 2

    # Decomposed SWAPs are Clifford — no logical cycles
    transformed = PSSPC(num_ts_per_rotation=16, ccx_magic_states=False).transform(trace)
    assert transformed is not None
    assert transformed.depth == 0


def test_read_write_memory_round_trip_memory_compute_true():
    """Test a write followed by a read produces both instruction types with memory_compute True."""
    circuit = _make_memory_circuit(write_to_memory, read_from_memory)
    trace = trace_from_cirq(circuit, track_memory_qubits=True)

    assert trace.compute_qubits == 2
    assert trace.memory_qubits == 2
    assert trace.total_qubits == 4
    assert trace.depth == 2
    assert trace.num_gates == 4

    # 2 WRITE_TO_MEMORY × 2 + 2 READ_FROM_MEMORY × 1 = 6
    transformed = PSSPC(num_ts_per_rotation=16, ccx_magic_states=False).transform(trace)
    assert transformed is not None
    assert transformed.depth == 6


def test_read_write_memory_round_trip_memory_compute_false():
    """Test a write followed by a read decomposes fully with memory_compute False."""
    circuit = _make_memory_circuit(write_to_memory, read_from_memory)
    trace = trace_from_cirq(circuit, track_memory_qubits=False)

    assert trace.compute_qubits == 4
    assert trace.memory_qubits is None
    assert trace.total_qubits == 4
    assert trace.depth == 2
    assert trace.num_gates == 4

    # Decomposed SWAPs are Clifford — no logical cycles
    transformed = PSSPC(num_ts_per_rotation=16, ccx_magic_states=False).transform(trace)
    assert transformed is not None
    assert transformed.depth == 0


def test_plain_circuit_unaffected_by_memory_compute():
    """Test that memory_compute has no effect on circuits without memory qubits."""
    circuit = cirq.H.on_each(*cirq.LineQubit.range(3))

    trace_true = trace_from_cirq(circuit, track_memory_qubits=True)
    trace_false = trace_from_cirq(circuit, track_memory_qubits=False)

    assert trace_true.compute_qubits == trace_false.compute_qubits == 3
    assert trace_true.memory_qubits is None
    assert trace_false.memory_qubits is None
    assert trace_true.num_gates == trace_false.num_gates == 3
