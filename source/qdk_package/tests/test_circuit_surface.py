# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import InstructionView, QuantumCircuit, QuantumRegister  # noqa: E402


def test_construct_by_integer_index():
    circuit = QuantumCircuit(2)
    circuit.h(0)
    circuit.cx(0, 1)

    assert len(circuit) == 2
    assert circuit.num_qubits == 2
    assert [circuit[i].name for i in range(len(circuit))] == ["h", "cx"]
    assert circuit[1].qubits == [0, 1]


def test_gate_methods_chain():
    circuit = QuantumCircuit(3)
    circuit.h(0).cx(0, 1).cx(1, 2).z(2)

    assert [circuit[i].name for i in range(len(circuit))] == ["h", "cx", "cx", "z"]


def test_construct_by_register_position():
    alice = QuantumRegister("alice", 2)
    bob = QuantumRegister("bob", 3)
    circuit = QuantumCircuit.from_registers(alice, bob)

    assert circuit.num_qubits == 5
    placed = {r.name: r for r in circuit.quantum_registers}
    assert placed["alice"].start == 0
    assert placed["bob"].start == 2

    circuit.h(placed["alice"][0])
    circuit.cx(placed["alice"][1], placed["bob"][0])

    assert circuit[0].qubits == [0]
    assert circuit[1].qubits == [1, 2]


def test_registers_do_not_overlap():
    first = QuantumRegister("q", 2)
    second = QuantumRegister("r", 2)
    circuit = QuantumCircuit.from_registers(first, second)

    placed = {r.name: r for r in circuit.quantum_registers}
    q_indices = {placed["q"][i] for i in range(len(placed["q"]))}
    r_indices = {placed["r"][i] for i in range(len(placed["r"]))}
    assert q_indices.isdisjoint(r_indices)


def test_duplicate_register_names_are_rejected():
    with pytest.raises(ValueError, match="duplicate register name"):
        QuantumCircuit.from_registers(QuantumRegister("q", 1), QuantumRegister("q", 1))


def test_register_index_out_of_range_is_rejected():
    register = QuantumRegister("q", 2)
    with pytest.raises(IndexError, match="outside register 'q'"):
        register[5]


def test_negative_indices_address_from_the_end():
    register = QuantumRegister("q", 3)
    assert register[-1] == 2

    circuit = QuantumCircuit(3)
    circuit.h(-1)
    assert circuit[0].qubits == [2]
    assert circuit[-1].name == "h"


def test_out_of_range_qubit_is_rejected():
    circuit = QuantumCircuit(2)
    with pytest.raises(IndexError, match="outside the circuit's 2 qubits"):
        circuit.h(7)


def test_instruction_handle_is_a_view_that_cannot_mutate_the_circuit():
    circuit = QuantumCircuit(2)
    circuit.cx(0, 1)

    view = circuit[0]
    assert isinstance(view, InstructionView)

    # The handle is frozen: it exposes stored data without offering a way to
    # write back through it.
    with pytest.raises(AttributeError):
        view.name = "h"

    assert circuit[0].name == "cx"


def test_mutating_a_list_from_a_handle_does_not_affect_the_circuit():
    circuit = QuantumCircuit(2)
    circuit.cx(0, 1)

    operands = circuit[0].qubits
    operands.append(99)

    assert circuit[0].qubits == [
        0,
        1,
    ], "the view must hand out a copy, not internal state"


def test_measurement_and_reset():
    circuit = QuantumCircuit(1, 1)
    circuit.h(0)
    circuit.measure(0, 0)
    circuit.reset(0)

    assert [circuit[i].name for i in range(len(circuit))] == ["h", "measure_z", "reset"]
    assert circuit[1].clbits == [0]


def test_measure_rejects_out_of_range_clbit():
    circuit = QuantumCircuit(1, 1)
    with pytest.raises(IndexError, match="classical bits"):
        circuit.measure(0, 4)


def test_barrier_defaults_to_every_qubit():
    circuit = QuantumCircuit(3)
    circuit.barrier()
    assert circuit[0].qubits == [0, 1, 2]

    circuit.barrier([0, 2])
    assert circuit[1].qubits == [0, 2]


def test_delay_records_duration_and_unit():
    circuit = QuantumCircuit(1)
    circuit.delay(2.5, 0, "ns")
    assert circuit[0].name == "delay"
    assert circuit[0].qubits == [0]


def test_delay_measures_in_target_time_steps_by_default():
    circuit = QuantumCircuit(1)
    circuit.delay(2.5, 0)
    assert "(unit dt)" in circuit.to_text()


def test_delay_refuses_a_unit_it_cannot_write():
    circuit = QuantumCircuit(1)
    with pytest.raises(ValueError, match="is not a time unit"):
        circuit.delay(1.0, 0, "fortnights")
    assert len(circuit) == 0


@pytest.mark.parametrize("duration", [-1.0, float("inf"), float("nan")])
def test_delay_refuses_a_duration_nothing_can_wait_out(duration):
    circuit = QuantumCircuit(1)
    with pytest.raises(ValueError, match="finite, non-negative"):
        circuit.delay(duration, 0, "ns")
    assert len(circuit) == 0


def test_reserve_does_not_change_observable_content():
    circuit = QuantumCircuit(2)
    circuit.reserve(1000)
    assert len(circuit) == 0
    circuit.h(0)
    assert len(circuit) == 1


def test_repr_reports_shape():
    circuit = QuantumCircuit(2, 1)
    circuit.h(0)
    assert "QuantumCircuit(2, 1)" in repr(circuit)
    assert "1 instructions" in repr(circuit)


if __name__ == "__main__":
    pytest.main([__file__])
