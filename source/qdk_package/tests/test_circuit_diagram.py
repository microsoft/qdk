# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for converting a QuantumCircuit into the package's diagram type.

The diagram is a display format, not a program representation. These tests pin
both what the conversion preserves and where it is deliberately lossy, so that
the lossy points are visible rather than discovered later.
"""

import json
import warnings

import pytest

with warnings.catch_warnings():
    warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
    from qdk.circuit import QuantumCircuit


def grid(circuit: QuantumCircuit) -> list[list[dict]]:
    """Returns the diagram's component grid as plain data."""
    return json.loads(circuit.to_diagram().json())["componentGrid"]


def gates_by_column(circuit: QuantumCircuit) -> list[list[str]]:
    """Returns the gate name in each column, in column order."""
    return [[c["gate"] for c in col["components"]] for col in grid(circuit)]


def only_component(circuit: QuantumCircuit) -> dict:
    """Returns the single component of a single-operation circuit."""
    columns = grid(circuit)
    assert len(columns) == 1
    assert len(columns[0]["components"]) == 1
    return columns[0]["components"][0]


def test_diagram_reports_every_qubit() -> None:
    circuit = QuantumCircuit(3)
    circuit.h(0)

    qubits = json.loads(circuit.to_diagram().json())["qubits"]

    # Qubits with no operations still appear, so the diagram width matches the
    # circuit width.
    assert [q["id"] for q in qubits] == [0, 1, 2]


def test_disjoint_gates_share_a_column() -> None:
    circuit = QuantumCircuit(3)
    circuit.h(0)
    circuit.h(1)
    circuit.h(2)

    assert gates_by_column(circuit) == [["H", "H", "H"]]


def test_overlapping_gates_split_into_columns() -> None:
    circuit = QuantumCircuit(2)
    circuit.h(0)
    circuit.x(0)

    assert gates_by_column(circuit) == [["H"], ["X"]]


def test_two_qubit_gate_blocks_both_qubits() -> None:
    circuit = QuantumCircuit(3)
    circuit.cx(0, 1)
    circuit.h(1)
    circuit.h(2)

    # The H on qubit 1 collides with the CX, so it moves right. The H on qubit 2
    # is free to stay in the first column.
    assert gates_by_column(circuit) == [["X", "H"], ["H"]]


def test_a_gate_is_never_drawn_before_an_earlier_gate_on_its_wire() -> None:
    circuit = QuantumCircuit(3)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.ccx(0, 1, 2)
    circuit.s(2)

    columns = gates_by_column(circuit)

    # Qubit 2 is untouched until the Toffoli, so its first two columns are free.
    # The S still has to follow the Toffoli, because both act on qubit 2.
    assert columns == [["H"], ["X"], ["X"], ["S"]]


def test_program_order_is_preserved_on_each_wire() -> None:
    circuit = QuantumCircuit(2)
    circuit.cx(0, 1)
    circuit.h(0)
    circuit.x(1)

    # Both single-qubit gates follow the CX and neither collides with the other,
    # so they share the column after it.
    assert gates_by_column(circuit) == [["X"], ["H", "X"]]


def test_controlled_gate_draws_as_the_base_gate_with_a_control() -> None:
    circuit = QuantumCircuit(2)
    circuit.cx(0, 1)

    component = only_component(circuit)

    # The diagram draws a CNOT as an X with a control wire, so the name is the
    # base gate rather than this API's "cx".
    assert component["gate"] == "X"
    assert [c["qubit"] for c in component["controls"]] == [0]
    assert [t["qubit"] for t in component["targets"]] == [1]


def test_toffoli_has_two_controls() -> None:
    circuit = QuantumCircuit(3)
    circuit.ccx(0, 1, 2)

    component = only_component(circuit)

    assert component["gate"] == "X"
    assert [c["qubit"] for c in component["controls"]] == [0, 1]
    assert [t["qubit"] for t in component["targets"]] == [2]


def test_controlled_rotation_keeps_its_angle() -> None:
    circuit = QuantumCircuit(2)
    circuit.crz(0.25, 0, 1)

    component = only_component(circuit)

    assert component["gate"] == "Rz"
    assert component["args"] == ["0.25"]
    assert [c["qubit"] for c in component["controls"]] == [0]


