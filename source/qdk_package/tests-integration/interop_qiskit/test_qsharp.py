# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from . import QISKIT_AVAILABLE, SKIP_REASON


if QISKIT_AVAILABLE:
    from qdk.qiskit import (
        OutputSemantics,
        ProgramType,
        QSharpBackend,
    )
    from qiskit.circuit import QuantumCircuit


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qsharp_smoke() -> None:
    circuit = QuantumCircuit(2, 2)
    circuit.x(0)
    circuit.cx(0, 1)
    circuit.measure_all(add_bits=False)
    circuit.name = "smoke"

    backend = QSharpBackend()
    res = backend._qsharp(circuit)
    assert res is not None
    assert "qasm_import" in res
    assert "operation smoke() : Result[]" in res
    assert "Std.Arrays.Reversed" in res


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qsharp_disable_output() -> None:
    circuit = QuantumCircuit(2, 2)
    circuit.x(0)
    circuit.cx(0, 1)
    circuit.measure_all(add_bits=False)
    circuit.name = "circuit_with_unit_output"
    backend = QSharpBackend()
    output_semantics = OutputSemantics.ResourceEstimation

    res = backend._qsharp(circuit, output_semantics=output_semantics)
    assert "operation circuit_with_unit_output() : Unit" in res


@pytest.mark.skipif(not QISKIT_AVAILABLE, reason=SKIP_REASON)
def test_qsharp_openqasm_output_semantics() -> None:
    circuit = QuantumCircuit(2, 2)
    circuit.x(0)
    circuit.cx(0, 1)
    circuit.measure_all(add_bits=False)
    circuit.name = "circuit_with_unit_output"
    backend = QSharpBackend()
    output_semantics = OutputSemantics.OpenQasm

    res = backend._qsharp(circuit, output_semantics=output_semantics)
    assert "Std.Arrays.Reversed" not in res
