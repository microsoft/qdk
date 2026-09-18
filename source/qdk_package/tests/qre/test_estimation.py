# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import os
from types import SimpleNamespace
from typing import cast

import pytest


from qdk.estimator import LogicalCounts
from qdk.qre import (
    LatticeSurgery,
    PSSPC,
    estimate,
)
from qdk.qre._estimation import _EstimationCollection, _zero_results_warning_message
from qdk.qre.instruction_ids import T
from qdk.qre.application import QSharpApplication
from qdk.qre.models import (
    SurfaceCode,
    GateBased,
    RoundBasedFactory,
    TwoDimensionalYokedSurfaceCode,
)

from .conftest import ExampleFactory


def test_zero_results_warning_suggests_expanding_isa_search_space():
    collection = cast(
        _EstimationCollection,
        SimpleNamespace(
            total_jobs=6,
            successful_estimates=0,
            maximum_error_exceeded=6,
            minimum_error_for_success=0.125,
            missing_instruction_ids=[],
            errors=[],
        ),
    )
    warning = _zero_results_warning_message(
        collection,
        num_traces=2,
        num_isas=3,
    )

    assert "minimum error" in warning
    assert "0.125" in warning
    assert "max_error were greater than this value" in warning
    assert "No compatible trace/ISA candidates" not in warning


def test_zero_results_warning_reports_missing_instruction_ids():
    collection = cast(
        _EstimationCollection,
        SimpleNamespace(
            total_jobs=0,
            successful_estimates=0,
            maximum_error_exceeded=0,
            minimum_error_for_success=None,
            missing_instruction_ids=[T],
            errors=[],
        ),
    )
    warning = _zero_results_warning_message(
        collection,
        num_traces=1,
        num_isas=2,
    )

    assert f"T ({T})" in warning
    assert "trace transforms or ISA transforms" in warning


def test_estimation_max_error():
    """Test that estimation results respect the max_error constraint."""
    app = QSharpApplication(LogicalCounts({"numQubits": 100, "measurementCount": 100}))
    arch = GateBased(gate_time=50, measurement_time=100)

    for max_error in [1e-1, 1e-2, 1e-3, 1e-4]:
        results = estimate(
            app,
            arch,
            SurfaceCode.q() * ExampleFactory.q(),
            PSSPC.q() * LatticeSurgery.q(),
            max_error=max_error,
        )

        assert len(results) == 1
        assert next(iter(results)).error <= max_error


@pytest.mark.skipif(
    "SLOW_TESTS" not in os.environ,
    reason="turn on slow tests by setting SLOW_TESTS=1 in the environment",
)
@pytest.mark.parametrize(
    "post_process, use_graph",
    [
        (False, False),
        (True, False),
        (False, True),
        (True, True),
    ],
)
def test_estimation_methods(post_process, use_graph):
    """Test all combinations of post_process and use_graph estimation paths."""
    counts = LogicalCounts(
        {
            "numQubits": 1000,
            "tCount": 1_500_000,
            "rotationCount": 0,
            "rotationDepth": 0,
            "cczCount": 1_000_000_000,
            "ccixCount": 0,
            "measurementCount": 25_000_000,
            "numComputeQubits": 200,
            "readFromMemoryCount": 30_000_000,
            "writeToMemoryCount": 30_000_000,
        }
    )

    trace_query = PSSPC.q() * LatticeSurgery.q(slow_down_factor=[1.0, 2.0])
    isa_query = (
        SurfaceCode.q()
        * RoundBasedFactory.q()
        * TwoDimensionalYokedSurfaceCode.q(source=SurfaceCode.q())
    )

    app = QSharpApplication(counts)
    arch = GateBased(gate_time=50, measurement_time=100)

    results = estimate(
        app,
        arch,
        isa_query,
        trace_query,
        max_error=1 / 3,
        post_process=post_process,
        use_graph=use_graph,
    )
    results.add_factory_summary_column()

    assert [(result.qubits, result.runtime) for result in results] == [
        (238707, 23997050000000),
        (240407, 11998525000000),
    ]

    print()
    print(results.stats)
