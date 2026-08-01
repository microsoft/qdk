# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from pathlib import Path

from qdk.qre._dollar_cost import compute_dollar_cost
from qdk.qre._instruction import InstructionSource
from qdk.qre._json_specs import JsonSpec, OpExUnit, ScaledUnit, System
from qdk.qre._results import EstimationTableEntry


COST_SPEC_PATH = Path(__file__).resolve().parents[4] / "samples/qre/example_cost_spec.json"


def test_json_spec_from_file():
    spec = JsonSpec.from_file(str(COST_SPEC_PATH))

    assert isinstance(spec.system, System)
    assert spec.system.name == "example_cost_spec"
    assert spec.system.qubits_per_node == 16_000
    assert isinstance(spec.scaled_units[0], ScaledUnit)
    assert spec.scaled_units[0].units_per_control_line == 1
    assert isinstance(spec.opex[0], OpExUnit)
    assert spec.opex[0].cost == 10_000_000


def test_compute_dollar_cost_from_json_spec():
    entry = EstimationTableEntry(
        qubits=16_000,
        runtime=3_600_000_000_000,
        error=0.0,
        source=InstructionSource(),
    )

    assert compute_dollar_cost(str(COST_SPEC_PATH), entry) == 408_675.80
