# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path

import pytest
from qdk import qsharp
from qdk.qre import ProfilerResult, SequentialCountingProfiler
from qdk.qre.profiler import QREProfiler


# TODO: remove this test after we stabilize QREProfiler API.
@pytest.mark.parametrize(
    "target_profile",
    [qsharp.TargetProfile.Base, qsharp.TargetProfile.Adaptive],
    ids=["base", "adaptive"],
)
def test_profile_qir(target_profile: qsharp.TargetProfile, tmp_path: Path) -> None:
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

    result = QREProfiler.profile_qir(str(qir_path), profiler)

    assert isinstance(result, ProfilerResult)
    assert result.totals.logical_compute_qubits == 1
    assert result.totals.physical_qubits == 10
    assert result.totals.calls_per_operation == {"x": 1, "mz": 1}
    assert result.totals.total_operations_count == 2
    assert result.totals.total_runtime_sec == 0.75
    assert result.infos == []
