# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for handing a circuit to the rest of the stack.

The compilation and conversion work happens in Rust and is tested there. What
these tests cover is the part a Rust test cannot: that a Python user reaches it,
and that a refusal arrives naming what was wrong rather than flattened.
"""

import warnings
from pathlib import Path

import pytest

from qdk import TargetProfile

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import QuantumCircuit, QuantumRegister  # noqa: E402


def bell() -> QuantumCircuit:
    """Two entangled qubits, both measured."""
    circuit = QuantumCircuit(2, 2)
    circuit.h(0)
    circuit.cx(0, 1)
    circuit.measure(0, 0)
    circuit.measure(1, 1)
    return circuit


def measured_conditional() -> QuantumCircuit:
    """A measurement that decides whether a later gate runs."""
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)
    return circuit


def feedback_circuit() -> QuantumCircuit:
    circuit = QuantumCircuit(2, 2)
    circuit.h(0)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)
    circuit.measure(1, 1)
    return circuit


QIR_FIXTURES = (
    Path(__file__).resolve().parents[2] / "circuit_capi" / "tests" / "fixtures"
)


def test_adaptive_qir_matches_complete_snapshot() -> None:
    actual = feedback_circuit().to_qir(TargetProfile.Adaptive).encode("utf-8")

    assert actual == (QIR_FIXTURES / "adaptive.ll").read_bytes()


def test_adaptive_ri_qir_matches_complete_snapshot() -> None:
    actual = feedback_circuit().to_qir(TargetProfile.Adaptive_RI).encode("utf-8")

    assert actual == (QIR_FIXTURES / "adaptive_ri.ll").read_bytes()


def names(circuit: QuantumCircuit) -> list[str]:
    return [circuit[i].name for i in range(len(circuit))]


def test_a_static_circuit_compiles_to_base_profile_qir() -> None:
    qir = bell().to_qir(TargetProfile.Base)

    assert "define i64 @ENTRYPOINT__main()" in qir
    assert "__quantum__qis__h__body" in qir
    assert "__quantum__qis__cx__body" in qir
    assert "base_profile" in qir


def test_an_adaptive_circuit_compiles_to_a_branch() -> None:
    qir = measured_conditional().to_qir(TargetProfile.Adaptive_RI)

    assert "__quantum__rt__read_result" in qir
    assert "br i1" in qir, f"a conditional should branch, got {qir}"


def test_a_conditional_on_the_base_profile_names_the_missing_capability() -> None:
    with pytest.raises(ValueError) as caught:
        feedback_circuit().to_qir(TargetProfile.Base)

    message = str(caught.value)
    assert "'if'" in message, message
    assert "Adaptive" in message, message


def test_an_unbound_parameter_is_named_rather_than_compiled() -> None:
    circuit = QuantumCircuit(1)
    circuit.rx(circuit.parameter("theta"), 0)

    with pytest.raises(ValueError) as caught:
        circuit.to_qir(TargetProfile.Base)

    assert "theta" in str(caught.value)


def test_a_gate_with_no_qir_intrinsic_is_decomposed_rather_than_refused() -> None:
    circuit = QuantumCircuit(3)
    circuit.cswap(0, 1, 2)

    qir = circuit.to_qir(TargetProfile.Base)

    assert "__quantum__qis__ccx__body" in qir


def test_openqasm_export_declares_the_circuit_width() -> None:
    text = bell().to_qasm()

    assert text.startswith("OPENQASM 3.0;")
    assert "qubit[2] q;" in text
    assert "bit[2] c;" in text
    assert "h q[0];" in text


def test_openqasm_round_trips_a_straight_line_circuit() -> None:
    circuit = bell()

    back = QuantumCircuit.from_qasm(circuit.to_qasm())

    assert back.num_qubits == circuit.num_qubits
    assert back.num_clbits == circuit.num_clbits
    assert names(back) == names(circuit)
    assert back.to_qasm() == circuit.to_qasm()


def test_imported_integer_casts_select_the_correct_constant_branch() -> None:
    circuit = QuantumCircuit.from_qasm("""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        if (int[32](1.9) == int[32](1.1)) { x q; }
        """)

    qir = circuit.to_qir(TargetProfile.Base)

    assert "call void @__quantum__qis__x__body" in qir
    assert "br i1" not in qir


def test_imported_dynamic_conversion_raises_a_named_value_error() -> None:
    with pytest.raises(ValueError, match="runtime cast in a condition"):
        QuantumCircuit.from_qasm("""
            OPENQASM 3.0;
            include "stdgates.inc";
            qubit q;
            bit c = measure q;
            if (float[64](int[32](c)) == 0.0) { x q; }
            """)


def test_openqasm_round_trips_control_flow() -> None:
    circuit = measured_conditional()

    back = QuantumCircuit.from_qasm(circuit.to_qasm())

    assert names(back) == names(circuit)
    assert [view.name for view in back.body_of(1)] == ["x"]


def test_a_switch_exports_as_openqasm_3_1() -> None:
    circuit = QuantumCircuit(1, 1)
    circuit.measure(0, 0)
    with circuit.switch(0) as selected:
        with selected.case(1):
            circuit.x(0)

    text = circuit.to_qasm()

    assert text.startswith("OPENQASM 3.1;"), text
    assert names(QuantumCircuit.from_qasm(text)) == names(circuit)


def test_register_names_do_not_survive_the_round_trip() -> None:
    # Export writes one qubit register and one classical register whatever the
    # circuit was built from, so this is a loss on the way out, not the way in.
    circuit = QuantumCircuit.from_registers(QuantumRegister("data", 2), clbits=2)
    circuit.h(0)

    back = QuantumCircuit.from_qasm(circuit.to_qasm())

    assert back.num_qubits == 2
    assert [register.name for register in circuit.quantum_registers] == ["data"]
    assert back.quantum_registers == []


def test_source_that_does_not_parse_is_refused() -> None:
    with pytest.raises(ValueError) as caught:
        QuantumCircuit.from_qasm("OPENQASM 3.0;\nthis is not OpenQASM\n")

    assert "parse" in str(caught.value).lower()


def test_a_construct_a_circuit_cannot_hold_is_named() -> None:
    source = (
        "OPENQASM 3.0;\n"
        'include "stdgates.inc";\n'
        "gate flip q { x q; }\n"
        "qubit[1] q;\n"
        "flip q[0];\n"
    )

    with pytest.raises(ValueError) as caught:
        QuantumCircuit.from_qasm(source)

    assert "gate definition" in str(caught.value)


@pytest.mark.parametrize(
    ("convert", "verb"),
    [
        (lambda circuit: circuit.to_qir(TargetProfile.Base), "compile"),
        (lambda circuit: circuit.to_qasm(), "export"),
        (lambda circuit: circuit.to_bytes(), "save"),
    ],
)
def test_converting_while_a_block_is_open_is_refused(convert, verb: str) -> None:
    circuit = QuantumCircuit(2, 1)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        with pytest.raises(ValueError) as caught:
            convert(circuit)

    assert f"cannot {verb} a circuit" in str(caught.value)


if __name__ == "__main__":
    pytest.main([__file__])
