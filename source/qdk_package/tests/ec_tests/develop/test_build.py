"""``qdk.ec.build_qodec`` — building a qodec from a code.

The suite is organised around what the builder promises: a *structurally* valid
qodec whose gadgets are *semantically* verified and that *round-trips*.
"""

from __future__ import annotations

from pathlib import Path

import pytest
import qodec as qc

from ec_tests.testing import code_catalog as catalog
from ec_tests.testing.qodecs import c4
from qdk.ec import CodeProfile, _audit, build_qodec
from qdk.ec import _distance as distance
from qdk.ec._analysis import channel_action as action
from qdk.ec._fill import complete_qodec
from qdk.ec._build import _METADATA_KEY, _build as qodec_from_code
from qdk.ec._analysis.code_algebra import as_qodec_code

#: CSS codes with and without a valid transversal H candidate. Each entry is
#: (label, factory, physical qubits, logical qubits).
CSS_CODES = [
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


def build_notes(qodec: qc.Qodec) -> dict:
    """The build record left in a built qodec's metadata."""
    section = dict(qodec.metadata).get(_METADATA_KEY) or {}
    return dict(section.get("build", {}))


def _round_tripped(qodec: qc.Qodec, directory: Path) -> qc.Qodec:
    # qodec.save/load take strings, not os.PathLike.
    qodec.save(str(directory), single_file=True)
    return qc.Qodec.load(str(directory / qodec.manifest_filename))


@pytest.fixture(scope="module")
def steane() -> qc.Qodec:
    return qodec_from_code(_code("steane", catalog.make_steane_code))


# ── Structure ───────────────────────────────────────────────────────────────


def test_result_is_a_two_layer_qodec(steane: qc.Qodec) -> None:
    assert len(steane.layers) == 2
    assert steane.layers[0].instruction_set.name == "steane"
    assert steane.layers[1].instruction_set.name == "stim"
    assert steane.layers[1].gadgets == {}


def test_logical_block_encodes_the_logical_qubits(steane: qc.Qodec) -> None:
    (block,) = steane.layers[0].instruction_set.blocks

    assert block.name == "steane"
    assert block.encodes == 1


def test_every_declared_instruction_has_a_gadget(steane: qc.Qodec) -> None:
    layer = steane.layers[0]

    assert set(layer.instruction_set.instructions) == set(layer.gadgets)


@pytest.mark.parametrize("strategy", ["bare-css/v1", "flagged-css/v1"])
def test_the_expected_instruction_menu_is_built(strategy: str) -> None:
    built = build_qodec(_code("steane", catalog.make_steane_code), strategy=strategy)
    expected = {
        "prepare_z_all",
        "prepare_x_all",
        "syndrome",
        "measure_z_all",
        "measure_x_all",
        "h_all",
        "cx_all",
    }
    assert len(expected) == 7
    assert set(built.layers[0].gadgets) == expected
    assert set(built.layers[0].instruction_set.instructions) == expected


def test_the_code_is_carried_through(steane: qc.Qodec) -> None:
    assert "steane" in steane.codes
    assert list(steane.codes["steane"].stabilizers)


@pytest.mark.parametrize("strategy", ["bare-css/v1", "flagged-css/v1"])
@pytest.mark.parametrize("code_name", ["C4", "steane"])
def test_built_boundary_types_are_explicit(strategy: str, code_name: str) -> None:
    code = (
        c4().codes["C4"]
        if code_name == "C4"
        else _code("steane", catalog.make_steane_code)
    )
    built = build_qodec(code, strategy=strategy, strict=False)

    for gadget in built.layers[0].gadgets.values():
        for encoding in (*gadget.inputs, *gadget.outputs):
            assert encoding.block_types == ["qubit"] * len(encoding.support)
    assert qc.Qodec.loads(built.dumps()) == built


def test_name_and_description_default_from_the_code() -> None:
    built = qodec_from_code(_code("steane", catalog.make_steane_code))

    assert built.name == "steane"
    assert built.description.startswith("Built from ")
    assert "[[7, 1]]" in built.description


def test_name_and_description_can_be_overridden() -> None:
    built = qodec_from_code(
        _code("steane", catalog.make_steane_code),
        name="my_qodec",
        description="hand written",
    )

    assert built.name == "my_qodec"
    assert built.description == "hand written"
    assert built.layers[0].instruction_set.name == "my_qodec"


@pytest.mark.parametrize(
    ("label", "factory", "physical", "logical"),
    CSS_CODES,
    ids=[case[0] for case in CSS_CODES],
)
def test_build_notes_record_the_code_shape(
    label: str, factory, physical: int, logical: int
) -> None:
    built = qodec_from_code(_code(label, factory))
    assert set(built.metadata[_METADATA_KEY]) == {"build"}
    notes = build_notes(built)

    assert notes["source"] == "qdk.ec.build_qodec"
    assert notes["code"] == label
    assert notes["physical_qubits"] == physical
    assert notes["logical_qubits"] == logical
    assert set(notes["omitted"]) == (set() if label == "steane" else {"h_all"})


def test_build_notes_are_empty_for_a_hand_authored_qodec() -> None:
    assert build_notes(c4()) == {}


# ── Circuits ────────────────────────────────────────────────────────────────


def test_syndrome_round_allocates_a_syndrome_ancilla_and_a_flag_per_stabilizer(
    steane: qc.Qodec,
) -> None:
    code = steane.codes["steane"]
    stabilizers = len(list(code.stabilizers))
    source = steane.layers[0].gadgets["syndrome"].circuit.source

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
    source = steane.layers[0].gadgets["syndrome"].circuit.source
    measurement_lines = [line for line in source.splitlines() if line.startswith("M ")]

    assert len(measurement_lines) == 2, "expected one M for syndromes, one for flags"


def test_flag_outcomes_are_discovered_as_deterministic_checks(
    steane: qc.Qodec,
) -> None:
    """A flag bit is deterministic, so completion must find it as a check.

    That is what turns a flagged hook error into a detector the decoder sees.
    """
    idle = steane.layers[0].gadgets["syndrome"]

    flag_checks = [
        check
        for check in idle.checks
        if len(check) == 1 and str(check[0]).startswith("circuit.readouts")
    ]
    assert len(flag_checks) == 6, "one flag check per weight-4 stabilizer"


def test_a_weight_two_stabilizer_carries_no_flag() -> None:
    """Flag brackets must stay nested, which a weight-2 stabilizer cannot host."""
    from qdk.ec._build import _flag_capacity

    assert _flag_capacity(2) == 0
    assert _flag_capacity(3) == 1
    assert _flag_capacity(4) == 1
    assert _flag_capacity(6) == 2


def test_flag_count_defaults_to_the_codes_error_correcting_radius() -> None:
    """Chamberland-Beverland call for t = (d-1)//2 flags for a distance-d code."""
    steane_code = _code("steane", catalog.make_steane_code)

    notes = build_notes(qodec_from_code(steane_code))

    assert notes["flags_per_stabilizer"] == 1


def test_default_strategy_is_flagged() -> None:
    code = _code("steane", catalog.make_steane_code)

    built = build_qodec(code)

    assert built == build_qodec(code, strategy="flagged-css/v1")
    assert build_notes(built)["flags_per_stabilizer"] == 1


def test_bare_strategy_uses_the_unflagged_circuit() -> None:
    code = _code("steane", catalog.make_steane_code)

    built = build_qodec(code, strategy="bare-css/v1")

    assert built == qodec_from_code(
        code, flags=0, strict=True, description=built.description
    )
    assert built.description.endswith("Strategy: bare-css/v1.")
    source = built.layers[0].gadgets["syndrome"].circuit.source
    assert build_notes(built)["flags_per_stabilizer"] == 0
    # 7 data qubits + one ancilla per stabilizer, and nothing else.
    assert max(int(t) for line in source.splitlines() for t in line.split()[1:]) == 12


@pytest.mark.parametrize("use_analysis", [False, True])
def test_bare_strategy_skips_distance_computation(
    monkeypatch: pytest.MonkeyPatch, use_analysis: bool
) -> None:
    from qdk.ec import _build

    def fail_distance(code: qc.Code) -> None:
        raise AssertionError("bare construction must not compute code distance")

    code = _code("steane", catalog.make_steane_code)
    argument = CodeProfile(code) if use_analysis else code
    monkeypatch.setattr(_build, "code_distance_of", fail_distance)

    built = build_qodec(
        argument,
        strategy="bare-css/v1",
        name="bare_steane",
        description="No flag ancillas.",
    )

    assert built.name == "bare_steane"
    assert built.description == "No flag ancillas."
    assert build_notes(built)["flags_per_stabilizer"] == 0


def test_build_from_profile_uses_its_operator_snapshot() -> None:
    code = _code("steane", catalog.make_steane_code)
    profile = CodeProfile(code)
    expected = build_qodec(profile, strategy="bare-css/v1", name="steane")
    code.stabilizers = []
    code.x = []
    code.z = []

    assert build_qodec(profile, strategy="bare-css/v1", name="steane") == expected


@pytest.mark.parametrize("strategy", ["bare-css/v2", "unknown"])
def test_unknown_build_strategy_is_rejected(strategy: str) -> None:
    code = _code("steane", catalog.make_steane_code)

    with pytest.raises(ValueError, match="unknown qodec construction strategy"):
        build_qodec(code, strategy=strategy)


def test_negative_flag_counts_are_rejected() -> None:
    with pytest.raises(ValueError, match="non-negative"):
        qodec_from_code(_code("steane", catalog.make_steane_code), flags=-1)


def test_syndrome_round_never_touches_data_qubits_with_single_qubit_gates(
    steane: qc.Qodec,
) -> None:
    source = steane.layers[0].gadgets["syndrome"].circuit.source

    for line in source.splitlines():
        gate, *targets = line.split()
        if gate in ("R", "H", "M"):
            assert all(int(target) >= 7 for target in targets), line


def test_measure_gadgets_are_transversal(steane: qc.Qodec) -> None:
    gadgets = steane.layers[0].gadgets

    assert gadgets["measure_z_all"].circuit.source == "M 0 1 2 3 4 5 6\n"
    assert gadgets["measure_x_all"].circuit.source == "H 0 1 2 3 4 5 6\nM 0 1 2 3 4 5 6\n"


def test_transversal_clifford_circuits(steane: qc.Qodec) -> None:
    gadgets = steane.layers[0].gadgets

    assert gadgets["h_all"].circuit.source == "H 0 1 2 3 4 5 6\n"
    assert gadgets["cx_all"].circuit.source == (
        "CX 0 7\nCX 1 8\nCX 2 9\nCX 3 10\nCX 4 11\nCX 5 12\nCX 6 13\n"
    )


def test_physical_instruction_set_has_no_unused_pauli_gates(steane: qc.Qodec) -> None:
    assert set(steane.layers[1].instruction_set.instructions) == {
        "R",
        "H",
        "CX",
        "CZ",
        "M",
    }


def test_circuits_are_tagged_as_stim(steane: qc.Qodec) -> None:
    assert all(
        gadget.circuit.format == "stim" for gadget in steane.layers[0].gadgets.values()
    )


# ── Semantics ───────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("label", "factory"),
    [(case[0], case[1]) for case in CSS_CODES],
    ids=[case[0] for case in CSS_CODES],
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
    [(case[0], case[1]) for case in CSS_CODES],
    ids=[case[0] for case in CSS_CODES],
)
def test_gadgets_that_hold_state_discover_checks(label: str, factory) -> None:
    built = qodec_from_code(_code(label, factory))

    for mnemonic in ("prepare_z_all", "prepare_x_all", "syndrome"):
        gadget = built.layers[0].gadgets[mnemonic]
        assert gadget.checks, f"{mnemonic} discovered no checks"


