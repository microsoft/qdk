# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Tests for the circuit_from_qir feature (pyqir.Module → Circuit)."""

from textwrap import dedent
import json
import pytest
import qsharp
from qsharp import circuit_from_qir
from qsharp._native import CircuitConfig, circuit_from_qir_program

# ---------------------------------------------------------------------------
# Helper: compile Q# to QIR, parse with pyqir, return the pyqir.Module.
# ---------------------------------------------------------------------------

pyqir = pytest.importorskip("pyqir")


def _qir_module(source: str) -> "pyqir.Module":
    """Compile Q# source to QIR and parse it into a pyqir.Module."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qsharp.eval(source)
    qir_text = str(qsharp.compile(qsharp.code.Main))
    ctx = pyqir.Context()
    return pyqir.Module.from_ir(ir=qir_text, context=ctx)


# ---------------------------------------------------------------------------
# High-level tests: circuit_from_qir(module)
# ---------------------------------------------------------------------------


def test_single_h_gate() -> None:
    module = _qir_module(
        dedent(
            """\
        operation Main() : Result {
            use q = Qubit();
            H(q);
            M(q)
        }
        """
        )
    )
    circuit = circuit_from_qir(module)
    assert str(circuit) == dedent(
        """\
        q_0    ── H ──── M ──
                         ╘═══
        """
    )


def test_bell_state() -> None:
    module = _qir_module(
        dedent(
            """\
        operation Main() : (Result, Result) {
            use (q0, q1) = (Qubit(), Qubit());
            H(q0);
            CNOT(q0, q1);
            let r0 = M(q0);
            let r1 = M(q1);
            Reset(q0);
            Reset(q1);
            (r0, r1)
        }
        """
        )
    )
    circuit = circuit_from_qir(module)
    circuit_str = str(circuit)
    # Verify structural properties—the circuit has two qubits,
    # H and CNOT gates, and two measurements.
    assert "H" in circuit_str
    assert "M" in circuit_str
    # JSON round-trip check.
    data = json.loads(circuit.json())
    assert len(data["qubits"]) >= 2


def test_multiple_gates() -> None:
    module = _qir_module(
        dedent(
            """\
        operation Main() : Result {
            use q = Qubit();
            H(q);
            X(q);
            Y(q);
            Z(q);
            M(q)
        }
        """
        )
    )
    circuit = circuit_from_qir(module)
    circuit_str = str(circuit)
    for gate in ("H", "X", "Y", "Z", "M"):
        assert gate in circuit_str


def test_rotation_gates() -> None:
    module = _qir_module(
        dedent(
            """\
        open Microsoft.Quantum.Math;
        operation Main() : Result {
            use q = Qubit();
            Rx(PI() / 2.0, q);
            Ry(PI() / 4.0, q);
            Rz(PI(), q);
            M(q)
        }
        """
        )
    )
    circuit = circuit_from_qir(module)
    circuit_str = str(circuit)
    assert "Rx" in circuit_str
    assert "Ry" in circuit_str
    assert "Rz" in circuit_str


def test_config_max_operations() -> None:
    module = _qir_module(
        dedent(
            """\
        operation Main() : Result {
            use q = Qubit();
            H(q);
            M(q)
        }
        """
        )
    )
    config = CircuitConfig()
    config.max_operations = 100
    circuit = circuit_from_qir(module, config=config)
    assert "H" in str(circuit)


def test_json_output() -> None:
    module = _qir_module(
        dedent(
            """\
        operation Main() : Result {
            use q = Qubit();
            H(q);
            M(q)
        }
        """
        )
    )
    circuit = circuit_from_qir(module)
    data = json.loads(circuit.json())
    assert "qubits" in data
    assert "componentGrid" in data
    assert len(data["qubits"]) >= 1
    assert len(data["componentGrid"]) >= 1


def test_no_entry_point_raises() -> None:
    ctx = pyqir.Context()
    # Create a module with no entry point.
    module = pyqir.Module.from_ir(
        ir="""\
        define void @not_an_entry_point() {
          ret void
        }
        """,
        context=ctx,
    )
    with pytest.raises(ValueError, match="No entry point"):
        circuit_from_qir(module)


# ---------------------------------------------------------------------------
# Low-level tests: circuit_from_qir_program(...)
# ---------------------------------------------------------------------------


def test_native_single_call() -> None:
    """Call the native function directly with a single H gate."""
    blocks = [
        (
            0,
            [
                {
                    "kind": "Call",
                    "callable_name": "__quantum__qis__h__body",
                    "args": [{"kind": "lit", "lit": {"kind": "Qubit", "value": 0}}],
                    "output": None,
                    "dbg_location": None,
                },
                {"kind": "Return"},
            ],
        )
    ]
    config = CircuitConfig()
    circuit = circuit_from_qir_program(0, 1, blocks, config)
    assert "H" in str(circuit)


def test_native_measurement() -> None:
    """Call the native function with an H gate followed by M."""
    blocks = [
        (
            0,
            [
                {
                    "kind": "Call",
                    "callable_name": "__quantum__qis__h__body",
                    "args": [{"kind": "lit", "lit": {"kind": "Qubit", "value": 0}}],
                    "output": None,
                    "dbg_location": None,
                },
                {
                    "kind": "Call",
                    "callable_name": "__quantum__qis__m__body",
                    "args": [
                        {"kind": "lit", "lit": {"kind": "Qubit", "value": 0}},
                        {"kind": "lit", "lit": {"kind": "Result", "value": 0}},
                    ],
                    "output": None,
                    "dbg_location": None,
                },
                {"kind": "Return"},
            ],
        )
    ]
    config = CircuitConfig()
    circuit = circuit_from_qir_program(0, 1, blocks, config)
    assert str(circuit) == dedent(
        """\
        q_0    ── H ──── M ──
                         ╘═══
        """
    )


def test_native_two_qubit_gate() -> None:
    """Call the native function with a CNOT gate."""
    blocks = [
        (
            0,
            [
                {
                    "kind": "Call",
                    "callable_name": "__quantum__qis__cx__body",
                    "args": [
                        {"kind": "lit", "lit": {"kind": "Qubit", "value": 0}},
                        {"kind": "lit", "lit": {"kind": "Qubit", "value": 1}},
                    ],
                    "output": None,
                    "dbg_location": None,
                },
                {"kind": "Return"},
            ],
        )
    ]
    config = CircuitConfig()
    circuit = circuit_from_qir_program(0, 2, blocks, config)
    circuit_str = str(circuit)
    assert "X" in circuit_str  # CNOT shows as controlled-X


def test_native_jump_between_blocks() -> None:
    """Two blocks connected by a jump."""
    blocks = [
        (
            0,
            [
                {
                    "kind": "Call",
                    "callable_name": "__quantum__qis__h__body",
                    "args": [{"kind": "lit", "lit": {"kind": "Qubit", "value": 0}}],
                    "output": None,
                    "dbg_location": None,
                },
                {"kind": "Jump", "target": 1},
            ],
        ),
        (
            1,
            [
                {
                    "kind": "Call",
                    "callable_name": "__quantum__qis__x__body",
                    "args": [{"kind": "lit", "lit": {"kind": "Qubit", "value": 0}}],
                    "output": None,
                    "dbg_location": None,
                },
                {"kind": "Return"},
            ],
        ),
    ]
    config = CircuitConfig()
    circuit = circuit_from_qir_program(0, 1, blocks, config)
    circuit_str = str(circuit)
    assert "H" in circuit_str
    assert "X" in circuit_str


def test_native_empty_program() -> None:
    """A single block with only a Return."""
    blocks = [(0, [{"kind": "Return"}])]
    config = CircuitConfig()
    circuit = circuit_from_qir_program(0, 1, blocks, config)
    # Should produce a circuit with one qubit and no operations.
    data = json.loads(circuit.json())
    assert len(data["qubits"]) == 1
    assert data["componentGrid"] == []
