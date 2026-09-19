# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""I2 host qualification through the existing QIR lowering and Rust region seam."""

import cmath
import math
from collections import Counter
from pathlib import Path
import struct

import numpy as np
import pytest
from qdk import _native
from qdk._adaptive_pass import AdaptiveProfilePass, Bytecode
from qdk._native import QirInstructionId as Op
from qdk.simulation._simulation import (
    _validate_base_profile,
    preprocess_simulation_input,
)

from build_measured_circuit import ParsedCircuit, compile_measured_qir, qsharp_source, shared_buffer_diagnostic
from reference import qir_instructions


DIRECTORY = Path(__file__).resolve().parent


def build_qir(qir):
    module, _, _, _ = preprocess_simulation_input(qir, shots=1, seed=42)
    _validate_base_profile(module)
    program = AdaptiveProfilePass(Bytecode.Bit64).run(module)
    return _native._tensor_network_build_probe(program.as_dict())


def build(qubits, gates):
    qir = compile_measured_qir(qsharp_source(ParsedCircuit(qubits, gates)))
    return build_qir(qir)


def assert_bindings(report):
    assert len(report["nodes"]) == len(report["node_buffer_ids"])
    assert report["marginalized"] == []
    for axes, buffer_id in zip(report["nodes"], report["node_buffer_ids"], strict=True):
        assert all(dim == 2 for _, dim in axes)
        values = np.asarray(report["buffers"][buffer_id], dtype=np.complex128)
        assert values.shape == (math.prod(dim for _, dim in axes),)
        assert np.isfinite(values).all()


def contract_tiny(report):
    """Evaluate only tiny built networks with NumPy, never the 4x4 fixture."""
    assert_bindings(report)
    ids = sorted({axis for node in report["nodes"] for axis, _ in node})
    assert len(ids) <= 12, "test contraction is bounded to tiny networks"
    labels = {axis: position for position, axis in enumerate(ids)}
    operands = []
    for node, buffer_id in zip(report["nodes"], report["node_buffer_ids"], strict=True):
        tensor = np.asarray(report["buffers"][buffer_id], dtype=np.complex128).reshape(
            tuple(dim for _, dim in node), order="F"
        )
        operands.extend((tensor, [labels[axis] for axis, _ in node]))
    operands.append([labels[axis] for axis, _ in report["output_axes"]])
    return np.einsum(*operands, optimize=False).ravel(order="F")


@pytest.mark.parametrize("angle", [0.0, 0.7, -0.7, math.pi])
@pytest.mark.parametrize("target", [0, 2])
def test_signed_rx_with_asymmetric_output_and_idle_boundaries(angle, target):
    report = build(3, [("rx", angle, target)])
    actual = contract_tiny(report)
    expected = np.zeros(8, dtype=np.complex128)
    expected[0] = math.cos(angle / 2)
    expected[1 << target] = -1j * math.sin(angle / 2)
    np.testing.assert_allclose(actual, expected, atol=1e-12, rtol=0)


@pytest.mark.parametrize("angle", [0.4, -0.4])
@pytest.mark.parametrize("bit0,bit2", [(0, 0), (0, 1), (1, 0), (1, 1)])
def test_every_rzz_phase_on_nonadjacent_basis_states(angle, bit0, bit2):
    gates = []
    if bit0:
        gates.append(("rx", math.pi, 0))
    if bit2:
        gates.append(("rx", math.pi, 2))
    gates.append(("rzz", angle, 2, 0))
    actual = contract_tiny(build(3, gates))
    expected = np.zeros(8, dtype=np.complex128)
    expected[bit0 + 4 * bit2] = (-1j) ** (bit0 + bit2) * cmath.exp(
        -0.5j * angle * (-1) ** (bit0 + bit2)
    )
    np.testing.assert_allclose(actual, expected, atol=1e-12, rtol=0)


def test_shared_buffers_with_interference_and_changed_wire_versions():
    a, b, phi, c = 0.7, -0.3, 0.41, 0.29
    diagnostic = shared_buffer_diagnostic()
    report = build(diagnostic.num_qubits, diagnostic.gates)
    ids = report["node_buffer_ids"]
    assert ids[3] == ids[4]
    assert ids[5] == ids[7]
    assert len(set(ids[3:])) == 4

    def rx(angle, out, inp):
        return math.cos(angle / 2) if out == inp else -1j * math.sin(angle / 2)

    def phase(left, right):
        return cmath.exp(-0.5j * phi * (-1) ** (left + right))

    expected = np.zeros(8, dtype=np.complex128)
    for out0 in range(2):
        for out2 in range(2):
            expected[out0 + 4 * out2] = sum(
                rx(a, in0, 0) * rx(a, in2, 0) * phase(in0, in2)
                * rx(b, out0, in0) * phase(out0, in2) * rx(c, out2, in2)
                for in0 in range(2) for in2 in range(2)
            )
    actual = contract_tiny(report)
    np.testing.assert_allclose(actual, expected, atol=1e-12, rtol=0)
    assert abs(np.vdot(actual, actual).real - 1) <= 1e-12