def test_measure_gadgets_bind_a_readout_per_logical_qubit(steane: qc.Qodec) -> None:
    for mnemonic in ("measure_z_all", "measure_x_all"):
        gadget = steane.layers[0].gadgets[mnemonic]
        assert len(gadget.readouts) == 1, mnemonic


def test_idle_checks_reference_both_boundaries(steane: qc.Qodec) -> None:
    atoms = {
        str(atom) for check in steane.layers[0].gadgets["syndrome"].checks for atom in check
    }

    assert any(atom.startswith("in[0].stabilizers") for atom in atoms)
    assert any(atom.startswith("out[0].stabilizers") for atom in atoms)


def test_built_code_keeps_its_distance() -> None:
    built = qodec_from_code(_code("steane", catalog.make_steane_code))

    code_distance, _ = distance.code_distance_of(built.codes["steane"])

    assert code_distance == 3


# ── Audit ───────────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    "code",
    [_code(case[0], case[1]) for case in CSS_CODES]
    + [
        qc.Code(
            "C4",
            ["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
            ["X_0 X_1", "X_0 X_2"],
            ["Z_0 Z_2", "Z_0 Z_1"],
        )
    ],
    ids=[case[0] for case in CSS_CODES] + ["C4"],
)
@pytest.mark.parametrize("strategy", ["bare-css/v1", "flagged-css/v1"])
def test_build_strategies_return_audit_clean_qodecs(
    code: qc.Code, strategy: str
) -> None:
    built = build_qodec(code, strategy=strategy, strict=False)

    report = _audit.audit(built)
    assert not report.diagnostics, str(report)
    expected = {
        "prepare_z_all",
        "prepare_x_all",
        "syndrome",
        "measure_z_all",
        "measure_x_all",
        "cx_all",
    }
    if code.name == "steane":
        expected.add("h_all")
    assert set(built.layers[0].gadgets) == expected
    assert set(built.layers[0].instruction_set.instructions) == expected


