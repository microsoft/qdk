"""Tests for `qdk.ec.targets._coerce.coerce_program`."""
from __future__ import annotations

from pathlib import Path

import pytest

import qodec
from qodec.circuits import Program
from ec_tests.testing.qodecs import c4
from qdk.ec.targets._coerce import coerce_program


@pytest.fixture
def isa() -> qodec.InstructionSet:
    return c4().layers[0].isa


def _expected_program(isa: qodec.InstructionSet) -> Program:
    return Program(
        [
            qodec.instructions.InstructionCall("prepare_zz", outputs={"block": "data"}),
            qodec.instructions.InstructionCall("measure_zz", inputs={"block": "data"}),
        ],
        isa,
    )


def test_coerce_passes_through_program(isa: qodec.InstructionSet) -> None:
    program = _expected_program(isa)
    assert coerce_program(program, isa) is program


def test_coerce_parses_qasm_text(isa: qodec.InstructionSet) -> None:
    pytest.importorskip("openqasm3")
    text = """OPENQASM 3.0;
def prepare_zz(qubit[2] block) -> bit { }
def measure_zz(qubit[2] block) -> bit[2] { }
qubit[2] data;
bit reject = prepare_zz(data);
bit[2] result = measure_zz(data);
"""
    program = coerce_program(text, isa)
    assert isinstance(program, Program)
    assert [c.mnemonic for c in program.instructions] == ["prepare_zz", "measure_zz"]


def test_coerce_parses_qasm_path(isa: qodec.InstructionSet, tmp_path: Path) -> None:
    pytest.importorskip("openqasm3")
    text = """OPENQASM 3.0;
def prepare_zz(qubit[2] block) -> bit { }
def measure_zz(qubit[2] block) -> bit[2] { }
qubit[2] data;
bit reject = prepare_zz(data);
bit[2] result = measure_zz(data);
"""
    file = tmp_path / "program.qasm"
    file.write_text(text)
    program = coerce_program(file, isa)
    assert [c.mnemonic for c in program.instructions] == ["prepare_zz", "measure_zz"]


def test_coerce_parses_cirq_circuit(isa: qodec.InstructionSet) -> None:
    cirq = pytest.importorskip("cirq")
    from qodec.circuits.cirq import gates_for
    gates = gates_for(isa)
    q = cirq.LineQubit.range(2)
    circuit = cirq.Circuit([gates.prepare_zz.on(*q), gates.measure_zz.on(*q)])
    program = coerce_program(circuit, isa)
    assert [c.mnemonic for c in program.instructions] == ["prepare_zz", "measure_zz"]
