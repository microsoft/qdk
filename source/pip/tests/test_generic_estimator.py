# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
import qsharp


class SampleAlgorithm:
    def __init__(self, *, qubits=10, depth=20, magic_states=[100]):
        self.qubits = qubits
        self.depth = depth
        self.magic_states = magic_states

    def logical_qubits(self):
        return self.qubits

    def logical_depth(self, budget):
        return self.depth

    def num_magic_states(self, budget, index):
        return self.magic_states[index]


class SampleCode:
    def physical_qubits(self, param):
        return 2 * param**2

    def logical_qubits(self, param):
        return 1

    def logical_cycle_time(self, qubit, param):
        return 6 * qubit["gate_time"] * param

    def logical_error_rate(self, qubit, param):
        return 0.03 * (qubit["error_rate"] / 0.01) ** ((param + 1) // 2)

    def code_parameter_range(self):
        return [1, 2, 3]

    def code_parameter_cmp(self, qubit, p1, p2):
        return -1 if p1 < p2 else (1 if p1 > p2 else 0)


class SampleFactory:
    def find_factories(self, code, qubit, target_error_rate):
        assert isinstance(code, SampleCode)
        assert isinstance(qubit, dict)
        assert qubit == sample_qubit()

        return [{"physical_qubits": 100, "duration": 1000}]


class SampleFactoryBuilder:
    def __init__(self):
        # Key to index into magic gate error rate in qubit
        self.gate_error = "error_rate"
        self.max_rounds = 3

    def distillation_units(self, code, qubit, max_code_parameter):
        return [
            {
                "num_input_states": 15,
                "physical_qubits": lambda _: 50,
                "duration": lambda _: 500,
                "output_error_rate": lambda input_error_rate: 35 * input_error_rate**3,
                "failure_probability": lambda input_error_rate: 15 * input_error_rate,
            }
        ]


def sample_qubit():
    return {"gate_time": 50, "error_rate": 1e-4}


def test_wrong_input():
    pytest.raises(
        AttributeError, qsharp.estimate_custom, 42, sample_qubit(), SampleCode()
    )

    # Catches missing methods in SampleAlgorithm
    for method_name in ["logical_qubits", "logical_depth", "num_magic_states"]:
        method = getattr(SampleAlgorithm, method_name)
        delattr(SampleAlgorithm, method_name)
        pytest.raises(
            AttributeError,
            qsharp.estimate_custom,
            SampleAlgorithm(),
            sample_qubit(),
            SampleCode(),
        )
        setattr(SampleAlgorithm, method_name, method)

    # Catches missing methods in SampleCode
    for method_name in [
        "physical_qubits",
        "logical_qubits",
        "logical_cycle_time",
        "logical_error_rate",
        "code_parameter_range",
        "code_parameter_cmp",
    ]:
        method = getattr(SampleCode, method_name)
        delattr(SampleCode, method_name)
        pytest.raises(
            AttributeError,
            qsharp.estimate_custom,
            SampleAlgorithm(),
            sample_qubit(),
            SampleCode(),
        )
        setattr(SampleCode, method_name, method)

    # Catches wrong type for method
    method = SampleAlgorithm.logical_qubits
    SampleAlgorithm.logical_qubits = "not a method"
    pytest.raises(
        TypeError,
        qsharp.estimate_custom,
        SampleAlgorithm(),
        sample_qubit(),
        SampleCode(),
    )
    SampleAlgorithm.logical_qubits = method

    # Catches wrong signature for method
    method = SampleAlgorithm.logical_depth
    SampleAlgorithm.logical_depth = lambda self, budget, extra: 20
    pytest.raises(
        RuntimeError,
        qsharp.estimate_custom,
        SampleAlgorithm(),
        sample_qubit(),
        SampleCode(),
    )
    SampleAlgorithm.logical_depth = lambda self: 20
    pytest.raises(
        RuntimeError,
        qsharp.estimate_custom,
        SampleAlgorithm(),
        sample_qubit(),
        SampleCode(),
    )
    SampleAlgorithm.logical_depth = method


def test_estimate_without_factories():
    result = qsharp.estimate_custom(SampleAlgorithm(), sample_qubit(), SampleCode())

    assert len(result["factoryParts"]) == 0
    assert len(result["layoutOverhead"]["numMagicStates"]) == 0
    assert result["runtime"] == 18000
    assert result["physicalQubits"] == 180

    assert "executionStats" in result
    assert "timeAlgorithm" in result["executionStats"]
    assert "timeEstimation" in result["executionStats"]


def test_with_single_factory():
    result = qsharp.estimate_custom(
        SampleAlgorithm(), sample_qubit(), SampleCode(), [SampleFactory()]
    )
    assert len(result["factoryParts"]) == 1
    assert len(result["layoutOverhead"]["numMagicStates"]) == 1

    assert "physical_qubits" in result["factoryParts"][0]["factory"]
    assert "duration" in result["factoryParts"][0]["factory"]


def test_with_multiple_factories():
    result = qsharp.estimate_custom(
        SampleAlgorithm(magic_states=[50, 100, 200]),
        sample_qubit(),
        SampleCode(),
        [SampleFactory()] * 3,
    )
    assert len(result["factoryParts"]) == 3
    assert len(result["layoutOverhead"]["numMagicStates"]) == 3

    for factory_part in result["factoryParts"]:
        assert "physical_qubits" in factory_part["factory"]
        assert "duration" in factory_part["factory"]


def test_with_factory_builder():
    result = qsharp.estimate_custom(
        SampleAlgorithm(),
        sample_qubit(),
        SampleCode(),
        [SampleFactoryBuilder()],
    )

    assert len(result["factoryParts"]) == 1
    assert len(result["layoutOverhead"]["numMagicStates"]) == 1

    assert "physical_qubits" in result["factoryParts"][0]["factory"]
    assert "duration" in result["factoryParts"][0]["factory"]


def test_prune_error_budget():
    result = qsharp.estimate_custom(SampleAlgorithm(), sample_qubit(), SampleCode())

    assert abs(result["errorBudget"]["logical"] - 0.01 / 3) < 1e-6
    assert abs(result["errorBudget"]["rotations"] - 0.01 / 3) < 1e-6
    assert abs(result["errorBudget"]["tstates"] - 0.01 / 3) < 1e-6

    result = qsharp.estimate_custom(
        SampleAlgorithm(), sample_qubit(), SampleCode(), error_budget=0.1
    )

    assert abs(result["errorBudget"]["logical"] - 0.1 / 3) < 1e-6
    assert abs(result["errorBudget"]["rotations"] - 0.1 / 3) < 1e-6
    assert abs(result["errorBudget"]["tstates"] - 0.1 / 3) < 1e-6

    def _prune_error_budget(self, budget, _):
        rotations = budget["rotations"]
        budget["logical"] += rotations / 2
        budget["magic_states"] += rotations / 2
        budget["rotations"] = 0.0

    SampleAlgorithm.prune_error_budget = _prune_error_budget

    result = qsharp.estimate_custom(SampleAlgorithm(), sample_qubit(), SampleCode())

    assert abs(result["errorBudget"]["logical"] - 0.01 / 2) < 1e-6
    assert abs(result["errorBudget"]["rotations"]) < 1e-6
    assert abs(result["errorBudget"]["tstates"] - 0.01 / 2) < 1e-6

    del SampleAlgorithm.prune_error_budget