@pytest.mark.parametrize("strategy", ["bare-css/v1", "flagged-css/v1"])
@pytest.mark.parametrize("strict", [False, True])
def test_build_rejects_invalid_final_declarations(
    monkeypatch, strategy, strict
) -> None:
    from qdk.ec import _build

    original = _build._rebound

    def rebound_without_readouts(gadget, instruction):
        rebound = original(gadget, instruction)
        if instruction.mnemonic == "measure_z_all":
            rebound.readouts = []
        return rebound

    monkeypatch.setattr(_build, "_rebound", rebound_without_readouts)
    with pytest.raises(ValueError, match="did not pass audit"):
        build_qodec(
            _code("steane", catalog.make_steane_code), strategy=strategy, strict=strict
        )


def test_hand_authored_readouts_need_incoming_frame_corrections() -> None:
    fixture = c4()
    report = _audit.audit(fixture)
    assert len(report.errors) == 4
    assert {item.rule for item in report.errors} == {"gadget/readout-mismatch"}
    for basis in ("x", "z"):
        gadget = fixture.layers[0].gadgets[f"measure_{basis}{basis}"]
        gadget.readouts = [
            (*readout.equation, f"in[0].{basis}[{index}]")
            for index, readout in enumerate(gadget.readouts)
        ]
    corrected = _audit.audit(fixture)
    assert corrected.ok, str(corrected)


