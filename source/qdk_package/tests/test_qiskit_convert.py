# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Structural conversion between Qiskit circuits and qdk.circuit.

The unitary comparisons here use exact equality rather than ``Operator.equiv``,
which ignores global phase. Phase is the thing most easily got wrong when a gate
is rewritten into others, and it is observable as soon as a circuit is used as a
controlled subroutine, so a check that hides it would not be worth running.
"""

import inspect
import math
import warnings

import numpy as np
import pytest

qiskit = pytest.importorskip("qiskit")

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qiskit import ClassicalRegister, QuantumCircuit, QuantumRegister  # noqa: E402
from qiskit.circuit import Parameter  # noqa: E402
from qiskit.circuit.classical import expr  # noqa: E402
from qiskit.circuit.library import standard_gates as sg  # noqa: E402
from qiskit.quantum_info import Operator  # noqa: E402

from qdk.qiskit import ConversionError, from_qiskit, to_qiskit  # noqa: E402


def assert_same_unitary(original: QuantumCircuit) -> None:
    """Round-trips a circuit and asserts the unitary is unchanged, phase included."""
    restored = to_qiskit(from_qiskit(original))
    expected = Operator(original).data
    actual = Operator(restored).data
    assert np.allclose(expected, actual), (
        f"unitary changed\nexpected:\n{np.round(expected, 6)}\n"
        f"actual:\n{np.round(actual, 6)}"
    )


def standard_gate_instances():
    """Every standard gate Qiskit ships, with arbitrary angles."""
    angles = [0.6, 0.35, 1.2, 0.8]
    for name, obj in sorted(vars(sg).items()):
        if not (inspect.isclass(obj) and name.endswith("Gate")):
            continue
        try:
            signature = inspect.signature(obj.__init__)
            required = sum(
                1
                for p in list(signature.parameters.values())[1:]
                if p.default is inspect.Parameter.empty
                and p.kind == p.POSITIONAL_OR_KEYWORD
            )
            gate = obj(*angles[:required])
        except Exception:  # noqa: BLE001 - a gate we cannot build is not under test
            continue
        if gate.num_qubits <= 5:
            yield gate


@pytest.mark.parametrize("gate", list(standard_gate_instances()), ids=lambda g: g.name)
def test_every_standard_gate_survives_a_round_trip(gate):
    circuit = QuantumCircuit(gate.num_qubits)
    circuit.append(gate, range(gate.num_qubits))
    assert_same_unitary(circuit)


def test_conversion_does_not_go_through_text(monkeypatch):
    """The conversion path must not serialize, at any depth."""
    import qiskit.qasm2
    import qiskit.qasm3

    def refuse(*args, **kwargs):
        raise AssertionError("conversion serialized the circuit to text")

    for module in (qiskit.qasm2, qiskit.qasm3):
        for name in ("dumps", "dump", "loads", "load"):
            if hasattr(module, name):
                monkeypatch.setattr(module, name, refuse)

    circuit = QuantumCircuit(3, 3)
    circuit.h(0)
    circuit.ccx(0, 1, 2)
    circuit.measure(range(3), range(3))
    to_qiskit(from_qiskit(circuit))


def test_global_phase_survives():
    circuit = QuantumCircuit(1)
    circuit.global_phase = 1.25
    circuit.h(0)
    assert from_qiskit(circuit).global_phase == pytest.approx(1.25)
    assert_same_unitary(circuit)


def test_a_transpiled_circuit_survives():
    from qiskit import transpile

    circuit = QuantumCircuit(3)
    circuit.h(0)
    circuit.ccx(0, 1, 2)
    circuit.rz(0.3, 2)
    for basis in (["u", "cx"], ["rz", "sx", "x", "cx"], ["rz", "ry", "rx", "cz"]):
        transpiled = transpile(circuit, basis_gates=basis, optimization_level=1)
        # Transpiling leaves a non-zero global phase, which is exactly the part
        # a converter without somewhere to put it would drop.
        assert transpiled.global_phase != 0
        assert_same_unitary(transpiled)


def test_a_library_circuit_survives():
    from qiskit.synthesis.qft import synth_qft_full

    assert_same_unitary(synth_qft_full(3))


def test_registers_round_trip():
    alice = QuantumRegister(2, "alice")
    bob = QuantumRegister(3, "bob")
    readout = ClassicalRegister(2, "readout")
    circuit = QuantumCircuit(alice, bob, readout)
    circuit.h(alice[0])
    circuit.cx(alice[1], bob[0])
    circuit.measure(alice[0], readout[0])

    mine = from_qiskit(circuit)
    assert [(r.name, r.size) for r in mine.quantum_registers] == [
        ("alice", 2),
        ("bob", 3),
    ]
    assert [(r.name, r.size) for r in mine.classical_registers] == [("readout", 2)]

    back = to_qiskit(mine)
    assert [(r.name, r.size) for r in back.qregs] == [("alice", 2), ("bob", 3)]
    assert [(r.name, r.size) for r in back.cregs] == [("readout", 2)]


def test_parameters_round_trip():
    theta = Parameter("theta")
    circuit = QuantumCircuit(1)
    circuit.rx(theta, 0)

    mine = from_qiskit(circuit)
    assert [p.name for p in mine.parameters] == ["theta"]

    bound = mine.assign_parameters([(mine.parameters[0], 0.5)])
    assert to_qiskit(bound).data[0].operation.params == [0.5]


def test_a_parameter_expression_is_refused_rather_than_approximated():
    theta = Parameter("theta")
    circuit = QuantumCircuit(1)
    circuit.rx(2 * theta, 0)

    with pytest.raises(ConversionError, match="only a plain parameter"):
        from_qiskit(circuit)


def test_a_delay_is_refused_because_the_converter_does_not_carry_it_yet():
    circuit = QuantumCircuit(1)
    circuit.delay(100, 0, unit="ns")

    with pytest.raises(ConversionError, match="delay has no conversion"):
        from_qiskit(circuit)


def test_overlapping_registers_are_refused():
    shared = QuantumRegister(2, "shared")
    circuit = QuantumCircuit(shared)
    circuit.add_register(QuantumRegister(bits=list(shared), name="alias"))

    with pytest.raises(ConversionError, match="contiguous"):
        from_qiskit(circuit)


def test_measurement_and_reset_survive():
    circuit = QuantumCircuit(2, 2)
    circuit.h(0)
    circuit.reset(1)
    circuit.measure(0, 0)
    circuit.measure(1, 1)

    back = to_qiskit(from_qiskit(circuit))
    assert [i.operation.name for i in back.data] == [
        "h",
        "reset",
        "measure",
        "measure",
    ]


def test_barrier_survives():
    circuit = QuantumCircuit(3)
    circuit.h(0)
    circuit.barrier([0, 1])

    back = to_qiskit(from_qiskit(circuit))
    names = [i.operation.name for i in back.data]
    assert names == ["h", "barrier"]
    assert len(back.data[1].qubits) == 2


def test_a_large_circuit_converts():
    """Scale is the reason this path exists, so it is exercised rather than assumed."""
    width = 20
    depth = 2000
    circuit = QuantumCircuit(width)
    for layer in range(depth):
        circuit.h(layer % width)
        circuit.cx(layer % width, (layer + 1) % width)

    mine = from_qiskit(circuit)
    assert len(mine) == depth * 2
    assert len(to_qiskit(mine).data) == depth * 2


def test_a_single_bit_conditional_converts():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(2, "q"), creg)
    circuit.measure(0, 0)
    with circuit.if_test((creg[0], 1)):
        circuit.x(1)

    mine = from_qiskit(circuit)
    assert [mine[i].name for i in range(len(mine))] == ["measure_z", "if_else"]
    assert [v.name for v in mine.body_of(1)] == ["x"]


def test_a_conditional_on_a_whole_register_converts():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(2, "q"), creg)
    circuit.measure(0, 0)
    with circuit.if_test((creg, 3)) as otherwise:
        circuit.y(1)
    with otherwise:
        circuit.z(1)

    mine = from_qiskit(circuit)
    assert [mine[i].name for i in range(len(mine))] == ["measure_z", "if_else"]
    # body_of reports every block of the construct, so both branches appear.
    assert [v.name for v in mine.body_of(1)] == ["y", "z"]


def test_a_modern_expression_condition_converts():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.if_test(expr.equal(creg, 2)):
        circuit.h(0)

    mine = from_qiskit(circuit)
    assert mine[0].name == "if_else"
    assert [v.name for v in mine.body_of(0)] == ["h"]


def test_a_while_loop_converts():
    creg = ClassicalRegister(1, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.while_loop((creg[0], 0)):
        circuit.x(0)
        circuit.measure(0, 0)

    mine = from_qiskit(circuit)
    assert mine[0].name == "while"
    assert [v.name for v in mine.body_of(0)] == ["x", "measure_z"]


def test_a_for_loop_carries_its_index_into_the_body():
    circuit = QuantumCircuit(1)
    with circuit.for_loop(range(3)) as index:
        circuit.rx(index, 0)
        circuit.ry(0.5, 0)

    mine = from_qiskit(circuit)
    body = mine.body_of(0)
    assert [v.name for v in body] == ["rx", "ry"]
    # The index reaches the gate as a symbol; a literal here would mean the
    # loop had been flattened and its values silently discarded.
    assert not isinstance(body[0].params[0], float)
    assert body[1].params == [0.5]


def test_a_box_converts():
    circuit = QuantumCircuit(1)
    with circuit.box():
        circuit.h(0)

    mine = from_qiskit(circuit)
    assert mine[0].name == "box"
    assert [v.name for v in mine.body_of(0)] == ["h"]


def test_nested_control_flow_converts():
    creg = ClassicalRegister(1, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.for_loop(range(2)):
        with circuit.if_test((creg[0], 1)):
            circuit.x(0)

    mine = from_qiskit(circuit)
    assert mine[0].name == "for"
    assert [v.name for v in mine.body_of(0)] == ["if_else"]


def test_a_switch_is_refused():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.switch(creg) as case:
        with case(0):
            circuit.x(0)

    with pytest.raises(ConversionError, match="switch_case"):
        from_qiskit(circuit)


def test_a_compound_condition_is_refused():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.if_test(expr.logic_and(expr.equal(creg, 1), expr.equal(creg, 2))):
        circuit.x(0)

    with pytest.raises(ConversionError, match="beyond a single equality"):
        from_qiskit(circuit)


def test_a_body_with_its_own_phase_is_refused():
    creg = ClassicalRegister(1, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.if_test((creg[0], 1)):
        circuit.h(0)
    circuit.data[-1].operation.params[0].global_phase = 0.5

    with pytest.raises(ConversionError, match="body carries its own global phase"):
        from_qiskit(circuit)


def test_a_register_value_that_cannot_fit_is_refused():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.if_test((creg, 7)):
        circuit.x(0)

    with pytest.raises(ValueError, match="can never equal"):
        from_qiskit(circuit)


def describe(circuit: QuantumCircuit, depth: int = 0):
    """Names every instruction, descending into control-flow bodies."""
    out = []
    for instruction in circuit.data:
        out.append("  " * depth + instruction.operation.name)
        for param in instruction.operation.params:
            if isinstance(param, QuantumCircuit):
                out += describe(param, depth + 1)
    return out


def assert_structure_survives(original: QuantumCircuit) -> None:
    restored = to_qiskit(from_qiskit(original))
    assert describe(restored) == describe(original)


def test_a_conditional_survives_a_round_trip():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(2, "q"), creg)
    circuit.measure(0, 0)
    with circuit.if_test((creg[0], 1)):
        circuit.x(1)
    assert_structure_survives(circuit)


def test_a_conditional_with_an_alternative_survives_a_round_trip():
    creg = ClassicalRegister(2, "c")
    circuit = QuantumCircuit(QuantumRegister(2, "q"), creg)
    circuit.measure(0, 0)
    with circuit.if_test((creg, 3)) as otherwise:
        circuit.y(1)
    with otherwise:
        circuit.z(1)
    assert_structure_survives(circuit)


def test_a_while_loop_survives_a_round_trip():
    creg = ClassicalRegister(1, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.while_loop((creg[0], 0)):
        circuit.x(0)
        circuit.measure(0, 0)
    assert_structure_survives(circuit)


def test_a_box_survives_a_round_trip():
    circuit = QuantumCircuit(1)
    with circuit.box():
        circuit.h(0)
    assert_structure_survives(circuit)


def test_nested_control_flow_survives_a_round_trip():
    creg = ClassicalRegister(1, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.for_loop(range(2)):
        with circuit.if_test((creg[0], 1)):
            circuit.x(0)
    assert_structure_survives(circuit)


@pytest.mark.parametrize("value", [0, 1, 5, 7])
def test_a_register_condition_keeps_its_value(value):
    """The value has to survive, not merely the shape of the condition.

    A register test is stored as one equality per bit, so a converter that
    rebuilt the shape without reading the bits back would pass a structural
    comparison while silently branching on the wrong number.
    """
    creg = ClassicalRegister(3, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.if_test((creg, value)):
        circuit.x(0)

    restored = to_qiskit(from_qiskit(circuit))
    subject, restored_value = restored.data[0].operation.condition
    assert subject.name == "c"
    assert restored_value == value


def test_a_single_bit_condition_keeps_its_bit():
    creg = ClassicalRegister(3, "c")
    circuit = QuantumCircuit(QuantumRegister(1, "q"), creg)
    with circuit.if_test((creg[2], 1)):
        circuit.x(0)

    restored = to_qiskit(from_qiskit(circuit))
    subject, value = restored.data[0].operation.condition
    assert restored.find_bit(subject).index == 2
    assert value == 1


def test_a_for_loop_keeps_its_values_and_its_index():
    circuit = QuantumCircuit(1)
    with circuit.for_loop(range(4)) as index:
        circuit.rx(index, 0)

    restored = to_qiskit(from_qiskit(circuit))
    indexset, loop_parameter, body = restored.data[0].operation.params
    assert list(indexset) == [0, 1, 2, 3]
    # The body's angle must be the loop's own parameter; anything else would
    # mean the index had been detached from the loop that supplies it.
    assert body.data[0].operation.params[0] == loop_parameter


def test_a_parameter_survives_a_round_trip_in_both_directions():
    theta = Parameter("theta")
    circuit = QuantumCircuit(1)
    circuit.rx(theta, 0)

    restored = to_qiskit(from_qiskit(circuit))
    assert [p.name for p in restored.parameters] == ["theta"]
    assert restored.data[0].operation.params[0].name == "theta"
