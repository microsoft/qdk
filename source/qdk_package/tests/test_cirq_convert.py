# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Structural conversion between Cirq circuits and qdk.circuit.

Unitary comparisons are exact rather than up to global phase. Cirq states phase
explicitly through each gate's ``global_shift``, so a comparison that ignored it
would hide the part of the mapping most easily got wrong.
"""

import warnings

import numpy as np
import pytest

cirq = pytest.importorskip("cirq")

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.cirq import ConversionError, from_cirq, to_cirq  # noqa: E402

Q = cirq.LineQubit.range(3)


def assert_same_unitary(original) -> None:
    qubits = sorted(original.all_qubits())
    restored = to_cirq(from_cirq(original), qubits)
    expected = cirq.unitary(original)
    actual = cirq.unitary(restored)
    assert np.allclose(expected, actual), (
        f"unitary changed\nexpected:\n{np.round(expected, 6)}\n"
        f"actual:\n{np.round(actual, 6)}"
    )


GATES = {
    "H": [cirq.H(Q[0])],
    "H_then_CNOT": [cirq.H(Q[0]), cirq.CNOT(Q[0], Q[1])],
    "X_quarter": [cirq.X(Q[0]) ** 0.25],
    "Y_negative_half": [cirq.Y(Q[0]) ** -0.5],
    "Z_half": [cirq.Z(Q[0]) ** 0.5],
    "rotations": [cirq.rx(0.3)(Q[0]), cirq.ry(0.4)(Q[1]), cirq.rz(0.5)(Q[2])],
    "S_dagger": [cirq.S(Q[0]) ** -1],
    "T": [cirq.T(Q[0])],
    "SWAP": [cirq.SWAP(Q[0], Q[1])],
    "TOFFOLI": [cirq.TOFFOLI(Q[0], Q[1], Q[2])],
    "CSWAP": [cirq.CSWAP(Q[0], Q[1], Q[2])],
    "ISWAP": [cirq.ISWAP(Q[0], Q[1])],
    "CZ_half": [cirq.CZ(Q[0], Q[1]) ** 0.5],
    "CX_third": [cirq.CNOT(Q[0], Q[1]) ** (1 / 3)],
    "shifted_power": [cirq.XPowGate(exponent=0.3, global_shift=-0.25)(Q[0])],
    "GHZ": [cirq.H(Q[0]), cirq.CNOT(Q[0], Q[1]), cirq.CNOT(Q[1], Q[2])],
}


@pytest.mark.parametrize("name", sorted(GATES))
def test_a_circuit_survives_a_round_trip(name):
    assert_same_unitary(cirq.Circuit(GATES[name]))


def test_conversion_does_not_go_through_text(monkeypatch):
    """The conversion path must not serialize, at any depth."""

    def refuse(*args, **kwargs):
        raise AssertionError("conversion serialized the circuit to text")

    monkeypatch.setattr(cirq.Circuit, "to_qasm", refuse)
    monkeypatch.setattr(cirq, "qasm", refuse)

    circuit = cirq.Circuit([cirq.H(Q[0]), cirq.CNOT(Q[0], Q[1])])
    to_cirq(from_cirq(circuit), sorted(circuit.all_qubits()))


def test_qubit_identity_is_restored_from_the_supplied_order():
    """Cirq names qubits; the index we assign is ours, not the circuit's."""
    circuit = cirq.Circuit([cirq.X(cirq.LineQubit(0)), cirq.H(cirq.LineQubit(9))])
    mine = from_cirq(circuit)
    # LineQubit(9) is a label, not a position, so the width is two rather than ten.
    assert mine.num_qubits == 2

    restored = to_cirq(mine, sorted(circuit.all_qubits()))
    assert sorted(restored.all_qubits()) == sorted(circuit.all_qubits())


