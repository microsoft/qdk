"""Declared and realized action characteristics for qodec gadgets."""

from .circuit_action import (
    CircuitAction,
    action_of,
    are_equivalent_mod_paulis,
    are_outcome_equivalent,
    gadget_action_mismatch,
    gadget_objective_action_of,
    gadget_realization_action_of,
    input_qubits_of,
)
from .equivalence import (
    LogicalAction,
    LogicalImage,
    gadgets_equivalent,
    logical_action_of,
    why_not_equivalent,
)
from .objective import ObjectiveLift, lift_objective

# Names that state which side of the gadget contract is being profiled.
declared_action_of = gadget_objective_action_of
realized_action_of = gadget_realization_action_of

# Names that read as a predicate over two actions.
actions_equivalent_mod_pauli = are_equivalent_mod_paulis
actions_outcome_equivalent = are_outcome_equivalent

__all__ = [
    "CircuitAction",
    "LogicalAction",
    "LogicalImage",
    "ObjectiveLift",
    "action_of",
    "actions_equivalent_mod_pauli",
    "actions_outcome_equivalent",
    "are_equivalent_mod_paulis",
    "are_outcome_equivalent",
    "declared_action_of",
    "gadget_action_mismatch",
    "gadget_objective_action_of",
    "gadget_realization_action_of",
    "gadgets_equivalent",
    "input_qubits_of",
    "lift_objective",
    "logical_action_of",
    "realized_action_of",
    "why_not_equivalent",
]
