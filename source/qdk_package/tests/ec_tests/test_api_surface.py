"""The ``qdk.ec`` public API surface.

This pins the shape agreed for the package so a refactor cannot silently drop
or rename a documented entry point.
"""

from __future__ import annotations

import importlib
import subprocess
import sys

import pytest

import qdk.ec

_SURFACE: dict[str, tuple[str, ...]] = {
    "qdk.ec": ("audit", "develop", "profile", "targets"),
    "qdk.ec.develop": (
        "complete_gadget",
        "complete_qodec",
        "from_yaml",
        "load",
        "save",
        "to_yaml",
    ),
    "qdk.ec.profile.action": (
        "action_of",
        "declared_action_of",
        "gadget_action_mismatch",
        "input_qubits_of",
        "realized_action_of",
    ),
    "qdk.ec.profile.checks": (
        "checks_of",
        "essential_checks_of",
        "outcome_code_of",
    ),
    "qdk.ec.profile.code": (
        "encoding_clifford_of",
        "gauge_basis_of",
        "logical_effect_of",
        "syndrome_of",
    ),
    "qdk.ec.profile.distance": (
        "code_distance_bounds_of",
        "code_distance_of",
    ),
    "qdk.ec.profile.faults": (
        "fault_effects_of",
        "fault_profile_of",
    ),
    "qdk.ec.profile.readouts": (
        "outcome_profile_of",
        "outcomes_flipped_by_anti_observables_of",
        "profile_of",
    ),
    "qdk.ec.audit.equivalence": (
        "actions_equivalent_mod_pauli",
        "actions_outcome_equivalent",
        "codes_equivalent",
        "gadgets_equivalent",
        "why_not_equivalent",
    ),
    "qdk.ec.audit": (
        "Report",
        "Severity",
        "audit",
        "checks",
        "readouts",
        "why_not_valid",
    ),
    "qdk.ec.targets": ("Sampler", "Target", "TargetModel"),
}


@pytest.mark.parametrize(
    ("module_name", "attribute"),
    [
        (module_name, attribute)
        for module_name, attributes in _SURFACE.items()
        for attribute in attributes
    ],
)
def test_documented_attribute_is_reachable(module_name: str, attribute: str) -> None:
    module = importlib.import_module(module_name)

    assert hasattr(module, attribute), f"{module_name}.{attribute} is missing"
    assert attribute in getattr(module, "__all__", ()), (
        f"{module_name}.{attribute} is not exported via __all__"
    )


def test_importing_qdk_ec_does_not_import_the_subpackages() -> None:
    # Run in a fresh interpreter: purging ``sys.modules`` in-process would give
    # the rest of the suite duplicate module objects.
    script = (
        "import sys, qdk.ec;"
        "assert 'qdk.ec.targets' not in sys.modules, 'targets imported eagerly';"
        "assert qdk.ec.targets is not None;"
        "assert 'qdk.ec.targets' in sys.modules"
    )

    result = subprocess.run(
        [sys.executable, "-c", script], capture_output=True, text=True, check=False
    )

    assert result.returncode == 0, result.stderr


def test_unknown_attribute_raises_attribute_error() -> None:
    with pytest.raises(AttributeError):
        qdk.ec.not_a_subpackage  # noqa: B018


def test_equivalence_aliases_are_the_profile_functions() -> None:
    from qdk.ec import audit
    from qdk.ec.profile import circuit_action, code, equivalence

    assert audit.actions_equivalent_mod_pauli is circuit_action.are_equivalent_mod_paulis
    assert audit.actions_outcome_equivalent is circuit_action.are_outcome_equivalent
    assert audit.codes_equivalent is code.codes_equivalent
    assert audit.gadgets_equivalent is equivalence.gadgets_equivalent
    assert audit.why_not_equivalent is equivalence.why_not_equivalent
