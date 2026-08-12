"""The ``qdk.ec`` public API surface.

This pins the shape agreed for the package so a refactor cannot silently drop
or rename a documented entry point.

The bracketed headings in the spec (``[develop]``, ``[profile]``,
``[test / audit]``) are conceptual groupings, not modules — so this file also
asserts they are *not* importable, which is what keeps the flat shape honest.
"""

from __future__ import annotations

import importlib
import subprocess
import sys

import pytest

import qdk.ec

#: module -> the attributes that module must export via ``__all__``.
_SURFACE: dict[str, tuple[str, ...]] = {
    # develop: primitives and smart tooling, flat on the package root
    "qdk.ec": (
        "complete_gadget",
        "complete_qodec",
        "from_yaml",
        "load",
        "qodec_from_code",
        "save",
        "to_yaml",
    ),
    # profile
    "qdk.ec.action": (
        "action_of",
        "declared_action_of",
        "gadget_action_mismatch",
        "input_qubits_of",
        "realized_action_of",
    ),
    "qdk.ec.checks": (
        "checks_of",
        "essential_checks_of",
        "outcome_code_of",
    ),
    "qdk.ec.code": (
        "encoding_clifford_of",
        "gauge_basis_of",
        "logical_effect_of",
        "syndrome_of",
    ),
    "qdk.ec.distance": (
        "code_distance_bounds_of",
        "code_distance_of",
    ),
    "qdk.ec.faults": (
        "fault_effects_of",
        "fault_profile_of",
    ),
    "qdk.ec.readouts": (
        "outcome_profile_of",
        "outcomes_flipped_by_anti_observables_of",
        "profile_of",
    ),
    # test / audit
    "qdk.ec.equivalence": (
        "actions_equivalent_mod_pauli",
        "actions_outcome_equivalent",
        "codes_equivalent",
        "gadgets_equivalent",
        "why_not_equivalent",
    ),
    "qdk.ec.lint": ("Report", "Severity", "diagnose", "why_not_valid"),
    # targets / deploy
    "qdk.ec.targets": (
        "Sampler",
        "Target",
        "TargetModel",
        "circuit_distance_of",
        "encodable_gates_of",
        "encode_qir",
        "run_qir_encoded",
    ),
}

#: Submodules the package root must expose.
_SUBMODULES = (
    "action",
    "checks",
    "code",
    "distance",
    "equivalence",
    "faults",
    "lint",
    "readouts",
    "targets",
)

#: The spec's bracketed headings are conceptual; these must not be modules.
_CONCEPTUAL = ("develop", "profile", "audit")


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


@pytest.mark.parametrize("name", _SUBMODULES)
def test_documented_submodule_is_reachable(name: str) -> None:
    assert name in qdk.ec.__all__
    assert importlib.import_module(f"qdk.ec.{name}") is getattr(qdk.ec, name)


@pytest.mark.parametrize("name", _CONCEPTUAL)
def test_conceptual_headings_are_not_modules(name: str) -> None:
    """``[develop]``, ``[profile]`` and ``[test / audit]`` group the API in the
    spec; they must not reappear as importable packages."""
    assert name not in qdk.ec.__all__
    assert not hasattr(qdk.ec, name)
    with pytest.raises(ModuleNotFoundError):
        importlib.import_module(f"qdk.ec.{name}")


def test_importing_qdk_ec_does_not_import_the_submodules() -> None:
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


def test_equivalence_predicates_are_the_underlying_functions() -> None:
    """The public names are aliases, not reimplementations."""
    from qdk.ec import code, equivalence
    from qdk.ec._analysis import circuit_action
    from qdk.ec._analysis import equivalence as _equivalence

    assert equivalence.actions_equivalent_mod_pauli is (
        circuit_action.are_equivalent_mod_paulis
    )
    assert equivalence.actions_outcome_equivalent is (
        circuit_action.are_outcome_equivalent
    )
    assert equivalence.codes_equivalent is code.codes_equivalent
    assert equivalence.gadgets_equivalent is _equivalence.gadgets_equivalent
    assert equivalence.why_not_equivalent is _equivalence.why_not_equivalent


def test_analysis_internals_stay_private() -> None:
    """The engines behind the profiling modules are not public API."""
    assert "_analysis" not in qdk.ec.__all__
