# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for saving and restoring a circuit.

The stated invariant is that save and restore is total and exact, so these
tests are written to try to break totality rather than to confirm the happy
path.
"""

import ctypes
import math
import os
import warnings
from pathlib import Path

import pytest

with warnings.catch_warnings():
    warnings.filterwarnings("ignore", message="qdk.circuit is experimental")
    from qdk.circuit import ClassicalRegister, QuantumCircuit, QuantumRegister


def restored(circuit: QuantumCircuit) -> QuantumCircuit:
    """Round-trips a circuit and asserts the saved form is stable."""
    data = circuit.to_bytes()
    assert isinstance(data, bytes)
    back = QuantumCircuit.from_bytes(data)
    assert back.to_bytes() == data, "saving a restored circuit produced different bytes"
    text = circuit.to_text()
    assert QuantumCircuit.from_text(text).to_bytes() == data
    return back


def test_complete_document_matches_native_and_wasm_fixtures() -> None:
    directory = Path(__file__).parents[2] / "compiler/qsc_snapshot/tests/fixtures"
    expected = (directory / "circuit.bin").read_bytes()
    text = (directory / "circuit.sexpr").read_text()
    circuit = QuantumCircuit(2, 1)
    circuit.h(0).cx(0, 1).measure(0, 0)
    circuit.global_phase = -0.0
    assert circuit.to_bytes() == expected
    assert circuit.to_text() == text
    assert QuantumCircuit.from_bytes(expected).to_text() == text


def test_graph_matches_native_and_wasm_fixtures() -> None:
    directory = Path(__file__).parents[2] / "compiler/qsc_snapshot/tests/fixtures"
    circuit = QuantumCircuit(2, 1)
    circuit.h(0).cx(0, 1).measure(0, 0)
    circuit.global_phase = -0.0
    graph = circuit.to_graph()
    assert graph.to_bytes() == (directory / "graph.bin").read_bytes()
    assert graph.to_text() == (directory / "graph.sexpr").read_text()


def views(circuit: QuantumCircuit) -> list[tuple]:
    """Returns each instruction as a comparable tuple."""
    return [
        (view.name, view.qubits, view.clbits, view.params)
        for view in (circuit[i] for i in range(len(circuit)))
    ]


def test_empty_circuit_round_trips() -> None:
    circuit = QuantumCircuit(3, 2)

    back = restored(circuit)

    assert back.num_qubits == 3
    assert back.num_clbits == 2
    assert len(back) == 0


def test_c_api_roundtrip_preserves_complete_python_document() -> None:
    library_path = os.environ.get("QDK_CIRCUIT_CAPI_LIBRARY")
    if not library_path:
        pytest.skip(
            "set QDK_CIRCUIT_CAPI_LIBRARY to the freshly built standalone library"
        )

    class Bytes(ctypes.Structure):
        _fields_ = [("data", ctypes.c_void_p), ("len", ctypes.c_size_t)]

    library = ctypes.CDLL(library_path)
    handle_slot = ctypes.POINTER(ctypes.c_void_p)
    library.qdk_circuit_import.argtypes = [
        ctypes.c_uint32,
        ctypes.c_void_p,
        ctypes.c_size_t,
        handle_slot,
        handle_slot,
    ]
    library.qdk_circuit_import.restype = ctypes.c_uint32
    library.qdk_circuit_export.argtypes = [
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.POINTER(Bytes),
        handle_slot,
    ]
    library.qdk_circuit_export.restype = ctypes.c_uint32
    library.qdk_circuit_free.argtypes = [handle_slot, handle_slot]
    library.qdk_circuit_free.restype = ctypes.c_uint32
    library.qdk_diagnostic_message.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(Bytes),
        handle_slot,
    ]
    library.qdk_diagnostic_message.restype = ctypes.c_uint32
    library.qdk_diagnostic_free.argtypes = [handle_slot]
    library.qdk_diagnostic_free.restype = None
    library.qdk_bytes_free.argtypes = [ctypes.POINTER(Bytes)]
    library.qdk_bytes_free.restype = None
    error = ctypes.c_void_p()
    native = ctypes.c_void_p()
    output = Bytes()

    def check(status: int) -> None:
        if status == 0:
            return
        message = Bytes()
        try:
            diagnostic_status = library.qdk_diagnostic_message(
                error, ctypes.byref(message), None
            )
            assert (
                diagnostic_status == 0
            ), f"cannot read diagnostic for C status {status}"
            pytest.fail(ctypes.string_at(message.data, message.len).decode("utf-8"))
        finally:
            library.qdk_bytes_free(ctypes.byref(message))
            library.qdk_diagnostic_free(ctypes.byref(error))

    circuit = QuantumCircuit.from_registers(
        QuantumRegister("q", 2), ClassicalRegister("readouts", 1)
    )
    circuit.global_phase = math.inf
    theta = circuit.parameter("theta")
    circuit.h(1).cx(1, 0).measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.rx(theta * 2, 1)
    circuit.rz(-math.inf, 0).ry(-0.0, 1)
    document = circuit.to_bytes()

    try:
        check(
            library.qdk_circuit_import(
                7, document, len(document), ctypes.byref(native), ctypes.byref(error)
            )
        )
        check(
            library.qdk_circuit_export(
                native, 7, 0, ctypes.byref(output), ctypes.byref(error)
            )
        )
        roundtrip = ctypes.string_at(output.data, output.len)
        assert roundtrip == document
        assert QuantumCircuit.from_bytes(roundtrip).to_bytes() == document
    finally:
        library.qdk_bytes_free(ctypes.byref(output))
        check(library.qdk_circuit_free(ctypes.byref(native), ctypes.byref(error)))
        library.qdk_diagnostic_free(ctypes.byref(error))


def test_gates_and_operands_round_trip() -> None:
    circuit = QuantumCircuit(3, 2)
    circuit.h(0).cx(0, 1).ccx(0, 1, 2).swap(1, 2)
    circuit.measure(0, 0)
    circuit.reset(2)
    circuit.barrier()

    back = restored(circuit)

    assert views(back) == views(circuit)


def test_rotation_angles_round_trip() -> None:
    circuit = QuantumCircuit(2)
    circuit.rx(0.25, 0).ry(-1.5, 1).rz(math.pi, 0)
    circuit.crz(0.125, 0, 1)
    circuit.rxx(2.0, 0, 1)

    back = restored(circuit)

    assert views(back) == views(circuit)


@pytest.mark.parametrize("angle", [float("inf"), float("-inf")])
def test_infinite_angles_round_trip(angle: float) -> None:
    circuit = QuantumCircuit(1)
    circuit.rx(angle, 0)

    back = restored(circuit)

    assert back[0].params == [angle]


def test_nan_angle_round_trips() -> None:
    circuit = QuantumCircuit(1)
    circuit.rx(float("nan"), 0)

    back = QuantumCircuit.from_text(circuit.to_text())

    value = back[0].params[0]
    assert isinstance(value, float) and math.isnan(value)


def test_extreme_angles_keep_their_exact_value() -> None:
    values = [0.0, -0.0, 5e-324, 1.7976931348623157e308, 1 / 3]
    circuit = QuantumCircuit(1)
    for value in values:
        circuit.rz(value, 0)

    back = restored(circuit)

    stored = [back[i].params[0] for i in range(len(values))]
    assert [math.copysign(1.0, v) for v in stored] == [
        math.copysign(1.0, v) for v in values
    ], "signed zero did not survive"
    assert stored == values


def test_parameters_round_trip() -> None:
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.parameter("unused")
    circuit.rx(theta, 0)

    back = restored(circuit)

    assert [p.name for p in back.parameters] == ["theta"]
    assert back[0].params == [theta]
    # A declared but unreferenced parameter is not reported by `parameters`,
    # so the saved form is where its survival is observable.
    assert '(parameters (list "theta" "unused"))' in back.to_text()


def test_a_restored_circuit_can_still_bind_parameters() -> None:
    circuit = QuantumCircuit(1)
    theta = circuit.parameter("theta")
    circuit.rx(theta, 0)

    back = restored(circuit)
    bound = back.assign_parameters([(theta, 0.5)])

    assert bound[0].params == [0.5]


def test_registers_round_trip() -> None:
    circuit = QuantumCircuit.from_registers(
        QuantumRegister("control", 2), QuantumRegister("target", 3), clbits=1
    )
    circuit.h(0)

    back = restored(circuit)

    # Registers belong to this surface rather than to the underlying store, so
    # they are the part most likely to be dropped on the way through.
    assert [(r.name, r.size) for r in back.quantum_registers] == [
        ("control", 2),
        ("target", 3),
    ]
    assert back.num_qubits == 5
    assert back.num_clbits == 1


def test_control_flow_round_trips() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1) as body:
        body.x(1)

    back = restored(circuit)

    assert views(back) == views(circuit)
    assert [v.name for v in back.body_of(len(back) - 1)] == ["x"]


def test_nested_control_flow_round_trips() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.box_("outer") as outer:
        outer.h(1)
        with outer.box_("inner") as inner:
            inner.x(1)

    back = restored(circuit)

    assert views(back) == views(circuit)
    outer_body = back.body_of(len(back) - 1)
    assert [v.name for v in outer_body] == ["h", "box"]


def test_if_else_round_trips() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1) as body:
        body.x(1)
    with circuit.if_test(0, 0) as other:
        other.h(1)

    back = restored(circuit)

    assert views(back) == views(circuit)


def test_loops_round_trip() -> None:
    circuit = QuantumCircuit(1, 1)
    circuit.measure(0, 0)
    with circuit.while_loop(0, 1) as body:
        body.x(0)
    with circuit.for_loop([0, 1, 2]) as index:
        circuit.rx(index, 0)

    back = restored(circuit)

    assert views(back) == views(circuit)


def test_every_control_flow_construct_round_trips() -> None:
    # The requirement names seven constructs, so all seven are built together
    # rather than one per test, which is what makes this evidence for it.
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)

    conditional = circuit.if_test(0, 1)
    with conditional:
        circuit.x(1)
    with conditional.otherwise():
        circuit.z(1)

    with circuit.while_loop(0, 1):
        circuit.h(0)
        circuit.break_loop()

    with circuit.for_loop([0, 1], runtime=True):
        circuit.continue_loop()

    with circuit.switch(0) as selected:
        with selected.case(0, 1):
            circuit.x(0)
        with selected.default():
            circuit.z(0)

    with circuit.box_("prep"):
        circuit.h(1)

    names = [circuit[i].name for i in range(len(circuit))]
    assert names == ["measure_z", "if_else", "while", "for", "switch", "box"]

    back = restored(circuit)

    assert views(back) == views(circuit)
    assert [back[i].name for i in range(len(back))] == names
    assert [v.name for v in back.body_of(2)] == ["h", "break"]
    assert [v.name for v in back.body_of(3)] == ["continue"]


def test_switch_cases_and_default_round_trip() -> None:
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with selected.case(0):
            circuit.x(0)
        with selected.case(2, 3):
            circuit.y(0)
        with selected.default():
            circuit.z(0)

    back = restored(circuit)

    assert back[0].cases == [[0], [2, 3]]
    assert back[0].default_block == back[0].blocks[-1]
    bodies = [[v.name for v in back.block(block)] for block in back[0].blocks]
    assert bodies == [["x"], ["y"], ["z"]]


def test_a_delay_round_trips_with_its_unit() -> None:
    circuit = QuantumCircuit(1)
    circuit.delay(100.0, 0, "ns")

    back = restored(circuit)

    assert "(unit ns)" in back.to_text()
    assert back.to_bytes() == circuit.to_bytes()


def test_text_form_is_a_readable_sexpression_envelope() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.h(0).cx(0, 1)

    document = circuit.to_text()

    assert document.startswith("QDKCIRCT(\n(qdk 1 ")
    assert "(version 1)" in document
    assert "(qubitCount 2)" in document
    assert "(standard h)" in document and "(standard cx)" in document


def tampered(circuit: QuantumCircuit, before: str, after: str) -> str:
    document = circuit.to_text()
    assert before in document, "test must change the snapshot"
    return document.replace(before, after)


def test_binary_form_has_a_distinct_non_json_header() -> None:
    data = QuantumCircuit(1).to_bytes()
    assert isinstance(data, bytes) and data.startswith(b"QDKCIRCT\x01\n")


def test_a_different_schema_version_is_rejected() -> None:
    circuit = QuantumCircuit(1)
    document = tampered(circuit, "(version 1)", "(version 99)")

    with pytest.raises(ValueError, match="schema version"):
        QuantumCircuit.from_text(document)


def test_text_that_is_not_a_circuit_is_rejected() -> None:
    with pytest.raises(ValueError):
        QuantumCircuit.from_text("not a snapshot")


def test_a_self_referential_block_is_rejected_before_export() -> None:
    circuit = QuantumCircuit(1)
    with circuit.box_("outer"):
        circuit.h(0)
    document = tampered(
        circuit,
        "(standard h)",
        "(control (box (box (body 0) (duration (none)) (label (none)))))",
    ).replace("(qubits (list 0))", "(qubits (list))")

    with pytest.raises(ValueError, match="block 0 is cyclic"):
        QuantumCircuit.from_text(document)


def test_a_sparse_moment_restores_at_maximum_declared_width() -> None:
    circuit = QuantumCircuit(1)
    with circuit.box_("outer"):
        circuit.h(0)
    document = tampered(
        circuit,
        '(box (box (body 0) (duration (none)) (label (some "outer"))))',
        "(moment 0)",
    ).replace("(qubitCount 1)", "(qubitCount 4294967295)")

    restored = QuantumCircuit.from_text(document)
    assert restored.num_qubits == 4294967295
    assert restored[0].name == "moment"
    assert restored.block(0)[0].qubits == [0]
    assert QuantumCircuit.from_bytes(restored.to_bytes()).to_text() == document


def test_an_operand_outside_the_circuit_is_rejected() -> None:
    circuit = QuantumCircuit(1)
    circuit.h(0)
    document = tampered(circuit, "(qubits (list 0))", "(qubits (list 7))")

    # Restoring applies the same checks as building, so a saved circuit cannot
    # smuggle in something that could not have been built.
    with pytest.raises(ValueError):
        QuantumCircuit.from_text(document)


def test_a_register_outside_the_circuit_is_rejected() -> None:
    circuit = QuantumCircuit.from_registers(QuantumRegister("q", 2))
    document = tampered(circuit, "(size 2)", "(size 50)")

    # Restoring checks registers the same way declaring them does, so a saved
    # circuit cannot name qubits the circuit does not have.
    with pytest.raises(ValueError, match="circuit has 2"):
        QuantumCircuit.from_text(document)


def test_overlapping_registers_are_rejected() -> None:
    circuit = QuantumCircuit.from_registers(
        QuantumRegister("a", 2), QuantumRegister("b", 2)
    )
    document = tampered(circuit, "(start 2)", "(start 1)")

    with pytest.raises(ValueError, match="overlaps"):
        QuantumCircuit.from_text(document)


def test_duplicate_register_names_are_rejected() -> None:
    circuit = QuantumCircuit.from_registers(
        QuantumRegister("a", 2), QuantumRegister("b", 2)
    )
    document = tampered(circuit, '(name "b")', '(name "a")')

    with pytest.raises(ValueError, match="duplicate register"):
        QuantumCircuit.from_text(document)


def test_saving_inside_an_open_block_is_refused() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)

    with circuit.if_test(0, 1) as body:
        body.x(1)
        # The block is still open here, so the circuit is half-built and there
        # is nothing meaningful to restore.
        with pytest.raises(ValueError, match="block is open"):
            circuit.to_text()

    assert circuit.to_text()


def test_a_restored_circuit_can_be_extended() -> None:
    circuit = QuantumCircuit(2)
    circuit.h(0)

    back = restored(circuit)
    back.cx(0, 1)

    assert [v.name for v in (back[i] for i in range(len(back)))] == ["h", "cx"]


def test_a_restored_circuit_draws_the_same() -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.h(0).cx(0, 1)
    circuit.measure(0, 0)

    assert str(restored(circuit)) == str(circuit)
