# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path

import pytest

cirq = pytest.importorskip("cirq")

import qdk as qsharp
from qdk.qre.application import QSharpApplication, QIRApplication
from qdk.qre.interop import trace_from_qir


def _ll_files():
    """Return the list of QIR .ll test files."""
    ll_dir = (
        Path(__file__).parent.parent.parent
        / "tests-integration"
        / "resources"
        / "adaptive_ri"
        / "output"
    )
    return sorted(ll_dir.glob("*.ll"))


@pytest.mark.parametrize("ll_file", _ll_files(), ids=lambda p: p.stem)
def test_trace_from_qir(ll_file):
    """Test that trace_from_qir can parse real QIR output files."""
    # NOTE: This test is primarily to ensure that the function can parse real
    # QIR output without errors, rather than checking specific properties of the
    # trace.
    try:
        app = QIRApplication(ll_file.read_text(encoding="utf-8"))
        _ = app.get_trace()
    except ValueError as e:
        # The only reason of failure is presence of control flow
        assert (
            str(e)
            == "simulation of programs with branching control flow is not supported"
        )


def test_trace_from_qir_handles_all_instruction_ids():
    """Verify that trace_from_qir handles every QirInstructionId except CorrelatedNoise.

    Generates a synthetic QIR program containing one instance of each gate
    intrinsic recognised by AggregateGatesPass and asserts that trace_from_qir
    processes all of them without error.
    """
    import pyqir
    import pyqir.qis as qis
    from qdk._native import QirInstructionId
    from qdk.qre.interop._qir import _GATE_MAP, _MEAS_MAP, _SKIP

    # -- Completeness check: every QirInstructionId must be covered --------
    handled_ids = (
        [qir_id for qir_id, _, _ in _GATE_MAP]
        + [qir_id for qir_id, _ in _MEAS_MAP]
        + list(_SKIP)
    )
    # Exhaustive list of all QirInstructionId variants (pyo3 enums are not iterable)
    all_ids = [
        QirInstructionId.I,
        QirInstructionId.H,
        QirInstructionId.X,
        QirInstructionId.Y,
        QirInstructionId.Z,
        QirInstructionId.S,
        QirInstructionId.SAdj,
        QirInstructionId.SX,
        QirInstructionId.SXAdj,
        QirInstructionId.T,
        QirInstructionId.TAdj,
        QirInstructionId.CNOT,
        QirInstructionId.CX,
        QirInstructionId.CY,
        QirInstructionId.CZ,
        QirInstructionId.CCX,
        QirInstructionId.SWAP,
        QirInstructionId.RX,
        QirInstructionId.RY,
        QirInstructionId.RZ,
        QirInstructionId.RXX,
        QirInstructionId.RYY,
        QirInstructionId.RZZ,
        QirInstructionId.RESET,
        QirInstructionId.M,
        QirInstructionId.MResetZ,
        QirInstructionId.MZ,
        QirInstructionId.Move,
        QirInstructionId.ReadResult,
        QirInstructionId.ResultRecordOutput,
        QirInstructionId.BoolRecordOutput,
        QirInstructionId.IntRecordOutput,
        QirInstructionId.DoubleRecordOutput,
        QirInstructionId.TupleRecordOutput,
        QirInstructionId.ArrayRecordOutput,
        QirInstructionId.CorrelatedNoise,
    ]
    unhandled = [
        i
        for i in all_ids
        if i not in handled_ids and i != QirInstructionId.CorrelatedNoise
    ]
    assert unhandled == [], (
        f"QirInstructionId values not covered by _GATE_MAP, _MEAS_MAP, or _SKIP: "
        f"{', '.join(str(i) for i in unhandled)}"
    )

    # -- Generate a QIR program with every producible gate -----------------
    simple = pyqir.SimpleModule("test_all_gates", num_qubits=4, num_results=3)
    builder = simple.builder
    ctx = simple.context
    q = simple.qubits
    r = simple.results

    void_ty = pyqir.Type.void(ctx)
    qubit_ty = pyqir.PointerType(void_ty)
    result_ty = pyqir.PointerType(void_ty)
    double_ty = pyqir.Type.double(ctx)
    i64_ty = pyqir.IntType(ctx, 64)

    def declare(name, param_types):
        return simple.add_external_function(
            name, pyqir.FunctionType(void_ty, param_types)
        )

    # Single-qubit gates (pyqir.qis builtins)
    qis.h(builder, q[0])
    qis.x(builder, q[0])
    qis.y(builder, q[0])
    qis.z(builder, q[0])
    qis.s(builder, q[0])
    qis.s_adj(builder, q[0])
    qis.t(builder, q[0])
    qis.t_adj(builder, q[0])

    # SX — not in pyqir.qis
    sx_fn = declare("__quantum__qis__sx__body", [qubit_ty])
    builder.call(sx_fn, [q[0]])

    # Two-qubit gates (qis.cx emits __quantum__qis__cnot__body which the
    # pass does not handle, so use builder.call with the correct name)
    cx_fn = declare("__quantum__qis__cx__body", [qubit_ty, qubit_ty])
    builder.call(cx_fn, [q[0], q[1]])
    qis.cz(builder, q[0], q[1])
    qis.swap(builder, q[0], q[1])

    cy_fn = declare("__quantum__qis__cy__body", [qubit_ty, qubit_ty])
    builder.call(cy_fn, [q[0], q[1]])

    # Three-qubit gate
    qis.ccx(builder, q[0], q[1], q[2])

    # Single-qubit rotations
    qis.rx(builder, 1.0, q[0])
    qis.ry(builder, 1.0, q[0])
    qis.rz(builder, 1.0, q[0])

    # Two-qubit rotations — not in pyqir.qis
    rot2_ty = [double_ty, qubit_ty, qubit_ty]
    angle = pyqir.const(double_ty, 1.0)
    for name in ("rxx", "ryy", "rzz"):
        fn = declare(f"__quantum__qis__{name}__body", rot2_ty)
        builder.call(fn, [angle, q[0], q[1]])

    # Measurements
    qis.mz(builder, q[0], r[0])

    m_fn = declare("__quantum__qis__m__body", [qubit_ty, result_ty])
    builder.call(m_fn, [q[1], r[1]])

    mresetz_fn = declare("__quantum__qis__mresetz__body", [qubit_ty, result_ty])
    builder.call(mresetz_fn, [q[2], r[2]])

    # Reset / Move
    qis.reset(builder, q[0])

    move_fn = declare("__quantum__qis__move__body", [qubit_ty])
    builder.call(move_fn, [q[0]])

    # Output recording
    tag = simple.add_byte_string(b"tag")
    arr_fn = declare("__quantum__rt__array_record_output", [i64_ty, tag.type])
    builder.call(arr_fn, [pyqir.const(i64_ty, 1), tag])

    rec_fn = declare("__quantum__rt__result_record_output", [result_ty, tag.type])
    builder.call(rec_fn, [r[0], tag])

    tup_fn = declare("__quantum__rt__tuple_record_output", [i64_ty, tag.type])
    builder.call(tup_fn, [pyqir.const(i64_ty, 1), tag])

    # -- Run trace_from_qir and verify it succeeds -------------------------
    trace = trace_from_qir(simple.ir())
    assert trace is not None


