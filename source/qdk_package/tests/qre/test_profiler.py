# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
from pathlib import Path

import pytest
from qdk import qsharp
from qdk.qre import (
    Profiler,
    ProfilerResult,
    SequentialCountingProfiler,
    TemporalProfiler,
    TemporalProfilerConfig,
    TemporalProfiles,
    TotalCounts,
)
from qdk.qre.application import QSharpApplication
from qdk.qre.profiler import QREProfiler


@pytest.mark.parametrize(
    "target_profile",
    [qsharp.TargetProfile.Base, qsharp.TargetProfile.Adaptive],
    ids=["base", "adaptive"],
)
def test_profile_qir_file(target_profile: qsharp.TargetProfile, tmp_path: Path) -> None:
    """Smoke-test profiling a compiled QIR file under both target profiles."""
    qsharp.init(target_profile=target_profile)
    qir = qsharp.compile("""
        {
            use q = Qubit();
            X(q);
            M(q)
        }
        """)
    qir_path = tmp_path / "program.ll"
    qir_path.write_text(str(qir), encoding="utf-8")
    profiler = SequentialCountingProfiler(
        physical_qubits_per_logical=10,
        gate_time_sec={"x": 0.25, "mz": 0.5},
    )

    result = QREProfiler(profiler).profile_qir_file(str(qir_path))

    assert isinstance(result, ProfilerResult)
    assert result.totals.logical_compute_qubits == 1
    assert result.totals.physical_qubits == 10
    assert result.totals.calls_per_operation == {"x": 1, "mz": 1}
    assert result.totals.total_operations_count == 2
    assert result.totals.total_runtime_sec == 0.75
    assert result.infos == []


@pytest.mark.parametrize("use_qir", [False, True])
def test_sequential_counting_profiler(use_qir: bool, tmp_path: Path) -> None:
    """Check sequential counts, timing, and result serialization through QREProfiler."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    app = QSharpApplication("""
        {
            use qs = Qubit[2];
            X(qs[0]);
            X(qs[1]);
            [M(qs[0]),M(qs[1])]
        }
        """)
    profiler = SequentialCountingProfiler(
        physical_qubits_per_logical=10,
        gate_time_sec={"x": 0.25, "mz": 0.5},
    )
    profiler.add_adapter("result_adapter")

    result = QREProfiler(profiler).profile(app, use_qir=use_qir)

    assert isinstance(profiler, Profiler)
    assert isinstance(result, ProfilerResult)
    assert isinstance(result.totals, TotalCounts)
    assert result.totals.logical_qubits == 2
    assert result.totals.logical_compute_qubits == 2
    assert result.totals.physical_qubits == 20
    assert result.totals.calls_per_operation == {"x": 2, "mz": 2}
    assert result.totals.runtime_sec_per_operation == {"x": 0.5, "mz": 1.0}
    assert result.totals.total_runtime_sec == 1.5
    assert isinstance(result.profiles, TemporalProfiles)
    assert result.profiles.profiles == []
    assert result.profiles.aggregation_window_sec is None
    assert result.infos == []
    summary = repr(result)
    assert summary.startswith("ProfilerResult { totals: TotalCounts {")
    assert "logical_qubits: 2" in summary
    assert "physical_qubits: 20" in summary
    assert "total_operations_count: 4" in summary
    assert "time_series_count: 0" in summary
    assert "trace_operations_count" not in summary
    path = tmp_path / "profile.json"
    for destination in (path, str(path)):
        path.write_text("old contents", encoding="utf-8")
        assert result.to_json(destination) is None
        assert json.loads(path.read_text(encoding="utf-8")) == {
            "totals": {
                "logical_qubits": 2,
                "logical_compute_qubits": 2,
                "physical_qubits": 20,
                "total_runtime_sec": 1.5,
                "total_operations_count": 4,
                "calls_per_operation": result.totals.calls_per_operation,
                "runtime_sec_per_operation": result.totals.runtime_sec_per_operation,
                "error": 0.0,
            },
            "profiles": {"profiles": [], "aggregation_window_sec": None},
            "trace": None,
            "infos": [],
        }
    missing_path = tmp_path / "missing" / "profile.json"
    with pytest.raises(OSError, match="Failed to create") as error:
        result.to_json(missing_path)
    assert str(missing_path) in str(error.value)


def test_sequential_counting_profiler_no_gate_times() -> None:
    """Check the missing-timing diagnostic through QREProfiler."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    app = QSharpApplication("""
        {
            use qs = Qubit();
            X(qs);
        }
        """)
    profiler = SequentialCountingProfiler(
        physical_qubits_per_logical=10,
        gate_time_sec={},
    )

    result = QREProfiler(profiler).profile(app)

    assert isinstance(profiler, Profiler)
    assert isinstance(result, ProfilerResult)
    assert result.totals.calls_per_operation == {}
    assert result.totals.runtime_sec_per_operation == {}
    assert result.totals.total_runtime_sec == 0.0
    assert result.profiles.profiles == []
    assert result.infos == ["These gates did not have time configured, assumed 0: [x]"]


@pytest.mark.parametrize("use_qir", [False, True])
def test_temporal_profiler(use_qir: bool, tmp_path: Path) -> None:
    """Profile CCX decomposition, gate dependencies, and seeded T-state production."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    app = QSharpApplication("""
        {
            use qs = Qubit[3];
            X(qs[0]);
            H(qs[0]);
            H(qs[1]);
            CNOT(qs[0], qs[1]);
            CCNOT(qs[0], qs[1], qs[2]);
            T(qs[0]);
            Adjoint T(qs[1]);
            [M(qs[0]), M(qs[1]), M(qs[2])]
        }
        """)
    config = TemporalProfilerConfig(
        clifford_gate_time_nanos=5000,
        measurment_time_nanos=1000,
        aggregation_window_nanos=10_000,
        t_factories_count=5,
        seed=42,
        record_trace=True,
        ecc_distance=7,
    )
    profiler = TemporalProfiler(config)
    profiler.add_adapter("result_adapter")
    profiler.add_adapter("transpiler")
    profiler = QREProfiler(profiler)

    result = profiler.profile(app, use_qir=use_qir)
    summary = repr(result)
    assert summary.startswith("ProfilerResult { totals: TotalCounts {")

    path = tmp_path / "temporal.json"
    result.to_json(path)
    data = json.loads(path.read_text(encoding="utf-8"))
    assert data["totals"]["logical_qubits"] == result.totals.logical_qubits
    assert result.totals.logical_qubits == 28
    assert result.totals.logical_compute_qubits == 3
    assert data["totals"]["logical_compute_qubits"] == 3
    assert data["totals"]["error"] == result.totals.error
    assert 0.0 < result.totals.error < 0.33
