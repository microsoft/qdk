# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path

from qdk.qre._dollar_cost import DollarCostModelFromSpec

COST_SPEC_PATH = str(
    Path(__file__).resolve().parents[4] / "samples/qre/example_cost_spec.json"
)


def test_compute_dollar_cost_from_json_spec():
    cost_model = DollarCostModelFromSpec(COST_SPEC_PATH)
    cost = cost_model.cost_usd(qubits=1000, runtime_nanos=3_600_000_000_000)
    assert cost == 68.49