def test_rotation_buckets():
    """Test that rotation bucketization preserves total count and depth."""
    from qdk.qre.interop._qsharp import _bucketize_rotation_counts

    r_count = 15066
    r_depth = 14756
    q_count = 291

    result = _bucketize_rotation_counts(r_count, r_depth)

    a_count = 0
    a_depth = 0
    for c, d in result:
        assert c <= q_count
        assert c > 0
        a_count += c * d
        a_depth += d

    assert a_count == r_count
    assert a_depth == r_depth


def test_qsharp_from_string():
    code = """
    {{
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }}
    """

    app = QSharpApplication(code)
    trace = app.get_trace()

    assert trace.total_qubits == 3, "unexpected number of qubits in trace"
    assert trace.depth == 3, "unexpected depth of trace"
    assert trace.num_gates == 3, "unexpected number of gates in trace"


def test_qsharp_from_callable():
    qsharp.eval(
        """
    operation Test(numTs: Int) : Unit {{
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        for i in 1..numTs {{
            T(a);
        }}
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }}
    """
    )

    for num_ts in range(1, 6):
        app = QSharpApplication(qsharp.code.Test, args=(num_ts,))  # type: ignore
        trace = app.get_trace()

        assert trace.total_qubits == 3, "unexpected number of qubits in trace"
        assert trace.depth == 2 + num_ts, "unexpected depth of trace"
        assert trace.num_gates == 2 + num_ts, "unexpected number of gates in trace"
