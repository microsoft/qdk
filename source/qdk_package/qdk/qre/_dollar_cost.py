import math

from ._json_specs import JsonSpec
from ._results import EstimationTableEntry


def compute_dollar_cost(cost_spec_path: str, result: EstimationTableEntry) -> float:
    """Computes cost in dollars."""
    spec = JsonSpec.from_file(cost_spec_path)
    system = spec.system

    # Build the node cost
    qubits_per_node = system.qubits_per_node
    control_lines = (
        system.fixed_control_lines
        + qubits_per_node * system.control_lines_per_physical_qubit
    )
    readout_lines = (
        system.fixed_readout_lines
        + qubits_per_node * system.readout_lines_per_physical_qubit
    )
    cost_per_control_line = 0
    for unit in spec.scaled_units:
        cost_per_control_line += (
            unit.cost * unit.units_per_control_line * unit.eos_factor
        )
    cost_per_readout_line = 0
    for unit in spec.scaled_units:
        cost_per_readout_line += (
            unit.cost * unit.units_per_readout_line * unit.eos_factor
        )

    capex = 0
    for unit in spec.fixed_units:
        capex += unit.cost * unit.quantity * unit.eos_factor
    capex += cost_per_control_line * control_lines
    capex += cost_per_readout_line * readout_lines

    opex = sum(item.cost for item in spec.opex)

    number_of_nodes = math.ceil(result.qubits / qubits_per_node) + 1
    runtime_hours = result.runtime / (3600 * 1_000_000_000)  # Convert to hours
    runtime_hours /= system.magical_speedup_factor
    operating_lifetime = system.lifetime_in_years
    up_hours_per_year = 8760 * system.uptime
    hourly_cost = (opex + capex / operating_lifetime) / up_hours_per_year
    dollar_cost = number_of_nodes * hourly_cost * runtime_hours
    return math.ceil(dollar_cost * 100) / 100
