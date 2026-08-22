"""``qdk.ec.qodec_from_code`` — synthesizing a qodec from a code.

The suite is organised around what synthesis promises: a *structurally* valid
qodec whose gadgets are *semantically* verified and that *round-trips*.
"""

from __future__ import annotations

from pathlib import Path

import pytest
import qodec as qc

from ec_tests.testing import code_catalog as catalog
from ec_tests.testing.qodecs import c4
import qdk.ec as ec
from qdk.ec import action, distance, lint
from qdk.ec import qodec_from_code, synthesis_notes
from qdk.ec._analysis.code_algebra import as_qodec_code

#: Codes for which every instruction is expected to synthesize. Each entry is
#: (label, factory, physical qubits, logical qubits).
FULLY_SUPPORTED = [
    ("repetition3", lambda: catalog.make_repetition_code(3), 3, 1),
    ("steane", catalog.make_steane_code, 7, 1),
    ("shor", catalog.make_shor_code, 9, 1),
    (
        "surface3",
        lambda: catalog.make_rotated_surface_code(x_distance=3, z_distance=3),
        9,
        1,
    ),
]


def _code(label: str, factory) -> qc.Code:
    return as_qodec_code(factory(), label)


@pytest.fixture(scope="module")
def steane() -> qc.Qodec:
    return qodec_from_code(_code("steane", catalog.make_steane_code))


# ── Structure ───────────────────────────────────────────────────────────────


def test_result_is_a_two_layer_qodec(steane: qc.Qodec) -> None:
    assert len(steane.layers) == 2
    assert steane.layers[0].isa.name == "steane"
    assert steane.layers[1].isa.name == "stim"
    assert steane.layers[1].gadgets == {}


def test_logical_block_encodes_the_logical_qubits(steane: qc.Qodec) -> None:
    (block,) = steane.layers[0].isa.blocks

    assert block.name == "steane"
    assert block.encodes == 1


def test_every_declared_instruction_has_a_gadget(steane: qc.Qodec) -> None:
    layer = steane.layers[0]

    assert set(layer.isa.instructions) == set(layer.gadgets)


def test_the_expected_instruction_menu_is_synthesized(steane: qc.Qodec) -> None:
    assert set(steane.layers[0].gadgets) == {
        "prepare_z",
        "prepare_x",
        "idle",
        "measure_z",
        "measure_x",
        "x0",
        "z0",
    }


def test_the_code_is_carried_through(steane: qc.Qodec) -> None:
    assert "steane" in steane.codes
    assert list(steane.codes["steane"].stabilizers)


def test_name_and_description_default_from_the_code() -> None:
    built = qodec_from_code(_code("steane", catalog.make_steane_code))

    assert built.name == "steane"
    assert "[[7, 1]]" in built.description


def test_name_and_description_can_be_overridden() -> None:
    built = qodec_from_code(
        _code("steane", catalog.make_steane_code),
        name="my_qodec",
        description="hand written",
    )

    assert built.name == "my_qodec"
    assert built.description == "hand written"
    assert built.layers[0].isa.name == "my_qodec"


@pytest.mark.parametrize(
    ("label", "factory", "physical", "logical"),
    FULLY_SUPPORTED,
    ids=[case[0] for case in FULLY_SUPPORTED],
)
def test_synthesis_notes_record_the_code_shape(
    label: str, factory, physical: int, logical: int
) -> None:
    notes = synthesis_notes(qodec_from_code(_code(label, factory)))

    assert notes["code"] == label
    assert notes["physical_qubits"] == physical
    assert notes["logical_qubits"] == logical
    assert notes["omitted"] == {}


def test_synthesis_notes_are_empty_for_a_hand_authored_qodec() -> None:
    assert synthesis_notes(c4()) == {}


# ── Circuits ────────────────────────────────────────────────────────────────


def test_syndrome_round_allocates_a_syndrome_ancilla_and_a_flag_per_stabilizer(
    steane: qc.Qodec,
) -> None:
    code = steane.codes["steane"]
    stabilizers = len(list(code.stabilizers))
    source = steane.layers[0].gadgets["idle"].circuit.source

    measured = [
        int(target)
        for line in source.splitlines()
        if line.startswith("M ")
        for target in line.split()[1:]
    ]
    # Every Steane stabilizer has weight 4, so each carries exactly one flag.
    assert len(measured) == 2 * stabilizers
    syndromes, flag_qubits = measured[:stabilizers], measured[stabilizers:]
    # Syndrome ancillas are measured first, in stabilizer order, so the record
    # index of stabilizer i is i regardless of which stabilizers carry flags.
    assert syndromes == sorted(syndromes)
    assert set(syndromes).isdisjoint(flag_qubits)
    assert min(measured) >= 7, "ancillas must not collide with the 7 data qubits"


