"""``qdk.ec.develop.qodec_from_code`` — synthesizing a qodec from a code.

The suite is organised around what synthesis promises: a *structurally* valid
qodec, whose gadgets are *semantically* verified, that *round-trips*, and that
is actually *runnable* on a target.
"""

from __future__ import annotations

from pathlib import Path

import pytest
import qodec

from ec_tests.testing import code_catalog as catalog
from ec_tests.testing.optional import requires_stim
from ec_tests.testing.qodecs import c4
from qdk.ec import audit, develop, profile
from qdk.ec.develop import qodec_from_code, synthesis_notes

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


def _code(label: str, factory) -> qodec.Code:
    return factory().to_qodec(label)


@pytest.fixture(scope="module")
def steane() -> qodec.Qodec:
    return qodec_from_code(_code("steane", catalog.make_steane_code))


# ── Structure ───────────────────────────────────────────────────────────────


def test_result_is_a_two_layer_qodec(steane: qodec.Qodec) -> None:
    assert len(steane.layers) == 2
    assert steane.layers[0].isa.name == "steane"
    assert steane.layers[1].isa.name == "stim"
    assert steane.layers[1].gadgets == {}


def test_logical_block_encodes_the_logical_qubits(steane: qodec.Qodec) -> None:
    (block,) = steane.layers[0].isa.blocks

    assert block.name == "steane"
    assert block.encodes == 1


def test_every_declared_instruction_has_a_gadget(steane: qodec.Qodec) -> None:
    layer = steane.layers[0]

    assert set(layer.isa.instructions) == set(layer.gadgets)


def test_the_expected_instruction_menu_is_synthesized(steane: qodec.Qodec) -> None:
    assert set(steane.layers[0].gadgets) == {
        "prepare_z",
        "prepare_x",
        "idle",
        "measure_z",
        "measure_x",
        "x0",
        "z0",
    }


def test_the_code_is_carried_through(steane: qodec.Qodec) -> None:
    assert "steane" in steane.codes
    assert list(steane.codes["steane"].stabilizers)


def test_name_and_description_default_from_the_code() -> None:
    built = qodec_from_code(_code("steane", catalog.make_steane_code))

    assert built.name == "steane"
    assert "[[7, 1]]" in built.description


def test_name_and_description_can_be_overridden() -> None:
    built = qodec_from_code(
        _code("steane", catalog.make_steane_code),
        name="my_codec",
        description="hand written",
    )

    assert built.name == "my_codec"
    assert built.description == "hand written"
    assert built.layers[0].isa.name == "my_codec"


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


def test_syndrome_round_uses_one_ancilla_per_stabilizer(steane: qodec.Qodec) -> None:
    code = steane.codes["steane"]
    source = steane.layers[0].gadgets["idle"].circuit.source

    ancillas = {
        int(target)
        for line in source.splitlines()
        if line.startswith("M ")
        for target in line.split()[1:]
    }
    assert ancillas == {7 + offset for offset in range(len(list(code.stabilizers)))}


def test_syndrome_round_never_touches_data_qubits_with_single_qubit_gates(
    steane: qodec.Qodec,
) -> None:
    source = steane.layers[0].gadgets["idle"].circuit.source

    for line in source.splitlines():
        gate, *targets = line.split()
        if gate in ("R", "H", "M"):
            assert all(int(target) >= 7 for target in targets), line


def test_measure_gadgets_are_transversal(steane: qodec.Qodec) -> None:
    gadgets = steane.layers[0].gadgets

    assert gadgets["measure_z"].circuit.source == "M 0 1 2 3 4 5 6\n"
    assert gadgets["measure_x"].circuit.source == "H 0 1 2 3 4 5 6\nM 0 1 2 3 4 5 6\n"


def test_logical_pauli_gadget_applies_the_codes_operator(steane: qodec.Qodec) -> None:
    code = steane.codes["steane"]
    x_operator = str(list(code.x)[0])
    expected = sorted(
        int(token.split("_")[1]) for token in x_operator.split() if token.startswith("X")
    )

    source = steane.layers[0].gadgets["x0"].circuit.source

    assert sorted(int(t) for t in source.split()[1:]) == expected


def test_circuits_are_tagged_as_stim(steane: qodec.Qodec) -> None:
    assert all(
        gadget.circuit.format == "stim"
        for gadget in steane.layers[0].gadgets.values()
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
        mnemonic: profile.gadget_action_mismatch(gadget)
        for mnemonic, gadget in built.layers[0].gadgets.items()
        if profile.gadget_action_mismatch(gadget) is not None
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


def test_measure_gadgets_bind_a_readout_per_logical_qubit(steane: qodec.Qodec) -> None:
    for mnemonic in ("measure_z", "measure_x"):
        gadget = steane.layers[0].gadgets[mnemonic]
        assert len(gadget.readouts) == 1, mnemonic


def test_idle_checks_reference_both_boundaries(steane: qodec.Qodec) -> None:
    atoms = {
        str(atom)
        for check in steane.layers[0].gadgets["idle"].checks
        for atom in check
    }

    assert any(atom.startswith("in[0].stabilizers") for atom in atoms)
    assert any(atom.startswith("out[0].stabilizers") for atom in atoms)


def test_synthesized_code_keeps_its_distance() -> None:
    built = qodec_from_code(_code("steane", catalog.make_steane_code))

    distance, _ = profile.code_distance_of(built.codes["steane"])

    assert distance == 3


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
        for d in audit.audit(built).errors()
        if d.rule != _KNOWN_AUDIT_RULE
    ]
    assert unexpected == []


