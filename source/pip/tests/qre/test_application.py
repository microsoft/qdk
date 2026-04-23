# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

cirq = pytest.importorskip("cirq")

from dataclasses import dataclass, field

import qsharp

from qsharp.qre import (
    Application,
    ISA,
    LOGICAL,
    PSSPC,
    EstimationResult,
    LatticeSurgery,
    Trace,
    linear_function,
)
from qsharp.qre._qre import _ProvenanceGraph
from qsharp.qre._enumeration import _enumerate_instances
from qsharp.qre.application import QSharpApplication
from qsharp.qre.instruction_ids import CCX, LATTICE_SURGERY, T, RZ
from qsharp.qre.property_keys import (
    ALGORITHM_COMPUTE_QUBITS,
    ALGORITHM_MEMORY_QUBITS,
    LOGICAL_COMPUTE_QUBITS,
    LOGICAL_MEMORY_QUBITS,
)


def _assert_estimation_result(trace: Trace, result: EstimationResult, isa: ISA):
    """Assert that an estimation result matches expected qubit, runtime, and error values."""
    actual_qubits = (
        isa[LATTICE_SURGERY].expect_space(trace.compute_qubits)
        + isa[T].expect_space() * result.factories[T].copies
    )
    if CCX in trace.resource_states:
        actual_qubits += isa[CCX].expect_space() * result.factories[CCX].copies
    assert result.qubits == actual_qubits

    assert (
        result.runtime
        == isa[LATTICE_SURGERY].expect_time(trace.compute_qubits) * trace.depth
    )

    actual_error = (
        trace.base_error
        + isa[LATTICE_SURGERY].expect_error_rate(trace.compute_qubits) * trace.depth
        + isa[T].expect_error_rate() * result.factories[T].states
    )
    if CCX in trace.resource_states:
        actual_error += isa[CCX].expect_error_rate() * result.factories[CCX].states
    assert abs(result.error - actual_error) <= 1e-8


def test_trace_properties():
    """Test setting and getting typed properties on a Trace."""
    trace = Trace(42)

    INT = 0
    FLOAT = 1
    BOOL = 2
    STR = 3

    trace.set_property(INT, 42)
    assert trace.get_property(INT) == 42
    assert isinstance(trace.get_property(INT), int)

    trace.set_property(FLOAT, 3.14)
    assert trace.get_property(FLOAT) == 3.14
    assert isinstance(trace.get_property(FLOAT), float)

    trace.set_property(BOOL, True)
    assert trace.get_property(BOOL) is True
    assert isinstance(trace.get_property(BOOL), bool)

    trace.set_property(STR, "hello")
    assert trace.get_property(STR) == "hello"
    assert isinstance(trace.get_property(STR), str)


def test_qsharp_application():
    """Test QSharpApplication trace generation and estimation from a Q# program."""
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

    assert trace.compute_qubits == 3
    assert trace.depth == 3
    assert trace.resource_states == {}

    assert {c.id for c in trace.required_isa} == {CCX, T, RZ}

    graph = _ProvenanceGraph()
    isa = graph.make_isa(
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
            graph.add_instruction(
                CCX, encoding=LOGICAL, time=2000, space=800, error_rate=1e-10
            ),
        ]
    )

    # Properties from the program
    counts = qsharp.logical_counts(code)
    num_ts = counts["tCount"]
    num_ccx = counts["cczCount"]
    num_rotations = counts["rotationCount"]
    rotation_depth = counts["rotationDepth"]

    lattice_surgery = LatticeSurgery()

    counter = 0
    for psspc in _enumerate_instances(PSSPC):
        counter += 1
        trace2 = psspc.transform(trace)
        assert trace2 is not None
        trace2 = lattice_surgery.transform(trace2)
        assert trace2 is not None
        assert trace2.compute_qubits == 12
        assert (
            trace2.depth
            == num_ts
            + num_ccx * 3
            + num_rotations
            + rotation_depth * psspc.num_ts_per_rotation
        )
        if psspc.ccx_magic_states:
            assert trace2.resource_states == {
                T: num_ts + psspc.num_ts_per_rotation * num_rotations,
                CCX: num_ccx,
            }
            assert {c.id for c in trace2.required_isa} == {CCX, T, LATTICE_SURGERY}
        else:
            assert trace2.resource_states == {
                T: num_ts + psspc.num_ts_per_rotation * num_rotations + 4 * num_ccx
            }
            assert {c.id for c in trace2.required_isa} == {T, LATTICE_SURGERY}
        assert trace2.get_property(ALGORITHM_COMPUTE_QUBITS) == 3
        assert trace2.get_property(ALGORITHM_MEMORY_QUBITS) == 0
        result = trace2.estimate(isa, max_error=float("inf"))
        assert result is not None
        assert result.properties[ALGORITHM_COMPUTE_QUBITS] == 3
        assert result.properties[ALGORITHM_MEMORY_QUBITS] == 0
        assert result.properties[LOGICAL_COMPUTE_QUBITS] == 12
        assert result.properties[LOGICAL_MEMORY_QUBITS] == 0
        _assert_estimation_result(trace2, result, isa)
    assert counter == 32


def test_application_enumeration():
    """Test that Application.q() enumerates the correct number of traces."""

    @dataclass(kw_only=True)
    class _Params:
        size: int = field(default=1, metadata={"domain": range(1, 4)})

    class TestApp(Application[_Params]):
        def get_trace(self, parameters: _Params) -> Trace:
            return Trace(parameters.size)

    app = TestApp()
    assert sum(1 for _ in TestApp.q().enumerate(app.context())) == 3
    assert sum(1 for _ in TestApp.q(size=1).enumerate(app.context())) == 1
    assert sum(1 for _ in TestApp.q(size=[4, 5]).enumerate(app.context())) == 2


def test_trace_enumeration():
    """Test trace query enumeration with PSSPC and LatticeSurgery transforms."""
    code = """
    {{
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }}
    """

    app = QSharpApplication(code)

    ctx = app.context()
    assert sum(1 for _ in QSharpApplication.q().enumerate(ctx)) == 1

    assert sum(1 for _ in PSSPC.q().enumerate(ctx)) == 32

    assert sum(1 for _ in LatticeSurgery.q().enumerate(ctx)) == 1

    q = PSSPC.q() * LatticeSurgery.q()
    assert sum(1 for _ in q.enumerate(ctx)) == 32


def test_rotation_error_psspc():
    """Test that PSSPC base error stays below 1.0 for a single rotation gate."""
    # This test helps to bound the variables for the number of rotations in PSSPC

    # Create a trace with a single rotation gate and ensure that the base error
    # after PSSPC transformation is less than 1.
    trace = Trace(1)
    trace.add_operation(RZ, [0])

    for psspc in _enumerate_instances(PSSPC, ccx_magic_states=False):
        transformed = psspc.transform(trace)
        assert transformed is not None
        assert (
            transformed.base_error < 1.0
        ), f"Base error too high: {transformed.base_error} for {psspc.num_ts_per_rotation} T states per rotation"
