# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import qsharp
from qsharp.estimator import EstimatorParams, QubitParams, QECScheme, LogicalCounts


def test_qsharp_estimation() -> None:
    source = """{{
             use qs = Qubit[10];
             for q in qs {{
                 T(q);
                 M(q);
             }}
             }}"""

    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    res = qsharp.estimate(source)
    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 10,
            "tCount": 10,
            "rotationCount": 0,
            "rotationDepth": 0,
            "cczCount": 0,
            "measurementCount": 10,
        }
    )

    res_logical = qsharp.logical_counts(source)
    assert res_logical == res.logical_counts


def test_qsharp_estimation_from_precalculated_counts() -> None:
    source = """{{
             import Std.ResourceEstimation.*;
             use qubits = Qubit[12581];
             AccountForEstimates(
                 [TCount(12), RotationCount(12), RotationDepth(12),
                 CczCount(3731607428), MeasurementCount(1078154040)],
                 PSSPCLayout(), qubits);
             }}"""

    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    res = qsharp.estimate(source)

    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 12581,
            "tCount": 12,
            "rotationCount": 12,
            "rotationDepth": 12,
            "cczCount": 3731607428,
            "measurementCount": 1078154040,
        }
    )

    res_logical = qsharp.logical_counts(source)

    assert res_logical == res.logical_counts


def test_qsharp_estimation_with_single_params() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)

    params = EstimatorParams()
    params.error_budget = 0.333
    params.qubit_params.name = QubitParams.MAJ_NS_E4
    assert params.as_dict() == {
        "qubitParams": {"name": "qubit_maj_ns_e4"},
        "errorBudget": 0.333,
    }

    res = qsharp.estimate(
        """{{
        use qs = Qubit[10];
        for q in qs {{
            T(q);
            M(q);
        }}
        }}""",
        params=params,
    )

    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res["jobParams"]["qubitParams"]["name"] == "qubit_maj_ns_e4"
    assert res.logical_counts == LogicalCounts(
        {
            "numQubits": 10,
            "tCount": 10,
            "rotationCount": 0,
            "rotationDepth": 0,
            "cczCount": 0,
            "measurementCount": 10,
        }
    )


def test_qsharp_estimation_with_multiple_params() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)

    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    res = qsharp.estimate(
        """{{
        use qs = Qubit[10];
        for q in qs {{
            T(q);
            M(q);
        }}
        }}""",
        params=params,
    )

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 10,
                "tCount": 10,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 10,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE


def test_qsharp_estimation_with_multiple_params_from_python_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)

    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    qsharp.eval(
        """
        operation Test() : Unit {
            use qs = Qubit[10];
            for q in qs {
                T(q);
                M(q);
            }
        }
        """
    )

    res = qsharp.estimate(qsharp.code.Test, params=params)

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 10,
                "tCount": 10,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 10,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE

    res_logical = qsharp.logical_counts(qsharp.code.Test)
    assert res_logical == res[0]["logicalCounts"]


def test_qsharp_estimation_with_multiple_params_from_qsharp_callable() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)

    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    qsharp.eval(
        """
        operation Test() : Unit {
            use qs = Qubit[10];
            for q in qs {
                T(q);
                M(q);
            }
        }
        """
    )

    test = qsharp.eval("Test")
    res = qsharp.estimate(test, params=params)

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 10,
                "tCount": 10,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 10,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE

    res_logical = qsharp.logical_counts(test)
    assert res_logical == res[0]["logicalCounts"]


def test_qsharp_estimation_with_multiple_params_from_python_callable_with_arg() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)

    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    qsharp.eval(
        """
        operation Test(nQubits : Int) : Unit {
            use qs = Qubit[nQubits];
            for q in qs {
                T(q);
                M(q);
            }
        }
        """
    )

    res = qsharp.estimate(qsharp.code.Test, params, 7)

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 7,
                "tCount": 7,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 7,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE

    res_logical = qsharp.logical_counts(qsharp.code.Test, 7)
    assert res_logical == res[0]["logicalCounts"]