def test_the_known_audit_rule_also_fires_on_the_hand_authored_fixture() -> None:
    """Pins the claim that ``_KNOWN_AUDIT_RULE`` is not a synthesis defect."""
    fixture = c4()

    rules = {
        d.rule
        for gadget in fixture.layers[0].gadgets.values()
        for d in audit.Auditor().audit_gadget(gadget, codec=fixture).errors()
    }
    assert _KNOWN_AUDIT_RULE in rules


# ── Round-tripping ──────────────────────────────────────────────────────────


def test_synthesized_qodec_round_trips_through_yaml(steane: qodec.Qodec) -> None:
    restored = develop.from_yaml(develop.to_yaml(steane))

    assert restored.name == steane.name
    assert sorted(restored.layers[0].gadgets) == sorted(steane.layers[0].gadgets)


def test_synthesized_qodec_round_trips_through_disk(
    steane: qodec.Qodec, tmp_path: Path
) -> None:
    develop.save(steane, tmp_path / "bundle")
    restored = develop.load(tmp_path / "bundle")

    assert restored.name == steane.name
    assert sorted(restored.codes) == sorted(steane.codes)


def test_completion_is_idempotent_on_a_synthesized_qodec(
    steane: qodec.Qodec,
) -> None:
    recompleted = develop.complete_qodec(steane)

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


def test_omissions_carry_a_reason() -> None:
    built = qodec_from_code(_code("five_qubit", catalog.make_five_qubit_code))

    assert all(
        isinstance(reason, str) and reason
        for reason in synthesis_notes(built)["omitted"].values()
    )


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
    """Guards the action-token resolution: k=6 needs a non-identity map."""
    built = qodec_from_code(_code("iceberg8", lambda: catalog.make_iceberg_code(8)))

    pauli_gadgets = {
        mnemonic: gadget
        for mnemonic, gadget in built.layers[0].gadgets.items()
        if mnemonic[0] in "xz" and mnemonic[1:].isdigit()
    }
    assert len(pauli_gadgets) == 12
    assert all(
        profile.gadget_action_mismatch(gadget) is None
        for gadget in pauli_gadgets.values()
    )


# ── Rejected inputs ─────────────────────────────────────────────────────────


def test_y_components_are_rejected_with_an_actionable_message() -> None:
    code = qodec.Code("has_y", stabilizers=["Y_0 X_1"], x=["X_0"], z=["Z_0 Z_1"])

    with pytest.raises(NotImplementedError, match="Y components"):
        qodec_from_code(code)


def test_a_code_with_no_logical_qubits_is_rejected() -> None:
    """A [[1, 0]] code: a valid stabilizer code that encodes nothing."""
    code = qodec.Code("full_rank", stabilizers=["Z_0"], x=[], z=[])

    with pytest.raises(ValueError, match="no logical qubits"):
        qodec_from_code(code)


def test_an_unnamed_code_requires_an_explicit_name() -> None:
    code = qodec.Code("", stabilizers=["Z_0 Z_1"], x=["X_0 X_1"], z=["Z_0"])

    with pytest.raises(ValueError, match="no name"):
        qodec_from_code(code)


# ── Execution ───────────────────────────────────────────────────────────────


@requires_stim
def test_a_synthesized_qodec_samples_without_detections_when_noiseless(
    steane: qodec.Qodec,
) -> None:
    import numpy as np

    from qdk.ec import targets

    program = _memory_program(steane)
    sampler = targets.StimSampler(steane)

    shots = np.asarray(sampler.execute(program, shots=64))
    events = sampler.emitter.detection_events(program, shots)

    assert events.shape[1] > 0, "the synthesized qodec produced no detectors"
    assert events.sum() == 0


@requires_stim
def test_a_synthesized_qodec_detects_noise(steane: qodec.Qodec) -> None:
    import numpy as np

    from qdk.ec import targets

    program = _memory_program(steane)
    sampler = targets.StimSampler(steane, noise={"p_data": 0.05, "p_meas": 0.05})

    shots = np.asarray(sampler.execute(program, shots=512))
    fired = sampler.emitter.detection_events(program, shots).any(axis=1)

    assert fired.mean() > 0.1


@requires_stim
def test_a_detector_error_model_can_be_built(steane: qodec.Qodec) -> None:
    from qdk.ec import targets

    dem = targets.detector_error_model_of(
        steane, _memory_program(steane), {"p_data": 0.001, "p_meas": 0.001}
    )

    assert str(dem).strip()


@requires_stim
def test_idle_gadget_has_a_circuit_level_distance(steane: qodec.Qodec) -> None:
    from qdk.ec import targets

    distance, _ = targets.gadget_distance_of(
        steane.layers[0].gadgets["idle"], targets.depolarizing(0.001)
    )

    assert distance >= 1


def _memory_program(codec: qodec.Qodec):
    """prepare_z / idle / measure_z over the codec's logical ISA."""
    from qodec.circuits import Program

    isa = codec.layers[0].isa

    def call(mnemonic: str) -> qodec.instructions.InstructionCall:
        instruction = isa.instruction(mnemonic)
        inputs = {str(i): "q" for i in range(len(list(instruction.inputs)))}
        outputs = {str(i): "q" for i in range(len(list(instruction.outputs)))}
        if not inputs and not outputs:
            return qodec.instructions.InstructionCall(mnemonic)
        return qodec.instructions.InstructionCall(
            mnemonic, inputs=inputs, outputs=outputs
        )

    return Program([call(m) for m in ("prepare_z", "idle", "measure_z")], isa)
