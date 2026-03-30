# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import os

import pytest

from qsharp.estimator import LogicalCounts
from qsharp.qre import (
    PSSPC,
    LatticeSurgery,
    estimate,
)
from qsharp.qre.application import QSharpApplication
from qsharp.qre.models import (
    SurfaceCode,
    GateBased,
    RoundBasedFactory,
    TwoDimensionalYokedSurfaceCode,
)

from .conftest import ExampleFactory


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
