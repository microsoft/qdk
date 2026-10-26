# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


import json
from dataclasses import dataclass, field
from pathlib import Path

import pytest
from qdk import Context, qsharp
from qdk.estimator import LogicalCounts

from qdk.qre import (
    Application,
    ISA,
    LOGICAL,
    PSSPC,
    Profiler,
    ProfilerConfig,
    ProfilerResult,
    SequentialCountingProfiler,
    TemporalProfiler,
    TemporalProfiles,
    TotalCounts,
    ErrorComposition,
    EstimationResult,
    LatticeSurgery,
    Trace,
    linear_function,
)
from qdk.qre._qre import _ProvenanceGraph
from qdk.qre._enumeration import _enumerate_instances
from qdk.qre.application import QSharpApplication
from qdk.qre.instruction_ids import CCX, LATTICE_SURGERY, T, RZ
from qdk.qre.property_keys import (
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


def test_error_composition_modes():
    """Test that union-bound and product error composition differ as expected."""
    # Ten T gates, each with error rate 0.1.  Under the union bound the total
    # error is the sum (1.0), while under product composition it is
    # 1 - (1 - 0.1)^10.
    trace = Trace(1)
    for _ in range(10):
        trace.add_operation(T, [0])

    graph = _ProvenanceGraph()
    isa = graph.make_isa(
        [
            graph.add_instruction(
                T, encoding=LOGICAL, time=1000, space=400, error_rate=0.1
            ),
        ]
    )

    union = trace.estimate(
        isa, max_error=float("inf"), composition=ErrorComposition.UnionBound
    )
    assert union is not None
    assert abs(union.error - 1.0) <= 1e-9
    assert union.error_composition == ErrorComposition.UnionBound

    product = trace.estimate(
        isa, max_error=float("inf"), composition=ErrorComposition.Product
    )
    assert product is not None
    assert abs(product.error - (1.0 - 0.9**10)) <= 1e-9
    assert product.error_composition == ErrorComposition.Product

    # The default composition is the union bound.
    default = trace.estimate(isa, max_error=float("inf"))
    assert default is not None
    assert abs(default.error - union.error) <= 1e-9


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
        assert transformed.base_error < 1.0, (
            f"Base error too high: {transformed.base_error} for {psspc.num_ts_per_rotation} T states per rotation"
        )


def test_trace_iter_simple():
    """Test that iterating over a trace yields gates in execution order."""
    trace = Trace(2)
    trace.add_operation(1, [0])
    trace.add_operation(2, [1])

    gate_ids = [g.id for g in trace.flatten()]
    assert gate_ids == [1, 2]


def test_trace_iter_with_repeated_block():
    """Test that iterating over a trace repeats gates for repeated blocks."""
    trace = Trace(2)
    trace.add_operation(1, [0])
    block = trace.add_block(3)
    block.add_operation(2, [1])
    trace.add_operation(3, [0])

    gate_ids = [g.id for g in trace.flatten()]
    assert gate_ids == [1, 2, 2, 2, 3]


def test_trace_iter_nested_blocks():
    """Test that iterating over nested repeated blocks works correctly."""
    trace = Trace(3)
    trace.add_operation(1, [0])
    block = trace.add_block(2)
    block.add_operation(2, [1])
    inner = block.add_block(3)
    inner.add_operation(3, [2])
    trace.add_operation(4, [0])

    gate_ids = [g.id for g in trace.flatten()]
    assert gate_ids == [1, 2, 3, 3, 3, 2, 3, 3, 3, 4]


def test_trace_iter_gate_attributes():
    """Test that gate attributes are correctly exposed."""
    trace = Trace(1)
    trace.add_operation(42, [0, 1], [3.14])

    gates = list(trace.flatten())
    assert len(gates) == 1
    assert gates[0].id == 42
    assert gates[0].qubits == [0, 1]
    assert gates[0].params == [3.14]


@pytest.mark.parametrize("use_qir", [False, True])
def test_qsharp_application_profile(use_qir: bool, tmp_path: Path):
    """Test profiling Q# code through the Python API."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    app = QSharpApplication(
        """
        {
            use qs = Qubit[2];
            X(qs[0]);
            X(qs[1]);
            [M(qs[0]),M(qs[1])]
        }
        """
    )
    profiler = SequentialCountingProfiler(
        physical_qubits_per_logical=10,
        gate_time_sec={"x": 0.25, "mz_store": 0.5, "read_result": 0.125},
    )

    result = app.profile(profiler, use_qir=use_qir)

    assert isinstance(profiler, Profiler)
    assert isinstance(result, ProfilerResult)
    assert isinstance(result.totals, TotalCounts)
    assert result.totals.logical_qubits == 2
    assert result.totals.physical_qubits == 20
    assert result.totals.calls_per_operation == {
        "x": 2,
        "mz_store": 2,
        "read_result": 2,
    }
    assert result.totals.runtime_sec_per_operation == {
        "x": 0.5,
        "mz_store": 1.0,
        "read_result": 0.25,
    }
    assert result.totals.total_runtime_sec == 0.875 * 2
    assert isinstance(result.profiles, TemporalProfiles)
    assert result.profiles.profiles == []
    assert result.profiles.aggregation_window_sec is None
    assert result.infos == []
    summary = repr(result)
    assert summary.startswith("ProfilerResult { totals: TotalCounts {")
    assert "logical_qubits: 2" in summary
    assert "physical_qubits: 20" in summary
    assert "total_operations_count: 6" in summary
    assert "time_series_count: 0" in summary
    assert "trace_operations_count" not in summary
    path = tmp_path / "profile.json"
    for destination in (path, str(path)):
        path.write_text("old contents", encoding="utf-8")
        assert result.to_json(destination) is None
        assert json.loads(path.read_text(encoding="utf-8")) == {
            "totals": {
                "logical_qubits": 2,
                "physical_qubits": 20,
                "total_runtime_sec": 1.75,
                "total_operations_count": 6,
                "calls_per_operation": result.totals.calls_per_operation,
                "runtime_sec_per_operation": result.totals.runtime_sec_per_operation,
            },
            "profiles": {"profiles": [], "aggregation_window_sec": None},
            "trace": None,
            "infos": [],
        }
    missing_path = tmp_path / "missing" / "profile.json"
    with pytest.raises(OSError, match="Failed to create") as error:
        result.to_json(missing_path)
    assert str(missing_path) in str(error.value)


def test_qsharp_application_profile_no_gate_times():
    """Test profiling Q# code through the Python API."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    app = QSharpApplication(
        """
        {
            use qs = Qubit();
            X(qs);
        }
        """
    )
    profiler = SequentialCountingProfiler(
        physical_qubits_per_logical=10,
        gate_time_sec={},
    )

    result = app.profile(profiler)

    assert isinstance(profiler, Profiler)
    assert isinstance(result, ProfilerResult)
    assert result.totals.calls_per_operation == {}
    assert result.totals.runtime_sec_per_operation == {}
    assert result.totals.total_runtime_sec == 0.0
    assert result.profiles.profiles == []
    assert result.infos == ["These gates did not have time configured, assumed 0: [x]"]



@pytest.mark.parametrize("use_qir", [False, True])
def test_temporal_profiler(use_qir: bool, tmp_path: Path) -> None:
    """Profile gate dependencies and seeded T-state production from Python."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    app = QSharpApplication(
        """
        {
            use qs = Qubit[2];
            X(qs[0]);
            H(qs[0]);
            H(qs[1]);
            CNOT(qs[0], qs[1]);
            T(qs[0]);
            Adjoint T(qs[1]);
            [M(qs[0]), M(qs[1])]
        }
        """
    )
    config = ProfilerConfig(
        clifford_gate_time_nanos=5000,
        measurment_time_nanos=1000,
        aggregation_window_nanos=10_000,
        t_factories_count=5,
        seed=42,
        record_trace=True,
        ecc_distance=7,
    )
    profiler = TemporalProfiler(config)

    result = app.profile(profiler, use_qir=use_qir)
    summary = repr(result)
    assert summary.startswith("ProfilerResult { totals: TotalCounts {")

    path = tmp_path / "temporal.json"
    result.to_json(path)
    data = json.loads(path.read_text(encoding="utf-8"))
    assert data["totals"]["logical_qubits"] == result.totals.logical_qubits
 