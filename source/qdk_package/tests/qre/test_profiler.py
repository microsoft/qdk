# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
from pathlib import Path
from unittest.mock import patch

import pyqir
import pytest
from qdk import Context, openqasm, qsharp
from qdk.estimator import LogicalCounts
from qdk.qre import (
    DemoModel1,
    Profiler,
    ProfilerResult,
    SequentialCountingProfiler,
    TemporalProfiler,
    TemporalProfilerConfig,
    TemporalProfiles,
    TotalCounts,
)
from qdk.qre.application import OpenQASMApplication, QIRApplication, QSharpApplication
from qdk.qre.profiler import QREProfiler


@pytest.mark.parametrize("as_bitcode", [False, True], ids=["text", "bitcode"])
def test_qir_application_profiler(as_bitcode: bool) -> None:
    """Profile equivalent QIR supplied as LLVM text or binary bitcode."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    qir = str(qsharp.compile("{ use q = Qubit(); X(q); M(q) }"))
    app = QIRApplication(
        pyqir.Module.from_ir(pyqir.Context(), qir).bitcode if as_bitcode else qir
    )
    profiler = QREProfiler(SequentialCountingProfiler(10, {"x": 0.25, "mz": 0.5}))

    result = profiler.profile(app)

    assert result.totals.calls_per_operation == {"x": 1, "mz": 1}
    assert result.totals.total_runtime_sec == 0.75


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

    qre_profiler = QREProfiler(profiler)
    result = qre_profiler.profile(app, use_qir=use_qir)

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
    model = DemoModel1(
        clifford_gate_time_nanos=5000,
        measurment_time_nanos=1000,
        t_factories_count=5,
        ecc_distance=7,
    )
    config = TemporalProfilerConfig(
        aggregation_window_nanos=10_000,
        seed=42,
        record_trace=True,
    )
    profiler = TemporalProfiler(model, config)
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
    assert result.totals.physical_qubits == 28 * (2 * 7**2 - 1)
    assert data["totals"]["logical_compute_qubits"] == 3
    assert data["totals"]["error"] == result.totals.error
    assert 0.0 < result.totals.error < 0.33


def test_temporal_profiler_configuration() -> None:
    """Keep hardware parameters separate from the three profiler settings."""
    model = DemoModel1(clifford_gate_time_nanos=5000, measurment_time_nanos=1000)
    assert model.clifford_gate_time_nanos == 5000
    assert model.measurment_time_nanos == 1000
    assert model.t_factories_count == 10
    assert model.ecc_distance == 9

    config = TemporalProfilerConfig()
    assert {name for name in dir(config) if not name.startswith("_")} == {
        "aggregation_window_nanos",
        "seed",
        "record_trace",
    }
    assert config.aggregation_window_nanos == 1_000_000_000
    assert config.seed is None
    assert config.record_trace is False
    with pytest.raises(ValueError, match="aggregation_window_nanos"):
        TemporalProfilerConfig(aggregation_window_nanos=0)


@pytest.mark.parametrize("field", ["t_factories_count", "ecc_distance"])
def test_demo_model_invalid_parameters(field: str) -> None:
    """Preserve positive factory-count and code-distance validation on the model."""
    with pytest.raises(ValueError, match=field):
        DemoModel1(
            clifford_gate_time_nanos=5000,
            measurment_time_nanos=1000,
            **{field: 0},
        )


@pytest.mark.parametrize("use_qir", [False, True])
@pytest.mark.parametrize(
    "source",
    [
        """
        OPENQASM 2.0;
        include "qelib1.inc";
        qreg q[1];
        creg c[1];
        x q[0];
        measure q[0] -> c[0];
        """,
        """
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        output bit c;
        x q;
        c = measure q;
        """,
    ],
    ids=["openqasm2", "openqasm3"],
)
def test_openqasm_profiler(source: str, use_qir: bool) -> None:
    """Profile both OpenQASM versions without changing the application source."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    app = OpenQASMApplication(source)
    backend = SequentialCountingProfiler(10, {"x": 0.25, "mz": 0.5})
    backend.add_adapter("result_adapter")
    profiler = QREProfiler(backend)

    with patch.object(
        profiler, "_profile_qir", wraps=profiler._profile_qir
    ) as profile_qir:
        result = profiler.profile(app, use_qir=use_qir)

    assert profile_qir.call_count == int(use_qir)
    assert app.program == source
    assert result.totals.logical_compute_qubits == 1
    assert result.totals.physical_qubits == 10
    assert result.totals.calls_per_operation == {"x": 1, "mz": 1}
    assert result.totals.total_operations_count == 2
    assert result.totals.total_runtime_sec == 0.75
    assert result.infos == []