def test_qubits_of_any_kind_are_carried():
    qubits = [cirq.GridQubit(1, 2), cirq.NamedQubit("alice")]
    circuit = cirq.Circuit([cirq.X(qubits[0]), cirq.H(qubits[1])])

    restored = to_cirq(from_cirq(circuit), sorted(circuit.all_qubits()))
    assert sorted(restored.all_qubits()) == sorted(qubits)


def test_without_the_qubit_order_the_wires_are_plain_line_qubits():
    circuit = cirq.Circuit([cirq.X(cirq.NamedQubit("alice"))])
    restored = to_cirq(from_cirq(circuit))
    assert sorted(restored.all_qubits()) == [cirq.LineQubit(0)]


def test_a_measurement_key_becomes_a_classical_register():
    circuit = cirq.Circuit([cirq.H(Q[0]), cirq.measure(Q[0], Q[1], key="alpha")])
    mine = from_cirq(circuit)
    assert [(r.name, r.size) for r in mine.classical_registers] == [("alpha", 2)]

    restored = to_cirq(mine, sorted(circuit.all_qubits()))
    keys = [
        operation.gate.key
        for moment in restored
        for operation in moment.operations
        if isinstance(operation.gate, cirq.MeasurementGate)
    ]
    assert keys == ["alpha"]


def test_a_key_used_at_two_widths_is_refused():
    circuit = cirq.Circuit(
        [cirq.measure(Q[0], key="k"), cirq.measure(Q[1], Q[2], key="k")]
    )
    with pytest.raises(ConversionError, match="covers"):
        from_cirq(circuit)


def test_moments_are_left_flat_when_scheduling_would_rebuild_them():
    """An ordinary circuit gains nothing from recording its moments."""
    circuit = cirq.Circuit([cirq.H(Q[0]), cirq.H(Q[1]), cirq.CNOT(Q[0], Q[1])])
    mine = from_cirq(circuit)
    assert [mine[i].name for i in range(len(mine))] == ["h", "h", "cx"]


def test_authored_moments_are_recorded():
    """Deliberate separation is the case flattening would silently discard."""
    circuit = cirq.Circuit([cirq.Moment([cirq.H(Q[0])]), cirq.Moment([cirq.H(Q[1])])])
    # Cirq's own scheduling would merge these two into one.
    assert len(cirq.Circuit(op for m in circuit for op in m.operations)) == 1

    mine = from_cirq(circuit)
    assert [mine[i].name for i in range(len(mine))] == ["moment", "moment"]

    restored = to_cirq(mine, sorted(circuit.all_qubits()))
    assert len(restored) == 2


def test_moment_recording_can_be_forced_on():
    circuit = cirq.Circuit([cirq.H(Q[0]), cirq.H(Q[1])])
    mine = from_cirq(circuit, preserve_moments=True)
    assert [mine[i].name for i in range(len(mine))] == ["moment"]


def test_moment_recording_can_be_forced_off():
    circuit = cirq.Circuit([cirq.Moment([cirq.H(Q[0])]), cirq.Moment([cirq.H(Q[1])])])
    mine = from_cirq(circuit, preserve_moments=False)
    assert [mine[i].name for i in range(len(mine))] == ["h", "h"]


def test_a_moment_cannot_hold_two_gates_on_one_qubit():
    """Simultaneity is what a moment asserts, so an overlap is not a moment."""
    from qdk.circuit import QuantumCircuit

    circuit = QuantumCircuit(1)
    with pytest.raises(ValueError, match="both act on qubit 0"):
        with circuit.moment():
            circuit.h(0)
            circuit.x(0)


def test_a_large_circuit_converts():
    width, depth = 12, 500
    circuit = cirq.Circuit()
    line = cirq.LineQubit.range(width)
    for layer in range(depth):
        circuit.append(cirq.H(line[layer % width]))
        circuit.append(cirq.CNOT(line[layer % width], line[(layer + 1) % width]))

    mine = from_cirq(circuit)
    assert len(mine) == depth * 2