# ── Round-tripping ──────────────────────────────────────────────────────────


def test_built_qodec_round_trips_through_yaml(steane: qc.Qodec, tmp_path: Path) -> None:
    restored = _round_tripped(steane, tmp_path / "bundle")

    assert restored == steane


def test_structured_omissions_round_trip_through_yaml(tmp_path: Path) -> None:
    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    restored = _round_tripped(built, tmp_path / "bundle")

    assert build_notes(restored)["omitted"] == build_notes(built)["omitted"]


def test_built_qodec_round_trips_through_disk(steane: qc.Qodec, tmp_path: Path) -> None:
    steane.save(str(tmp_path / "bundle"))
    restored = qc.Qodec.load(str(tmp_path / "bundle" / steane.manifest_filename))

    assert restored == steane


def test_completion_is_idempotent_on_a_built_qodec(
    steane: qc.Qodec,
) -> None:
    recompleted = complete_qodec(steane)

    assert not _audit.audit(recompleted).diagnostics
    for mnemonic, gadget in steane.layers[0].gadgets.items():
        before = {frozenset(str(a) for a in c) for c in gadget.checks}
        after = {
            frozenset(str(a) for a in c)
            for c in recompleted.layers[0].gadgets[mnemonic].checks
        }
        assert before == after, mnemonic
        assert gadget.readouts == recompleted.layers[0].gadgets[mnemonic].readouts