def test_frozen_case_a_binds_every_gate_and_keeps_all_final_axes():
    qir = (DIRECTORY / "fixtures/case_a_4x4/measured.ll").read_text(encoding="utf-8")
    instructions, qubits, results = qir_instructions(qir, measured=True)
    gates = instructions[:432]
    assert qubits == results == 16
    assert sum(gate[0] == Op.RX for gate in gates) == 192
    assert sum(gate[0] == Op.RZZ for gate in gates) == 240
    report = build_qir(qir)
    assert_bindings(report)
    assert report["operation_count"] == 432
    nodes = report["nodes"]
    bindings = report["node_buffer_ids"]
    assert len(nodes) == 448
    wires = []
    seen = set()
    for node in range(qubits):
        assert len(nodes[node]) == 1
        wire = nodes[node][0]
        assert wire not in seen
        seen.add(wire)
        wires.append(wire)
        assert bindings[node] == bindings[0]
        assert report["buffers"][bindings[node]] == [1 + 0j, 0j]

    buffer_ids = {}
    for node, (gate, angle, *operands) in enumerate(gates, start=qubits):
        axes = nodes[node]
        assert len(axes) == 2
        values = np.asarray(report["buffers"][bindings[node]]).reshape((2, 2), order="F")
        expected = np.empty((2, 2), dtype=np.complex128)
        if gate == Op.RX:
            target, = operands
            assert axes[1] == wires[target]
            assert axes[0] not in seen
            seen.add(axes[0])
            wires[target] = axes[0]
            for out in range(2):
                for inp in range(2):
                    expected[out, inp] = (
                        math.cos(angle / 2) if out == inp else -1j * math.sin(angle / 2)
                    )
        else:
            assert gate == Op.RZZ
            q1, q2 = operands
            assert axes == [wires[q1], wires[q2]]
            for a in range(2):
                for b in range(2):
                    expected[a, b] = cmath.exp(-0.5j * angle * (-1) ** (a + b))
        np.testing.assert_allclose(values, expected, atol=1e-12, rtol=0)
        key = ("rx" if gate == Op.RX else "rzz", struct.pack("<d", angle))
        if key in buffer_ids:
            assert bindings[node] == buffer_ids[key]
        else:
            assert bindings[node] not in buffer_ids.values()
            assert bindings[node] != bindings[0]
            buffer_ids[key] = bindings[node]

    assert len(report["buffers"]) == 1 + len(buffer_ids) == 6
    assert sum(len(values) * 16 for values in report["buffers"]) == 352
    assert len(seen) == 208
    assert report["output_axes"] == wires
    assert len(set(wires)) == 16
    assert math.prod(dim for _, dim in wires) == 65536
    assert report["measurement_qubits"] == report["measurement_result_ids"] == list(range(16))
    incidence = Counter(axis for node in nodes for axis, _ in node)
    kept = {axis for axis, _ in wires}
    assert all(count >= 2 or axis in kept for axis, count in incidence.items())
    expected_hyperedges = sorted(axis for axis, count in incidence.items() if count + (axis in kept) != 2)
    assert report["hyperedges"] == expected_hyperedges
    assert expected_hyperedges


def test_probe_rejects_unsupported_unitary_instead_of_dropping_it():
    qir = compile_measured_qir(qsharp_source(ParsedCircuit(1, [("rx", 0.7, 0)])))
    qir = qir.replace("rx__body", "ry__body")
    with pytest.raises(ValueError, match="unsupported: Ry"):
        build_qir(qir)


def test_probe_rejects_multiple_regions_without_fabricating_measurements():
    qir = """\
%Qubit = type opaque
%Result = type opaque
define void @main() #0 {
entry:
  call void @__quantum__qis__rx__body(double 0.7, %Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__rx__body(double 0.3, %Qubit* null)
  ret void
}
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*)
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
"""
    with pytest.raises(ValueError, match="one leading unitary region"):
        build_qir(qir)