@pytest.mark.parametrize("use_qir", [False, True])
@pytest.mark.parametrize("as_callable", [False, True], ids=["source", "callable"])
def test_openqasm_profiler_arguments(use_qir: bool, as_callable: bool) -> None:
    """Preserve classical arguments and an imported callable's owning context."""
    qsharp.init(target_profile=qsharp.TargetProfile.Base)
    source = """
        OPENQASM 3.0;
        include "stdgates.inc";
        input int n;
        qubit q;
        output bit c;
        for int i in [0:n-1] {
            x q;
        }
        c = measure q;
        """
    program = source
    if as_callable:
        context = Context(target_profile=qsharp.TargetProfile.Base)
        context.import_openqasm(
            source, name="ProfileWithInput", program_type=openqasm.ProgramType.File
        )
        program = context.code.qasm_import.ProfileWithInput
    app = OpenQASMApplication(program, args=(3,))
    backend = SequentialCountingProfiler(10, {"x": 0.25, "mz": 0.5})
    backend.add_adapter("result_adapter")
    profiler = QREProfiler(backend)

    result = profiler.profile(app, use_qir=use_qir)

    assert result.totals.calls_per_operation == {"x": 3, "mz": 1}
    assert result.totals.total_operations_count == 4
    assert result.totals.total_runtime_sec == 1.25
    assert result.infos == []


@pytest.mark.parametrize("use_qir", [False, True])
def test_openqasm_profiler_measurement_control_flow(use_qir: bool) -> None:
    """Execute exactly one measurement-dependent branch rather than counting both."""
    qsharp.init(target_profile=qsharp.TargetProfile.Adaptive)
    app = OpenQASMApplication("""
        OPENQASM 3.0;
        include "stdgates.inc";
        qubit q;
        output bit first;
        output bit last;
        first = measure q;
        if (first) {
            x q;
        } else {
            z q;
        }
        last = measure q;
        """)
    profiler = SequentialCountingProfiler(10, {"x": 0.25, "z": 0.25, "mz": 0.5})
    profiler.add_adapter("result_adapter")

    result = QREProfiler(profiler).profile(app, use_qir=use_qir)

    calls = result.totals.calls_per_operation
    assert calls["mz"] == 2
    assert calls.get("x", 0) + calls.get("z", 0) == 1
    assert result.totals.total_operations_count == 3
    assert result.totals.total_runtime_sec == 1.25
    assert result.infos == []


@pytest.mark.parametrize("use_qir", [False, True])
@pytest.mark.parametrize("application_type", [QSharpApplication, OpenQASMApplication])
def test_profiler_rejects_logical_counts(
    application_type: type[QSharpApplication] | type[OpenQASMApplication],
    use_qir: bool,
) -> None:
    """Reject aggregate counts because they do not contain executable instructions."""
    app = application_type(LogicalCounts({"numQubits": 1, "tCount": 1}))
    profiler = QREProfiler(SequentialCountingProfiler(10, {}))

    with pytest.raises(TypeError, match="LogicalCounts"):
        profiler.profile(app, use_qir=use_qir)