def test_qsharp_estimation_with_multiple_params_from_qsharp_callable_with_arg() -> None:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)

    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    assert params.as_dict() == {
        "items": [
            {
                "qubitParams": {"name": "qubit_gate_us_e3"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_gate_us_e4"},
                "errorBudget": 0.333,
            },
            {
                "qubitParams": {"name": "qubit_maj_ns_e6"},
                "qecScheme": {"name": "floquet_code"},
                "errorBudget": 0.333,
            },
        ],
        "resumeAfterFailedItem": True,
    }

    qsharp.eval(
        """
        operation Test(nQubits : Int) : Unit {
            use qs = Qubit[nQubits];
            for q in qs {
                T(q);
                M(q);
            }
        }
        """
    )

    test = qsharp.eval("Test")
    res = qsharp.estimate(test, params, 7)

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == LogicalCounts(
            {
                "numQubits": 7,
                "tCount": 7,
                "rotationCount": 0,
                "rotationDepth": 0,
                "cczCount": 0,
                "measurementCount": 7,
            }
        )
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE

    res_logical = qsharp.logical_counts(test, 7)
    assert res_logical == res[0]["logicalCounts"]


def test_estimation_from_logical_counts() -> None:
    logical_counts = LogicalCounts(
        {
            "numQubits": 12581,
            "tCount": 12,
            "rotationCount": 12,
            "rotationDepth": 12,
            "cczCount": 3731607428,
            "measurementCount": 1078154040,
        }
    )
    res = logical_counts.estimate()

    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res.logical_counts == logical_counts


def test_estimation_from_logical_counts_with_single_params() -> None:
    logical_counts = LogicalCounts(
        {
            "numQubits": 12581,
            "tCount": 12,
            "rotationCount": 12,
            "rotationDepth": 12,
            "cczCount": 3731607428,
            "measurementCount": 1078154040,
        }
    )
    params = EstimatorParams()
    params.error_budget = 0.333
    params.qubit_params.name = QubitParams.MAJ_NS_E4
    res = logical_counts.estimate(params=params)

    assert res["status"] == "success"
    assert res["physicalCounts"] is not None
    assert res["jobParams"]["qubitParams"]["name"] == "qubit_maj_ns_e4"
    assert res.logical_counts == logical_counts
    assert "frontierEntries" not in res


def test_estimation_from_logical_counts_with_multiple_params() -> None:
    logical_counts = LogicalCounts(
        {
            "numQubits": 12581,
            "tCount": 12,
            "rotationCount": 12,
            "rotationDepth": 12,
            "cczCount": 3731607428,
            "measurementCount": 1078154040,
        }
    )
    params = EstimatorParams(3)
    params.items[0].qubit_params.name = QubitParams.GATE_US_E3
    params.items[0].error_budget = 0.333
    params.items[1].qubit_params.name = QubitParams.GATE_US_E4
    params.items[1].error_budget = 0.333
    params.items[2].qubit_params.name = QubitParams.MAJ_NS_E6
    params.items[2].qec_scheme.name = QECScheme.FLOQUET_CODE
    params.items[2].error_budget = 0.333
    res = logical_counts.estimate(params=params)

    for idx in res:
        assert res[idx]["status"] == "success"
        assert res[idx]["physicalCounts"] is not None
        assert (
            res[idx]["jobParams"]["qubitParams"]["name"]
            == params.items[idx].qubit_params.name
        )
        assert res[idx]["logicalCounts"] == logical_counts
    assert res[2]["jobParams"]["qecScheme"]["name"] == QECScheme.FLOQUET_CODE


def test_building_frontier_from_logical_counts_with_single_params() -> None:
    logical_counts = LogicalCounts(
        {
            "numQubits": 12581,
            "tCount": 12,
            "rotationCount": 12,
            "rotationDepth": 12,
            "cczCount": 3731607428,
            "measurementCount": 1078154040,
        }
    )
    params = EstimatorParams()
    params.error_budget = 0.333
    params.qubit_params.name = QubitParams.MAJ_NS_E4
    params.estimate_type = "frontier"
    res = logical_counts.estimate(params=params)

    assert res["status"] == "success"
    assert "physicalCounts" not in res
    assert "physicalCountsFormatted" not in res
    assert res["jobParams"]["qubitParams"]["name"] == "qubit_maj_ns_e4"
    assert res.logical_counts == logical_counts
    assert res["frontierEntries"] is not None
    assert len(res["frontierEntries"]) > 0
    first_entry = res["frontierEntries"][0]
    assert first_entry["physicalCounts"] is not None
    assert first_entry["physicalCountsFormatted"] is not None