def test_syndrome_records_are_ordered_stabilizers_then_flags(
    steane: qc.Qodec,
) -> None:
    """The record layout must not depend on which stabilizers carry flags."""
    source = steane.layers[0].gadgets["idle"].circuit.source
    measurement_lines = [line for line in source.splitlines() if line.startswith("M ")]

    assert len(measurement_lines) == 2, "expected one M for syndromes, one for flags"


def test_flag_outcomes_are_discovered_as_deterministic_checks(
    steane: qc.Qodec,
) -> None:
    """A flag bit is deterministic, so completion must find it as a check.

    That is what turns a flagged hook error into a detector the decoder sees.
    """
    idle = steane.layers[0].gadgets["idle"]

    flag_checks = [
        check
        for check in idle.checks
        if len(check) == 1 and str(check[0]).startswith("circuit.readouts")
    ]
    assert len(flag_checks) == 6, "one flag check per weight-4 stabilizer"


def test_a_weight_two_stabilizer_carries_no_flag() -> None:
    """Flag brackets must stay nested, which a weight-2 stabilizer cannot host."""
    from qdk.ec._synthesis import _flag_capacity

    assert _flag_capacity(2) == 0
    assert _flag_capacity(3) == 1
    assert _flag_capacity(4) == 1
    assert _flag_capacity(6) == 2


def test_flag_count_defaults_to_the_codes_error_correcting_radius() -> None:
    """Chamberland-Beverland call for t = (d-1)//2 flags for a distance-d code."""
    steane_code = _code("steane", catalog.make_steane_code)

    notes = synthesis_notes(qodec_from_code(steane_code))

    assert notes["flags_per_stabilizer"] == 1


def test_flags_can_be_disabled_for_the_naive_circuit() -> None:
    code = _code("steane", catalog.make_steane_code)

    built = qodec_from_code(code, flags=0)

    source = built.layers[0].gadgets["idle"].circuit.source
    assert synthesis_notes(built)["flags_per_stabilizer"] == 0
    # 7 data qubits + one ancilla per stabilizer, and nothing else.
    assert max(int(t) for line in source.splitlines() for t in line.split()[1:]) == 12


def test_negative_flag_counts_are_rejected() -> None:
    with pytest.raises(ValueError, match="non-negative"):
        qodec_from_code(_code("steane", catalog.make_steane_code), flags=-1)


def test_syndrome_round_never_touches_data_qubits_with_single_qubit_gates(
    steane: qc.Qodec,
) -> None:
    source = steane.layers[0].gadgets["idle"].circuit.source

    for line in source.splitlines():
        gate, *targets = line.split()
        if gate in ("R", "H", "M"):
            assert all(int(target) >= 7 for target in targets), line


def test_measure_gadgets_are_transversal(steane: qc.Qodec) -> None:
    gadgets = steane.layers[0].gadgets

    assert gadgets["measure_z"].circuit.source == "M 0 1 2 3 4 5 6\n"
    assert gadgets["measure_x"].circuit.source == "H 0 1 2 3 4 5 6\nM 0 1 2 3 4 5 6\n"


def test_logical_pauli_gadget_applies_the_codes_operator(steane: qc.Qodec) -> None:
    code = steane.codes["steane"]
    x_operator = str(list(code.x)[0])
    expected = sorted(
        int(token.split("_")[1])
        for token in x_operator.split()
        if token.startswith("X")
    )

    source = steane.layers[0].gadgets["x0"].circuit.source

    assert sorted(int(t) for t in source.split()[1:]) == expected


def test_circuits_are_tagged_as_stim(steane: qc.Qodec) -> None:
    assert all(
        gadget.circuit.format == "stim" for gadget in steane.layers[0].gadgets.values()
    )


# ── Semantics ───────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("label", "factory"),
    [(case[0], case[1]) for case in FULLY_SUPPORTED],
    ids=[case[0] for case in FULLY_SUPPORTED],
)
def test_every_gadget_realizes_the_action_it_declares(label: str, factory) -> None:
    built = qodec_from_code(_code(label, factory))

    mismatched = {
        mnemonic: action.gadget_action_mismatch(gadget)
        for mnemonic, gadget in built.layers[0].gadgets.items()
        if action.gadget_action_mismatch(gadget) is not None
    }
    assert mismatched == {}


@pytest.mark.parametrize(
    ("label", "factory"),
    [(case[0], case[1]) for case in FULLY_SUPPORTED],
    ids=[case[0] for case in FULLY_SUPPORTED],
)
def test_gadgets_that_hold_state_discover_checks(label: str, factory) -> None:
    built = qodec_from_code(_code(label, factory))

    for mnemonic in ("prepare_z", "prepare_x", "idle"):
        gadget = built.layers[0].gadgets[mnemonic]
        assert gadget.checks, f"{mnemonic} discovered no checks"