# ── Partial build ───────────────────────────────────────────────────────


def test_a_non_z_logical_basis_omits_the_gadgets_it_cannot_support() -> None:
    """The five-qubit code's conventional basis has X components in logical Z."""
    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    omitted = build_notes(built)["omitted"]
    assert "prepare_z_all" in omitted
    assert "measure_z_all" in omitted
    assert "syndrome" in built.layers[0].gadgets
    assert set(built.layers[0].instruction_set.instructions) == set(
        built.layers[0].gadgets
    )


def test_omissions_carry_structured_reasons() -> None:
    from collections.abc import Mapping

    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    assert all(
        isinstance(reason, Mapping)
        and set(reason) == {"stage", "kind", "message"}
        and reason["stage"] in {"completion", "verification"}
        and isinstance(reason["kind"], str)
        and reason["kind"]
        and isinstance(reason["message"], str)
        and reason["message"]
        for reason in build_notes(built)["omitted"].values()
    )


def test_unexpected_completion_failure_propagates(monkeypatch) -> None:
    from qdk.ec import _build

    original = _build.complete_gadget

    def complete_or_fail(gadget: qc.Gadget) -> qc.Gadget:
        if gadget.implements.mnemonic == "syndrome":
            raise RuntimeError("unexpected completion failure")
        return original(gadget)

    monkeypatch.setattr(_build, "complete_gadget", complete_or_fail)

    with pytest.raises(RuntimeError, match="unexpected completion failure"):
        qodec_from_code(_code("steane", catalog.make_steane_code))


def test_unexpected_verification_failure_propagates(monkeypatch) -> None:
    from qdk.ec import _build

    def fail_verification(gadget: qc.Gadget) -> str | None:
        raise RuntimeError("unexpected verification failure")

    monkeypatch.setattr(_build, "gadget_action_mismatch", fail_verification)

    with pytest.raises(RuntimeError, match="unexpected verification failure"):
        qodec_from_code(_code("steane", catalog.make_steane_code))


def test_strict_mode_raises_instead_of_omitting() -> None:
    code = _code("five_qubit", catalog.make_five_qubit_code)

    with pytest.raises(ValueError, match="could not build"):
        qodec_from_code(code, strict=True)


def test_strict_mode_is_a_no_op_when_everything_builds() -> None:
    code = _code("steane", catalog.make_steane_code)

    assert set(qodec_from_code(code, strict=True).layers[0].gadgets) == set(
        qodec_from_code(code).layers[0].gadgets
    )


