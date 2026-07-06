# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from qdk._context import Context
from qdk.qre import ISA, LOGICAL, PSSPC, LatticeSurgery, linear_function
from qdk.qre._qre import _ProvenanceGraph
from qdk.qre.application import QSharpApplication
from qdk.qre.instruction_ids import (
    CCX,
    CX,
    LATTICE_SURGERY,
    MEAS_RESET_Z,
    MEAS_Z,
    PAULI_X,
    T,
)

SMALL_PROGRAM = """
{
    use (a, b, c) = (Qubit(), Qubit(), Qubit());
    X(a);
    CNOT(a, b);
    CCNOT(a, b, c);
    let _ = M(a);
    MResetZ(b);
}
"""


def _make_basic_isa() -> ISA:
    graph = _ProvenanceGraph()
    return graph.make_isa(
        [
            graph.add_instruction(
                LATTICE_SURGERY,
                encoding=LOGICAL,
                arity=None,
                time=1000,
                space=linear_function(50),
                error_rate=linear_function(1e-6),
            ),
            graph.add_instruction(
                T, encoding=LOGICAL, time=1000, space=400, error_rate=1e-8
            ),
        ]
    )


def test_qsharp_trace_backend_from_entry_expr():
    app = QSharpApplication(SMALL_PROGRAM, use_trace_backend=True)
    trace = app.get_trace()

    assert trace.compute_qubits == 3
    assert trace.total_qubits == 3
    assert trace.num_gates == 5
    assert trace.depth == 4
    assert {c.id for c in trace.required_isa} == {
        PAULI_X,
        CX,
        CCX,
        MEAS_Z,
        MEAS_RESET_Z,
    }

    transformed = PSSPC(num_ts_per_rotation=16, ccx_magic_states=False).transform(trace)
    assert transformed is not None
    transformed = LatticeSurgery().transform(transformed)
    assert transformed is not None

    result = transformed.estimate(_make_basic_isa(), max_error=float("inf"))
    assert result is not None
    assert result.qubits > 0
    assert result.runtime > 0


def test_qsharp_trace_backend_from_callable(context: Context):
    context.eval(
        """
        namespace TestTraceInterop {
            operation Entry() : Unit {
                use (a, b, c) = (Qubit(), Qubit(), Qubit());
                X(a);
                CNOT(a, b);
                CCNOT(a, b, c);
                let _ = M(a);
                MResetZ(b);
            }
        }
        """
    )

    app = QSharpApplication(context.code.TestTraceInterop.Entry, use_trace_backend=True)
    trace = app.get_trace()

    assert trace.compute_qubits == 3
    assert trace.num_gates == 5
    assert {c.id for c in trace.required_isa} == {
        PAULI_X,
        CX,
        CCX,
        MEAS_Z,
        MEAS_RESET_Z,
    }