def test_measure_gadgets_bind_a_readout_per_logical_qubit(steane: qc.Qodec) -> None:
    for mnemonic in ("measure_z", "measure_x"):
        gadget = steane.layers[0].gadgets[mnemonic]
        assert len(gadget.readouts) == 1, mnemonic


def test_idle_checks_reference_both_boundaries(steane: qc.Qodec) -> None:
    atoms = {
        str(atom) for check in steane.layers[0].gadgets["idle"].checks for atom in check
    }

    assert any(atom.startswith("in[0].stabilizers") for atom in atoms)
    assert any(atom.startswith("out[0].stabilizers") for atom in atoms)


def test_synthesized_code_keeps_its_distance() -> None:
    built = qodec_from_code(_code("steane", catalog.make_steane_code))

    code_distance, _ = distance.code_distance_of(built.codes["steane"])

    assert code_distance == 3


# ── Audit ───────────────────────────────────────────────────────────────────

#: Rule that misfires on X-basis destructive measurement gadgets. It fires on
#: the hand-authored c4 fixture's `measure_xx` too, so it is a property of the
#: audit rule rather than of synthesis. Asserted as a known exception here so
#: this suite tightens automatically once the rule is fixed.
_KNOWN_AUDIT_RULE = "gadget/readout-mismatch"


@pytest.mark.parametrize(
    ("label", "factory"),
    [(case[0], case[1]) for case in FULLY_SUPPORTED],
    ids=[case[0] for case in FULLY_SUPPORTED],
)
def test_audit_reports_no_unexpected_errors(label: str, factory) -> None:
    built = qodec_from_code(_code(label, factory))

    unexpected = [
        f"{d.rule}: {d.summary}"
        for d in lint.diagnose(built).errors()
        if d.rule != _KNOWN_AUDIT_RULE
    ]
    assert unexpected == []


def test_the_known_audit_rule_also_fires_on_the_hand_authored_fixture() -> None:
    """Pins the claim that ``_KNOWN_AUDIT_RULE`` is not a synthesis defect."""
    fixture = c4()

    rules = {
        d.rule
        for gadget in fixture.layers[0].gadgets.values()
        for d in lint.Auditor().audit_gadget(gadget, qodec=fixture).errors()
    }
    assert _KNOWN_AUDIT_RULE in rules


# ── Round-tripping ──────────────────────────────────────────────────────────


def test_synthesized_qodec_round_trips_through_yaml(steane: qc.Qodec) -> None:
    restored = ec.from_yaml(ec.to_yaml(steane))

    assert restored.name == steane.name
    assert sorted(restored.layers[0].gadgets) == sorted(steane.layers[0].gadgets)


def test_structured_omissions_round_trip_through_yaml() -> None:
    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    restored = ec.from_yaml(ec.to_yaml(built))

    assert synthesis_notes(restored)["omitted"] == synthesis_notes(built)["omitted"]


def test_synthesized_qodec_round_trips_through_disk(
    steane: qc.Qodec, tmp_path: Path
) -> None:
    ec.save_yaml(steane, tmp_path / "bundle")
    restored = ec.load_yaml(tmp_path / "bundle")

    assert restored.name == steane.name
    assert sorted(restored.codes) == sorted(steane.codes)


def test_completion_is_idempotent_on_a_synthesized_qodec(
    steane: qc.Qodec,
) -> None:
    recompleted = ec.complete_qodec(steane)

    for mnemonic, gadget in steane.layers[0].gadgets.items():
        before = {frozenset(str(a) for a in c) for c in gadget.checks}
        after = {
            frozenset(str(a) for a in c)
            for c in recompleted.layers[0].gadgets[mnemonic].checks
        }
        assert before == after, mnemonic


# ── Partial synthesis ───────────────────────────────────────────────────────


def test_a_non_z_logical_basis_omits_the_gadgets_it_cannot_support() -> None:
    """The five-qubit code's conventional basis has X components in logical Z."""
    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    omitted = synthesis_notes(built)["omitted"]
    assert "prepare_z" in omitted
    assert "measure_z" in omitted
    assert "idle" in built.layers[0].gadgets
    assert set(built.layers[0].isa.instructions) == set(built.layers[0].gadgets)


def test_omissions_carry_structured_reasons() -> None:
    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    assert all(
        isinstance(reason, dict)
        and set(reason) == {"stage", "kind", "message"}
        and reason["stage"] in {"completion", "verification"}
        and isinstance(reason["kind"], str)
        and reason["kind"]
        and isinstance(reason["message"], str)
        and reason["message"]
        for reason in synthesis_notes(built)["omitted"].values()
    )