@pytest.mark.parametrize("strategy", ["flagged-css/v1", "bare-css/v1"])
def test_unsupported_transversal_h_is_omitted_or_raises(strategy: str) -> None:
    code = _code("repetition3", lambda: catalog.make_repetition_code(3))
    built = build_qodec(code, strategy=strategy, strict=False)

    assert "h_all" not in built.layers[0].gadgets
    assert "cx_all" in built.layers[0].gadgets
    assert set(build_notes(built)["omitted"]) == {"h_all"}
    assert build_notes(built)["omitted"]["h_all"]["stage"] == "verification"
    with pytest.raises(ValueError, match="could not build 'h_all'"):
        build_qodec(code, strategy=strategy)


def test_transversal_h_must_match_the_declared_logical_action() -> None:
    built = qodec_from_code(c4().codes["C4"])

    assert "h_all" not in built.layers[0].gadgets
    failure = build_notes(built)["omitted"]["h_all"]
    assert failure["stage"] == "verification"
    assert failure["kind"] == "ActionMismatch"


def test_logical_basis_choice_can_decide_whether_readout_builds() -> None:
    """Two valid logical bases for [[4,2,2]] behave differently.

    This pins an observed basis-dependence in the observable-discovery pass
    completion relies on, so the difference is visible rather than silent.
    """
    fixture_basis = qodec_from_code(c4().codes["C4"], name="c4_fixture_basis")
    catalog_basis = qodec_from_code(_code("c422", catalog.make_422_code))

    assert set(build_notes(fixture_basis)["omitted"]) == {"h_all"}
    assert "measure_z_all" in build_notes(catalog_basis)["omitted"]


# ── Multi-logical-qubit codes ───────────────────────────────────────────────


def test_a_k_equals_two_code_gets_a_two_block_cnot() -> None:
    fixture = c4()
    built = qodec_from_code(fixture.codes["C4"], name="c4_build")

    assert set(built.layers[0].gadgets) == {
        "prepare_z_all",
        "prepare_x_all",
        "syndrome",
        "measure_z_all",
        "measure_x_all",
        "cx_all",
    }
    gadget = built.layers[0].gadgets["cx_all"]
    assert len(gadget.implements.inputs) == len(gadget.implements.outputs) == 2
    for boundary in (gadget.inputs, gadget.outputs):
        assert [tuple(encoding.support) for encoding in boundary] == [
            ("0", "1", "2", "3"),
            ("4", "5", "6", "7"),
        ]
    assert (
        gadget.implements.action
        == fixture.layers[0].gadgets["transversal_cx"].implements.action
    )
    assert action.gadget_action_mismatch(gadget) is None


def test_transversal_cnot_is_verified_for_a_large_k_code() -> None:
    """Logical coordinates remain authored-order even when k is large."""
    built = qodec_from_code(_code("iceberg8", lambda: catalog.make_iceberg_code(8)))

    gadget = built.layers[0].gadgets["cx_all"]
    assert len(built.codes["iceberg8"].x) == 6
    assert len(gadget.inputs) == len(gadget.outputs) == 2
    assert action.gadget_action_mismatch(gadget) is None
    assert set(built.layers[0].gadgets) | set(build_notes(built)["omitted"]) == {
        "prepare_z_all",
        "prepare_x_all",
        "syndrome",
        "measure_z_all",
        "measure_x_all",
        "h_all",
        "cx_all",
    }


def test_transversal_h_covers_every_logical_qubit() -> None:
    code = qc.Code("pair", stabilizers=[], x=["X_0", "X_1"], z=["Z_0", "Z_1"])
    built = qodec_from_code(code, flags=0, strict=True)

    gadget = built.layers[0].gadgets["h_all"]
    assert gadget.circuit.source == "H 0 1\n"
    assert action.gadget_action_mismatch(gadget) is None


# ── Rejected inputs ─────────────────────────────────────────────────────────


def test_y_components_are_rejected_with_an_actionable_message() -> None:
    code = qc.Code("has_y", stabilizers=["Y_0 X_1"], x=["X_1"], z=["Z_0 Z_1"])

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