def test_rotation_angle_is_drawn_as_an_argument() -> None:
    circuit = QuantumCircuit(1)
    circuit.rx(0.5, 0)

    component = only_component(circuit)

    assert component["gate"] == "Rx"
    assert component["args"] == ["0.5"]


def test_symbolic_angle_is_drawn_by_name() -> None:
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta, 0)

    component = only_component(circuit)

    assert component["args"] == ["theta"]


def test_measurement_results_number_per_qubit() -> None:
    circuit = QuantumCircuit(2, 2)
    circuit.measure(0, 0)
    circuit.measure(0, 1)
    circuit.measure(1, 0)

    components = [c for col in grid(circuit) for c in col["components"]]
    seen = [(c["qubits"][0]["qubit"], c["results"][0]["result"]) for c in components]

    # Result indices count per qubit, which is how the diagram addresses
    # results. Qubit 1's first measurement is result 0 on that qubit, not 2.
    assert sorted(seen) == [(0, 0), (0, 1), (1, 0)]


def test_measured_qubits_report_their_result_count() -> None:
    circuit = QuantumCircuit(2, 2)
    circuit.measure(0, 0)
    circuit.measure(0, 1)

    qubits = json.loads(circuit.to_diagram().json())["qubits"]

    assert [q.get("numResults", 0) for q in qubits] == [2, 0]


def test_measurement_records_the_target_clbit_in_args() -> None:
    circuit = QuantumCircuit(1, 4)
    circuit.measure(0, 3)

    component = only_component(circuit)

    # The diagram addresses results per qubit and has no classical-register
    # model, so the circuit's clbit index survives only as an argument string.
    assert component["args"] == ["c[3]"]
    assert component["results"][0]["result"] == 0


def test_control_flow_spans_the_full_width() -> None:
    circuit = QuantumCircuit(3, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)

    last = grid(circuit)[-1]["components"][0]

    # A control-flow construct can affect any qubit, so it occupies the whole
    # width rather than claiming to touch only the qubits in its body.
    assert last["gate"] == "if_else"
    assert [t["qubit"] for t in last["targets"]] == [0, 1, 2]


def test_control_flow_body_is_not_expanded() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)
        circuit.h(1)

    last = grid(circuit)[-1]["components"][0]

    # Bodies are stored as blocks in the circuit but are not nested into the
    # diagram, so the diagram shows a placeholder. The body is still reachable
    # through body_of on the circuit.
    assert last.get("children", []) == []
    assert len(circuit.body_of(len(circuit) - 1)) == 2


def test_str_renders_without_source_locations() -> None:
    circuit = QuantumCircuit(1)
    circuit.h(0)

    text = str(circuit)

    assert "H" in text
    assert "q_0" in text


def test_empty_circuit_converts() -> None:
    circuit = QuantumCircuit(2)

    assert grid(circuit) == []
    assert str(circuit)


@pytest.mark.parametrize(
    ("gate", "expected"),
    [
        ("i", "I"),
        ("x", "X"),
        ("y", "Y"),
        ("z", "Z"),
        ("h", "H"),
        ("s", "S"),
        ("t", "T"),
        ("sx", "SX"),
    ],
)
def test_gates_use_the_diagram_vocabulary(gate: str, expected: str) -> None:
    circuit = QuantumCircuit(1)
    getattr(circuit, gate)(0)

    assert gates_by_column(circuit) == [[expected]]


@pytest.mark.parametrize(("gate", "base"), [("sdg", "S"), ("tdg", "T"), ("sxdg", "SX")])
def test_adjoint_gates_are_flagged_rather_than_renamed(gate: str, base: str) -> None:
    circuit = QuantumCircuit(1)
    getattr(circuit, gate)(0)

    component = only_component(circuit)

    # The diagram marks an adjoint with a flag, so "sdg" becomes S with the flag
    # set rather than a box labelled "sdg".
    assert component["gate"] == base
    assert component["isAdjoint"] is True


def test_reset_draws_as_the_zero_state() -> None:
    circuit = QuantumCircuit(1)
    circuit.reset(0)

    component = only_component(circuit)

    assert component["kind"] == "ket"
    assert component["gate"] == "0"


def test_swap_has_two_targets_and_no_control() -> None:
    circuit = QuantumCircuit(2)
    circuit.swap(0, 1)

    component = only_component(circuit)

    assert component["gate"] == "SWAP"
    assert [t["qubit"] for t in component["targets"]] == [0, 1]
    assert component.get("controls", []) == []
