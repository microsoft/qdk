# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
from pathlib import Path

import pytest
from qdk.qre.dollar_cost import CostSpec, DollarCostModelFromSpec

COST_SPEC_PATH = str(
    Path(__file__).resolve().parents[4] / "samples/qre/example_cost_spec.json"
)


def test_compute_dollar_cost_from_json_spec():
    cost_model = DollarCostModelFromSpec(COST_SPEC_PATH)
    cost = cost_model.cost_usd(qubits=1000, runtime_nanos=3_600_000_000_000)
    assert cost == 68.49


def test_cost_spec_minimal_required_fields():
    spec = CostSpec.from_dict(
        {
            "system": {
                "name": "test",
                "lifetime_in_years": 1,
                "uptime": 1,
            }
        }
    )

    assert spec.fixed_units == []
    assert spec.scaled_units == []
    assert spec.opex == []


def _cost_model(tmp_path: Path, **system_overrides: float):
    spec = {
        "system": {
            "name": "test",
            "lifetime_in_years": 1,
            "uptime": 1,
            "magical_speedup_factor": 1,
            **system_overrides,
        },
        "fixed_units": [],
        "scaled_units": [{"name": "qubits", "cost": 31_536, "units_per_qubit": 1}],
        "opex": [],
    }
    path = tmp_path / "cost_spec.json"
    path.write_text(json.dumps(spec), encoding="utf-8")
    return DollarCostModelFromSpec(str(path))


def test_cost_uses_node_rounding_and_speedup(tmp_path: Path):
    model = _cost_model(tmp_path, qubits_per_node=10, magical_speedup_factor=2)
    assert model.cost_usd(qubits=11, runtime_nanos=1_000_000_000_000) == 10.0


def test_cost_returns_none_when_application_exceeds_machine(tmp_path: Path):
    model = _cost_model(tmp_path, physical_qubits=10)
    assert model.cost_usd(qubits=11, runtime_nanos=1) is None


@pytest.mark.parametrize(
    "field,value",
    [
        ("lifetime_in_years", 0),
        ("uptime", 0),
        ("uptime", 1.1),
        ("magical_speedup_factor", 0),
        ("qubits_per_node", 0),
    ],
)
def test_invalid_system_values(tmp_path: Path, field: str, value: float):
    with pytest.raises(ValueError, match=field):
        _cost_model(tmp_path, **{field: value})
