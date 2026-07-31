"""Compute focused, typed characteristics of qodec objects.

Everything here is a *profile*: a pure, deterministic read of a qodec object
that answers one question about it. The submodules group those questions:

* :mod:`~qdk.ec.profile.action` — what a gadget declares it does, and what its
  circuit actually does.
* :mod:`~qdk.ec.profile.checks` — the deterministic parity structure among a
  gadget's measurement outcomes.
* :mod:`~qdk.ec.profile.code` — characteristics of :class:`qodec.Code` objects.
* :mod:`~qdk.ec.profile.distance` — code distance, exactly or in bounds.
* :mod:`~qdk.ec.profile.faults` — how a basis of faults propagates to the
  gadget boundary.
* :mod:`~qdk.ec.profile.readouts` — what a gadget's measurement outcomes mean.

Some of these — checks and readouts in particular — are *completions* of a
gadget and can be written back into a qodec (see :mod:`qdk.ec.develop`); others,
such as faults and actions, are information that would not go back into a qodec.
"""

from . import action, checks, code, distance, faults, readouts
from .action import (
    CircuitAction,
    LogicalAction,
    LogicalImage,
    ObjectiveLift,
    action_of,
    actions_equivalent_mod_pauli,
    actions_outcome_equivalent,
    are_equivalent_mod_paulis,
    are_outcome_equivalent,
    declared_action_of,
    gadget_action_mismatch,
    gadget_objective_action_of,
    gadget_realization_action_of,
    gadgets_equivalent,
    input_qubits_of,
    lift_objective,
    logical_action_of,
    realized_action_of,
    why_not_equivalent,
)
from .checks import (
    OutcomeCode,
    OutcomeProfile,
    Profile,
    checks_of,
    essential_checks_of,
    outcome_code_of,
    outcome_profile_of,
    outcomes_flipped_by_anti_observables_of,
    profile_of,
    readouts_of,
)
from .code import (
    codes_equivalent,
    encoding_clifford_of,
    gauge_basis_of,
    logical_effect_of,
    syndrome_of,
)
from .distance import (
    CodeDistanceData,
    ExhaustiveSolverOptions,
    MwpfSolverOptions,
    code_distance_bounds_of,
    code_distance_of,
)
from .faults import (
    Fault,
    FaultEffect,
    FaultProfile,
    fault_effects_of,
    fault_profile_of,
)

__all__ = [
    "CircuitAction",
    "CodeDistanceData",
    "ExhaustiveSolverOptions",
    "Fault",
    "FaultEffect",
    "FaultProfile",
    "LogicalAction",
    "LogicalImage",
    "MwpfSolverOptions",
    "ObjectiveLift",
    "OutcomeCode",
    "OutcomeProfile",
    "Profile",
    "action",
    "action_of",
    "actions_equivalent_mod_pauli",
    "actions_outcome_equivalent",
    "are_equivalent_mod_paulis",
    "are_outcome_equivalent",
    "checks",
    "checks_of",
    "code",
    "code_distance_bounds_of",
    "code_distance_of",
    "codes_equivalent",
    "declared_action_of",
    "distance",
    "encoding_clifford_of",
    "essential_checks_of",
    "fault_effects_of",
    "fault_profile_of",
    "faults",
    "gadget_action_mismatch",
    "gadget_objective_action_of",
    "gadget_realization_action_of",
    "gadgets_equivalent",
    "gauge_basis_of",
    "input_qubits_of",
    "lift_objective",
    "logical_action_of",
    "logical_effect_of",
    "outcome_code_of",
    "outcome_profile_of",
    "outcomes_flipped_by_anti_observables_of",
    "profile_of",
    "readouts",
    "readouts_of",
    "realized_action_of",
    "syndrome_of",
    "why_not_equivalent",
]