def test_unexpected_completion_failure_propagates(monkeypatch) -> None:
    from qdk.ec import _synthesis

    original = _synthesis.complete_gadget

    def complete_or_fail(gadget: qc.Gadget) -> qc.Gadget:
        if gadget.implements.mnemonic == "idle":
            raise RuntimeError("unexpected completion failure")
        return original(gadget)

    monkeypatch.setattr(_synthesis, "complete_gadget", complete_or_fail)

    with pytest.raises(RuntimeError, match="unexpected completion failure"):
        qodec_from_code(_code("steane", catalog.make_steane_code))


def test_unexpected_verification_failure_propagates(monkeypatch) -> None:
    from qdk.ec import _synthesis

    def fail_verification(gadget: qc.Gadget) -> str | None:
        raise RuntimeError("unexpected verification failure")

    monkeypatch.setattr(_synthesis, "gadget_action_mismatch", fail_verification)

    with pytest.raises(RuntimeError, match="unexpected verification failure"):
        qodec_from_code(_code("steane", catalog.make_steane_code))


def test_strict_mode_raises_instead_of_omitting() -> None:
    code = _code("five_qubit", catalog.make_five_qubit_code)

    with pytest.raises(ValueError, match="could not synthesize"):
        qodec_from_code(code, strict=True)


def test_strict_mode_is_a_no_op_when_everything_synthesizes() -> None:
    code = _code("steane", catalog.make_steane_code)

    assert set(qodec_from_code(code, strict=True).layers[0].gadgets) == set(
        qodec_from_code(code).layers[0].gadgets
    )


def test_logical_basis_choice_can_decide_whether_readout_synthesizes() -> None:
    """Two valid logical bases for [[4,2,2]] behave differently.

    This pins an observed basis-dependence in the observable-discovery pass
    completion relies on, so the difference is visible rather than silent.
    """
    fixture_basis = qodec_from_code(c4().codes["C4"], name="c4_fixture_basis")
    catalog_basis = qodec_from_code(_code("c422", catalog.make_422_code))

    assert synthesis_notes(fixture_basis)["omitted"] == {}
    assert "measure_z" in synthesis_notes(catalog_basis)["omitted"]


# ── Multi-logical-qubit codes ───────────────────────────────────────────────


def test_a_k_equals_two_code_gets_one_pauli_gadget_per_logical_qubit() -> None:
    built = qodec_from_code(c4().codes["C4"], name="c4_synth")

    assert {"x0", "x1", "z0", "z1"} <= set(built.layers[0].gadgets)


def test_logical_pauli_gadgets_are_verified_for_a_large_k_code() -> None:
    """Logical coordinates remain authored-order even when k is large."""
    built = qodec_from_code(_code("iceberg8", lambda: catalog.make_iceberg_code(8)))

    pauli_gadgets = {
        mnemonic: gadget
        for mnemonic, gadget in built.layers[0].gadgets.items()
        if mnemonic[0] in "xz" and mnemonic[1:].isdigit()
    }
    assert len(pauli_gadgets) == 12
    assert all(
        action.gadget_action_mismatch(gadget) is None
        for gadget in pauli_gadgets.values()
    )


# ── Rejected inputs ─────────────────────────────────────────────────────────


def test_y_components_are_rejected_with_an_actionable_message() -> None:
    code = qc.Code("has_y", stabilizers=["Y_0 X_1"], x=["X_0"], z=["Z_0 Z_1"])

    with pytest.raises(NotImplementedError, match="Y components"):
        qodec_from_code(code)


def test_a_code_with_no_logical_qubits_is_rejected() -> None:
    """A [[1, 0]] code: a valid stabilizer code that encodes nothing."""
    code = qc.Code("full_rank", stabilizers=["Z_0"], x=[], z=[])

    with pytest.raises(ValueError, match="no logical qubits"):
        qodec_from_code(code)


def test_an_unnamed_code_requires_an_explicit_name() -> None:
    code = qc.Code("", stabilizers=["Z_0 Z_1"], x=["X_0 X_1"], z=["Z_0"])

    with pytest.raises(ValueError, match="no name"):
        qodec_from_code(code)


# ── Memory programs ─────────────────────────────────────────────────────────


def test_memory_program_reports_missing_instructions() -> None:
    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    with pytest.raises(ValueError, match="missing"):
        ec.memory_program(built)


def test_memory_program_has_the_expected_shape(steane: qc.Qodec) -> None:
    program = ec.memory_program(steane, rounds=3)

    assert [call.mnemonic for call in program.instructions] == [
        "prepare_z",
        "idle",
        "idle",
        "idle",
        "measure_z",
    ]
