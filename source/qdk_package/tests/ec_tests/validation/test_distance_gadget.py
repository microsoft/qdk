"""Distance counts allowed circuit faults, not single-qubit Pauli factors."""

from __future__ import annotations

from copy import deepcopy
from itertools import product
from functools import reduce
from importlib import import_module
from operator import mul
from unittest.mock import patch

from binar import BitMatrix
import qodec as qc
import pytest

from qdk.ec import (
    ChannelAction,
    FaultEffect,
    FaultEvent,
    GadgetProfile,
    Pauli,
    CodeProfile,
)
from qdk.ec._analysis.distance_solvers import EnumerationSolverOptions
from qdk.ec._analysis.propagation.frames import FrameGroup, PauliFrame
from qdk.ec._analysis.propagation.interpreter import propagate_faults
from qdk.ec._profile import _fault_observables
from ec_tests.testing.optional import requires_highs, requires_stim
from ec_tests.testing.qodecs import c4


def _flipped_indices(effect: FaultEffect, field: str) -> set[int]:
    return {
        reference.segments[1].value
        for reference in effect
        if reference.segments[0] == qc.Reference.Field(field)
        and isinstance(reference.segments[1], qc.Reference.Index)
    }


def test_fault_observables_collect_outcome_indexed_generators() -> None:
    stabilizer = PauliFrame(Pauli("Z_0"), frozenset({1}))
    image = PauliFrame(Pauli("X_1"), frozenset({2}))
    measurement = PauliFrame(Pauli("Z_2"), frozenset({3}))
    action = ChannelAction._create(
        FrameGroup([measurement]), FrameGroup([stabilizer]), {Pauli("X_0"): image}
    )
    assert _fault_observables(action) == FrameGroup(
        [stabilizer, image, PauliFrame(Pauli.identity(), frozenset({3}))]
    )


def test_distance_constraints_do_not_use_display_iteration() -> None:
    from qdk.ec._distance import _FaultDistanceData

    constraints = [
        "checks[0]",
        "readouts[1]",
        "out[0].stabilizers[0]",
        "out[1].stabilizers[0]",
    ]
    effects = [FaultEffect([reference]) for reference in constraints]
    effects.extend(
        [
            FaultEffect(["readouts[0]", "out[0].x[0]", "out[0].z[0]"]),
            FaultEffect(constraints),
        ]
    )
    faults = tuple(
        FaultEvent.after(index, Pauli("X_0")) for index in range(len(effects))
    )
    indicators = tuple(frozenset({index}) for index in range(len(effects)))
    with (
        patch.object(
            FaultEffect, "__iter__", side_effect=AssertionError("display iteration")
        ),
        patch("qdk.ec._distance.OddCycles") as solver_input,
    ):
        _FaultDistanceData.of(
            faults, effects, indicators, flag_positions=frozenset({1})
        )
    matrix, actual_indicators = solver_input.call_args.args
    assert len(matrix) == len(effects)
    assert all(len(column) == 1 for column in matrix[:4])
    assert len(set(matrix[:4])) == 4
    assert not matrix[4]
    assert matrix[5] == frozenset.union(*matrix[:4])
    assert actual_indicators == indicators


def _measurement_gadget(physical: qc.InstructionSet, qubit_count: int) -> qc.Gadget:
    qubit = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction("idle", inputs=[qubit], outputs=[qubit]),
    ]
    code = qc.Code(
        "repetition",
        [f"Z_{index} Z_{index + 1}" for index in range(qubit_count - 1)],
        [" ".join(f"X_{index}" for index in range(qubit_count))],
        ["Z_0"],
    )
    return qc.Gadget(
        qc.Instruction(
            "measure",
            inputs=[qc.instructions.BlockOperand("repetition")],
            action=[qc.actions.Observe(["Z_0"])],
        ),
        qc.gadgets.Circuit(
            physical,
            "\n".join(
                f"- {mnemonic}: [{index}]"
                for mnemonic in ("idle", "M")
                for index in range(qubit_count)
            ),
            format="yaml",
        ),
        inputs=[
            qc.gadgets.Encoding(
                code, support=[str(index) for index in range(qubit_count)]
            )
        ],
        checks=[
            [
                f"circuit.readouts[{index}]",
                f"circuit.readouts[{index + 1}]",
                f"in[0].stabilizers[{index}]",
            ]
            for index in range(qubit_count - 1)
        ],
        readouts=[["circuit.readouts[0]"]],
    )


@requires_stim
def test_measurement_only_distance_includes_readout_flips(rep3_qodec: qc.Qodec) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    gadget = _measurement_gadget(physical, 3)
    gadget.circuit = qc.gadgets.Circuit(physical, "M 0 1 2", format="stim")
    profile = GadgetProfile(gadget)

    distance = profile.distance()
    witness = distance.witness.factors
    assert distance == len(witness) == 3
    bounds = profile.distance_bounds(solver=EnumerationSolverOptions())
    lower, upper = bounds.lower_bound, bounds.upper_bound
    bounded = bounds.witness.factors
    assert lower == upper == len(bounded) == 3
    (effect,) = profile.effects_of([distance.witness.product])
    assert effect == FaultEffect(["readouts[0]"])


@pytest.mark.parametrize(
    "observable,error,width",
    [("Z_0", "X_0", 1), ("X_0", "Z_0", 1), ("Z_0 Z_1", "X_1", 2)],
)
def test_nondestructive_readout_flip_matches_pauli_sandwich(
    observable: str, error: str, width: int
) -> None:
    operands = [qc.instructions.BlockOperand("qubit") for _ in range(width)]
    physical = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[
            qc.Instruction("wait", inputs=operands, outputs=operands),
            qc.Instruction(
                "measure",
                inputs=operands,
                outputs=operands,
                action=[qc.actions.Observe([observable])],
            ),
        ],
    )
    targets = ", ".join(str(index) for index in range(width))
    circuit = qc.gadgets.Circuit(
        physical, f"- wait: [{targets}]\n- measure: [{targets}]", format="yaml"
    )
    code = qc.Code(
        "data",
        [],
        [f"X_{index}" for index in range(width)],
        [f"Z_{index}" for index in range(width)],
    )
    encoding = qc.gadgets.Encoding(code, support=[str(index) for index in range(width)])
    logical = qc.instructions.BlockOperand("data")
    gadget = qc.Gadget(
        qc.Instruction(
            "measure",
            inputs=[logical],
            outputs=[logical],
            action=[qc.actions.Observe([observable])],
        ),
        circuit,
        inputs=[encoding],
        outputs=[encoding],
        readouts=[["circuit.readouts[0]"]],
    )
    profile = GadgetProfile(gadget)
    readout_fault = FaultEvent.after(1, readout_flips=0)
    sandwich = FaultEvent.after(0, Pauli(error)) * FaultEvent.after(1, Pauli(error))
    direct_effect, sandwich_effect = profile.effects_of([readout_fault, sandwich])
    assert direct_effect == sandwich_effect
    assert direct_effect == FaultEffect(["readouts[0]"])
    distance = profile.distance(faults=[readout_fault])
    assert distance == 1
    assert distance.witness.factors == (readout_fault,)
    post_fault = FaultEvent.after(1, Pauli(error))
    (post_effect,) = profile.effects_of([post_fault])
    assert not _flipped_indices(post_effect, "readouts")
    assert any(
        reference.segments[0] == qc.Reference.Field("out") for reference in post_effect
    )
    assert readout_fault in profile._circuit_faults()
    assert readout_fault * post_fault in profile._circuit_faults()
    assert readout_fault in dict(profile.fault_effects)


@requires_stim
def test_readout_noise_uses_call_local_positions_not_reset_rows(
    rep3_qodec: qc.Qodec,
) -> None:
    circuit = qc.gadgets.Circuit(
        rep3_qodec.layers[1].instruction_set, "R 0\nM 0\nR 1\nM 1", format="stim"
    )
    profile = GadgetProfile(circuit)
    first, second, both = profile.effects_of(
        [
            FaultEvent.after(1, readout_flips=0),
            FaultEvent.after(3, readout_flips=[0]),
            FaultEvent.after(1, readout_flips=0) * FaultEvent.after(3, readout_flips=0),
        ]
    )
    assert first == FaultEffect(["checks[0]", "readouts[0]"])
    assert second == FaultEffect(["checks[1]", "readouts[1]"])
    assert both == first ^ second
    for index in (-1, 1):
        with pytest.raises(ValueError, match="fault readout index"):
            profile.effects_of([FaultEvent.after(3, readout_flips=index)])
    with pytest.raises(ValueError, match="call 0 with 0 readouts"):
        profile.effects_of([FaultEvent.after(0, readout_flips=0)])
    for call in (-1, 4):
        with pytest.raises(ValueError, match="fault call index"):
            profile.effects_of([FaultEvent.after(call, readout_flips=0)])


@pytest.mark.parametrize(
    "readout_flips,expected", [(0, {1}), ([1], {2}), ([0, 1], {1, 2}), ([0, 0], {1})]
)
def test_local_readouts_can_address_repeated_measurements_in_one_call(
    rep3_qodec: qc.Qodec, readout_flips: int | list[int], expected: set[int]
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    qubit = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction(
            "twice",
            inputs=[qubit],
            outputs=[qubit],
            action=[qc.actions.Observe(["Z_0"]), qc.actions.Observe(["Z_0"])],
        ),
    ]
    circuit = qc.gadgets.Circuit(
        physical, "- R: [0]\n- M: [0]\n- twice: [1]", format="yaml"
    )
    profile = GadgetProfile(circuit)
    fault = FaultEvent.after(2, readout_flips=readout_flips)
    (effect,) = profile.effects_of([fault])
    assert _flipped_indices(effect, "readouts") == expected
    assert not any(
        reference.segments[0] == qc.Reference.Field("out") for reference in effect
    )
    assert fault in profile._circuit_faults()
    with pytest.raises(ValueError, match="call 2 with 2 readouts"):
        profile.effects_of([FaultEvent.after(2, readout_flips=2)])


def test_one_call_can_corrupt_several_readouts_as_one_fault(
    rep3_qodec: qc.Qodec,
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    gadget = _measurement_gadget(physical, 2)
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction(
            "measure_pair",
            inputs=[
                qc.instructions.BlockOperand("qubit"),
                qc.instructions.BlockOperand("qubit"),
            ],
            action=[qc.actions.Observe(["Z_0", "Z_1"])],
        ),
    ]
    gadget.circuit = qc.gadgets.Circuit(
        physical, "- measure_pair: [0, 1]", format="yaml"
    )
    profile = GadgetProfile(gadget)
    fault = FaultEvent.after(0, readout_flips=[0, 1])
    assert fault in profile._circuit_faults()
    distance = profile.distance()
    witness = distance.witness.factors
    assert distance == len(witness) == 1
    distance = profile.distance(faults=[fault])
    assert distance == 1
    assert distance.witness.factors == (fault,)
    (effect,) = profile.effects_of(witness)
    assert effect == FaultEffect(["readouts[0]"])


@requires_stim
def test_c4_z_measurement_has_distance_two_with_readout_noise() -> None:
    gadget = c4().layers[0].gadgets["measure_zz"]
    gadget.readouts = [
        [*readout.equation, f"in[0].z[{index}]"]
        for index, readout in enumerate(gadget.readouts)
    ]
    profile = GadgetProfile(gadget)
    distance = profile.distance()
    witness = distance.witness.factors
    assert distance == len(witness) == 2
    (effect,) = profile.effects_of([distance.witness.product])
    assert not _flipped_indices(effect, "checks") and _flipped_indices(
        effect, "readouts"
    )


@requires_highs
def test_highs_gadget_distance_returns_replayable_witness(rep3_qodec: qc.Qodec) -> None:
    gadget = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 3)
    profile = GadgetProfile(gadget)
    distance = profile.distance(solver="highs")
    witness = distance.witness.factors
    bounds = profile.distance_bounds(solver="highs")
    lower, upper = bounds.lower_bound, bounds.upper_bound
    bounded = bounds.witness.factors
    assert distance == lower == upper == 3
    for factors in (witness, bounded):
        assert len(factors) == 3
        (effect,) = profile.effects_of([reduce(mul, factors, FaultEvent({}))])
        assert effect == FaultEffect(["readouts[0]"])
    with pytest.raises(RuntimeError, match="exact distance"):
        profile.distance(solver="highs", upper_bound=2)


@requires_stim
def test_preparation_distance_uses_the_prepared_logical_state(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = rep3_qodec.layers[0].gadgets["prepare_z"]
    profile = GadgetProfile(gadget)
    assert profile.objective is not None
    assert profile.action.is_equivalent_to(profile.objective)
    last = len(gadget.circuit.calls()) - 1
    harmless = FaultEvent.after(last, Pauli("Z_0"))
    harmful = FaultEvent.after(last, Pauli("X_0 X_1 X_2"))
    assert profile.distance(faults=[harmless]).lower_bound is None
    assert profile.distance_bounds(faults=[harmless]).lower_bound is None
    distance = profile.distance(faults=[harmful])
    assert distance == 1
    assert distance.witness.factors == (harmful,)
    distance = profile.distance_bounds(faults=[harmful])
    assert distance == 1
    assert distance.witness.factors == (harmful,)
    distance = profile.distance()
    witness = distance.witness.factors
    bounds = profile.distance_bounds(solver=EnumerationSolverOptions())
    lower, upper = bounds.lower_bound, bounds.upper_bound
    bounded = bounds.witness.factors
    assert distance == lower == upper == 3
    assert len(witness) == len(bounded) == 3


@pytest.mark.parametrize("bare", [False, True])
@pytest.mark.parametrize("bounded", [False, True])
def test_distance_propagates_faults_once_without_direct_readout_solves(
    rep3_qodec: qc.Qodec, bare: bool, bounded: bool
) -> None:
    gadget = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 3)
    profile = GadgetProfile(gadget.circuit if bare else gadget)
    with (
        patch(
            "qdk.ec._faults.propagate_faults", wraps=propagate_faults
        ) as gadget_propagation,
        patch(
            "qdk.ec._profile.propagate_faults", wraps=propagate_faults
        ) as circuit_propagation,
        patch("qdk.ec._faults.BitMatrix", wraps=BitMatrix) as readout_matrix,
    ):
        if bounded:
            profile.distance_bounds(solver=EnumerationSolverOptions())
        else:
            profile.distance()
    assert gadget_propagation.call_count + circuit_propagation.call_count == 1
    readout_matrix.zeros.assert_not_called()


def test_readout_dependencies_are_reduced_once_for_all_faults(
    rep3_qodec: qc.Qodec,
) -> None:
    base = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 1)
    gadget = qc.Gadget(
        qc.Instruction(
            "measure",
            inputs=list(base.implements.inputs),
            flags=["reject"],
            action=list(base.implements.action),
        ),
        base.circuit,
        inputs=base.inputs,
        readouts=[["readouts[1]"], ["circuit.readouts[0]"]],
    )
    profile = GadgetProfile(gadget)
    faults = [
        FaultEvent.after(0, Pauli({0: character})) for character in ("X", "Y", "Z")
    ]
    with patch("qdk.ec._faults.BitMatrix", wraps=BitMatrix) as readout_matrix:
        effects = profile.effects_of(faults)
    readout_matrix.zeros.assert_called_once_with(2, 2 + len(faults))
    assert effects == (
        FaultEffect(["readouts[0:2]"]),
        FaultEffect(["readouts[0:2]"]),
        FaultEffect(),
    )
    assert profile.distance(faults=faults).value is None


@requires_stim
@pytest.mark.parametrize("preserves_input", [False, True])
def test_action_signs_combine_measurement_and_output_faults(
    rep3_qodec: qc.Qodec, preserves_input: bool
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    qubit = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction(
            "H",
            inputs=[qubit],
            outputs=[qubit],
            action=[qc.actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
        ),
    ]
    code = qc.Code("qubit", [], ["X_0"], ["Z_0"])
    if preserves_input:
        source = "R 1 2\nH 1\nCX 1 2\nCX 0 1\nH 0\nM 0\nR 3\nM 1"
        measured_qubit, output_qubit = 1, 2
    else:
        source = "R 0 1\nH 0\nCX 0 1\nM 0"
        measured_qubit, output_qubit = 0, 1
    gadget = qc.Gadget(
        qc.Instruction(
            "transfer" if preserves_input else "prepare",
            inputs=[qubit] if preserves_input else [],
            outputs=[qubit],
        ),
        qc.gadgets.Circuit(physical, source, format="stim"),
        inputs=[qc.gadgets.Encoding(code, support=["0"])] if preserves_input else [],
        outputs=[qc.gadgets.Encoding(code, support=[str(output_qubit)])],
    )
    profile = GadgetProfile(gadget)
    before_measurement = (
        next(
            index
            for index, call in enumerate(gadget.circuit.calls())
            if call.mnemonic == "M"
        )
        - 1
    )
    record_fault = FaultEvent.after(before_measurement, Pauli({measured_qubit: "X"}))
    measurement_call = next(
        index
        for index, call in enumerate(gadget.circuit.calls())
        if call.mnemonic == "M" and call.operands == [measured_qubit]
    )
    readout_fault = FaultEvent.after(measurement_call, readout_flips=0)
    output_fault = FaultEvent.after(before_measurement, Pauli({output_qubit: "X"}))
    assert not gadget.readouts and not gadget.checks
    for fault in (record_fault, readout_fault, output_fault):
        distance = profile.distance(faults=[fault])
        assert distance == 1
        assert distance.witness.factors == (fault,)
        distance = profile.distance_bounds(faults=[fault])
        assert distance == 1
        assert distance.witness.factors == (fault,)
    combined = record_fault * output_fault
    assert profile.distance(faults=[combined]).lower_bound is None
    assert profile.distance_bounds(faults=[combined]).lower_bound is None
    combined_readout = readout_fault * output_fault
    assert profile.distance(faults=[combined_readout]).lower_bound is None
    assert profile.distance_bounds(faults=[combined_readout]).lower_bound is None


def test_action_probes_follow_output_encoding_order(rep3_qodec: qc.Qodec) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    qubit = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction("idle", inputs=[qubit], outputs=[qubit]),
    ]
    code = qc.Code("qubit", [], ["X_0"], ["Z_0"])
    gadget = qc.Gadget(
        qc.Instruction("prepare_and_preserve", inputs=[qubit], outputs=[qubit, qubit]),
        qc.gadgets.Circuit(physical, "- idle: [2]\n- R: [5]", format="yaml"),
        inputs=[qc.gadgets.Encoding(code, support=["2"])],
        outputs=[
            qc.gadgets.Encoding(code, support=["5"]),
            qc.gadgets.Encoding(code, support=["2"]),
        ],
    )
    profile = GadgetProfile(gadget)
    harmless = FaultEvent.after(1, Pauli("Z_5"))
    harmful = FaultEvent.after(1, Pauli("Z_2"))
    assert profile.distance(faults=[harmless]).lower_bound is None
    assert profile.distance_bounds(faults=[harmless]).lower_bound is None
    distance = profile.distance(faults=[harmful])
    assert distance == 1
    assert distance.witness.factors == (harmful,)
    distance = profile.distance_bounds(faults=[harmful])
    assert distance == 1
    assert distance.witness.factors == (harmful,)


def test_profile_keeps_readout_dependencies_in_snapshot(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.readouts = [["readouts[0]", "circuit.readouts[0]"]]
    profile = GadgetProfile(gadget)
    assert isinstance(profile._target, qc.Gadget)
    assert profile._target.readouts == gadget.readouts


@requires_stim
def test_fault_measurement_rows_allow_interleaved_resets(rep3_qodec: qc.Qodec) -> None:
    circuit = qc.gadgets.Circuit(
        rep3_qodec.layers[1].instruction_set,
        "R 0\nM 0\nR 1\nM 1",
        format="stim",
    )
    gadget = qc.Gadget(
        qc.Instruction("measure", action=[qc.actions.Observe(["Z_0", "Z_1"])]),
        circuit,
        readouts=[["circuit.readouts[0]"], ["circuit.readouts[1]"]],
    )
    (effect,) = GadgetProfile(gadget).effects_of([FaultEvent.after(0, Pauli("X_0"))])
    assert effect == FaultEffect(["readouts[0]"])


def test_default_circuit_faults_cover_each_call_support() -> None:
    operand = qc.instructions.BlockOperand("qubit")
    instruction_set = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[
            qc.Instruction("one", inputs=[operand], outputs=[operand]),
            qc.Instruction(
                "two", inputs=[operand, operand], outputs=[operand, operand]
            ),
            qc.Instruction("tick"),
        ],
    )
    circuit = qc.gadgets.Circuit(
        instruction_set, "- one: [2]\n- two: [2, 5]\n- tick: []\n", format="yaml"
    )
    faults = GadgetProfile(circuit)._circuit_faults()
    assert len(faults) == 18
    assert set(faults[:3]) == {
        FaultEvent.after(0, Pauli({2: character})) for character in ("X", "Y", "Z")
    }
    assert set(faults[3:]) == {
        FaultEvent.after(
            1,
            Pauli(
                {
                    qubit: character
                    for qubit, character in zip((2, 5), characters)
                    if character != "I"
                }
            ),
        )
        for characters in product(("I", "X", "Y", "Z"), repeat=2)
        if characters != ("I", "I")
    }


def test_circuit_faults_use_qubit_support_not_operand_count() -> None:
    operand = qc.instructions.BlockOperand("pair")
    instruction_set = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("pair", encodes=2)],
        instructions=[qc.Instruction("idle", inputs=[operand], outputs=[operand])],
    )
    profile = GadgetProfile(
        qc.gadgets.Circuit(instruction_set, "- idle: [data]\n", format="yaml")
    )
    faults = profile._circuit_faults()
    assert len(faults) == 15
    assert set(faults) == {
        FaultEvent.after(
            0,
            Pauli(
                {
                    qubit: character
                    for qubit, character in enumerate(characters)
                    if character != "I"
                }
            ),
        )
        for characters in product(("I", "X", "Y", "Z"), repeat=2)
        if characters != ("I", "I")
    }


def test_default_correlated_fault_is_cheaper_than_single_qubit_events(
    rep3_qodec: qc.Qodec,
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    operand = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction("pair", inputs=[operand, operand], outputs=[operand, operand]),
    ]
    gadget = _measurement_gadget(physical, 2)
    gadget.circuit = qc.gadgets.Circuit(
        physical, "- pair: [0, 1]\n- M: [0]\n- M: [1]\n", format="yaml"
    )
    profile = GadgetProfile(gadget)
    single_qubit = [
        FaultEvent.after(0, Pauli({qubit: basis}))
        for qubit in (0, 1)
        for basis in ("X", "Y", "Z")
    ]
    distance = profile.distance()
    witness = distance.witness.factors
    assert distance == 1 and len(witness) == 1
    assert witness[0].weight == 2
    assert witness[0] in {
        FaultEvent.after(0, Pauli({0: control, 1: target}))
        for control, target in product(("X", "Y"), repeat=2)
    }
    assert profile.distance(faults=single_qubit).value == 2
    assert profile.distance_bounds() == 1
    assert profile.distance_bounds(faults=single_qubit) == 2
    (effect,) = profile.effects_of(witness)
    assert effect == FaultEffect(["readouts[0]"])


def test_profile_distance_counts_correlated_gate_error_once(
    rep3_qodec: qc.Qodec,
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    code = qc.Code("pair", ["Z_0 Z_1"], ["X_0 X_1"], ["Z_0"])
    operand = qc.instructions.BlockOperand("pair")
    qubit = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction("pair", inputs=[qubit, qubit], outputs=[qubit, qubit]),
    ]
    gadget = qc.Gadget(
        qc.Instruction("idle", inputs=[operand], outputs=[operand]),
        qc.gadgets.Circuit(physical, "- pair: [0, 1]", format="yaml"),
        inputs=[qc.gadgets.Encoding(code, support=["0", "1"])],
        outputs=[qc.gadgets.Encoding(code, support=["0", "1"])],
    )
    profile = GadgetProfile(gadget)
    fault = FaultEvent.after(0, Pauli("X_0 X_1"))
    distance = profile.distance(faults=[fault])
    assert distance == 1
    assert distance.witness.factors == (fault,)
    distance = profile.distance_bounds(faults=[fault])
    assert distance == 1
    assert distance.witness.factors == (fault,)
    assert fault.weight == 2
    distance = profile.distance()
    witness = distance.witness.factors
    bounds = profile.distance_bounds()
    lower, upper = bounds.lower_bound, bounds.upper_bound
    bounded = bounds.witness.factors
    assert distance == lower == upper == 1
    assert len(witness) == len(bounded) == 1


def test_distance_finds_combinations_and_returns_replayable_faults(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 2)
    faults = [FaultEvent.after(0, Pauli("X_0")), FaultEvent.after(1, Pauli("X_1"))]
    profile = GadgetProfile(gadget)
    distance = profile.distance(faults=faults)
    witness = distance.witness.factors
    assert distance == 2 and witness == tuple(faults)
    distance = profile.distance_bounds(faults=faults)
    assert distance == 2
    assert distance.witness.factors == tuple(faults)
    distance = profile.distance_bounds(faults=faults, solver=EnumerationSolverOptions())
    assert distance == 2
    assert distance.witness.factors == tuple(faults)
    (combined,) = profile.effects_of([witness[0] * witness[1]])
    assert combined == FaultEffect(["readouts[0]"])
    event = witness[0] * witness[1]
    assert event == FaultEvent({0: Pauli("X_0"), 1: Pauli("X_1")})
    distance = profile.distance(faults=[event])
    assert distance == 1
    assert distance.witness.factors == (event,)
    distance = profile.distance_bounds(faults=[event])
    assert distance == 1
    assert distance.witness.factors == (event,)
    assert profile.distance(faults=[]).lower_bound is None
    assert profile.distance_bounds(faults=[]).lower_bound is None


@requires_stim
def test_flag_alone_is_not_a_logical_failure(
    rep3_qodec: qc.Qodec,
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    fault = FaultEvent.after(0, Pauli("X_0"))
    circuit = qc.gadgets.Circuit(physical, "R 0\nM 0", format="stim")
    flag_only = qc.Gadget(
        qc.Instruction("flag", flags=["reject"]),
        circuit,
        readouts=[{"reject": ["circuit.readouts[0]"]}],
    )
    assert GadgetProfile(flag_only).distance(faults=[fault]).lower_bound is None
    assert GadgetProfile(flag_only).distance_bounds(faults=[fault]).lower_bound is None


def _flagged_measurement_gadget(physical: qc.InstructionSet) -> qc.Gadget:
    base = _measurement_gadget(physical, 1)
    return qc.Gadget(
        qc.Instruction(
            "measure",
            inputs=list(base.implements.inputs),
            flags=["reject_first", "reject_second"],
            action=[qc.actions.Observe(["Z_0"])],
        ),
        qc.gadgets.Circuit(
            physical,
            "- R: [1]\n- R: [2]\n- idle: [0]\n- M: [0]\n- M: [1]\n- M: [2]",
            format="yaml",
        ),
        inputs=base.inputs,
        readouts=[
            ["circuit.readouts[0]", "in[0].z[0]"],
            {"reject_first": ["circuit.readouts[1]"]},
            {"reject_second": ["circuit.readouts[2]"]},
        ],
    )


@pytest.mark.parametrize("method", ["distance", "distance_bounds"])
def test_distance_requires_each_flag_zero_in_the_combined_fault(
    rep3_qodec: qc.Qodec, method: str
) -> None:
    gadget = _flagged_measurement_gadget(rep3_qodec.layers[1].instruction_set)
    profile = GadgetProfile(gadget)
    search = getattr(profile, method)
    logical_fault = FaultEvent.after(2, Pauli("X_0 X_1 X_2"))
    first_mask = FaultEvent.after(4, readout_flips=0)
    second_mask = FaultEvent.after(5, readout_flips=0)
    faults = [logical_fault, first_mask, second_mask]
    (effect,) = profile.effects_of([logical_fault])
    assert effect == FaultEffect(["readouts[0:3]"])
    for incomplete in ([logical_fault], faults[:2], [logical_fault, second_mask]):
        assert search(faults=incomplete, solver="enumeration").lower_bound is None

    distance = search(faults=faults, solver="enumeration")

    assert distance == 3
    assert distance.witness.factors == tuple(faults)
    for witness in distance.witnesses:
        (combined,) = profile.effects_of([witness.product])
        assert combined == FaultEffect(["readouts[0]"])
    assert not gadget.checks
    assert search(faults=[distance.witness.product], solver="enumeration") == 1


@pytest.mark.parametrize("method", ["distance", "distance_bounds"])
def test_distance_allows_two_faults_to_cancel_flags(
    rep3_qodec: qc.Qodec, method: str
) -> None:
    gadget = _flagged_measurement_gadget(rep3_qodec.layers[1].instruction_set)
    profile = GadgetProfile(gadget)
    faults = [
        FaultEvent.after(2, Pauli("X_0 X_1 X_2")),
        FaultEvent.after(2, Pauli("X_1 X_2")),
    ]
    assert all(
        "readouts[1]" in effect and "readouts[2]" in effect
        for effect in profile.effects_of(faults)
    )

    distance = getattr(profile, method)(faults=faults, solver="enumeration")

    assert distance == 2
    (combined,) = profile.effects_of([distance.witness.product])
    assert combined == FaultEffect(["readouts[0]"])


@pytest.mark.parametrize("method", ["distance", "distance_bounds"])
@pytest.mark.parametrize("bound_flags", [0, 1])
def test_distance_requires_all_flag_equations_even_without_faults(
    rep3_qodec: qc.Qodec, method: str, bound_flags: int
) -> None:
    gadget = _flagged_measurement_gadget(rep3_qodec.layers[1].instruction_set)
    gadget.readouts = [
        list(readout.equation) for readout in gadget.readouts[: 1 + bound_flags]
    ]

    with pytest.raises(ValueError, match="every flag readout to be bound"):
        getattr(GadgetProfile(gadget), method)(faults=[], solver="enumeration")


@requires_stim
def test_output_codespace_is_required_without_adding_declared_checks(
    rep3_qodec: qc.Qodec,
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    code = qc.Code("pair", ["Z_0 Z_1"], ["X_0 X_1"], ["Z_0"])
    encoding = qc.gadgets.Encoding(code, support=["0", "1"])
    gadget = qc.Gadget(
        qc.Instruction("idle", outputs=[qc.instructions.BlockOperand("pair")]),
        qc.gadgets.Circuit(physical, "R 0 1", format="stim"),
        outputs=[encoding],
    )
    fault = FaultEvent.after(1, Pauli("X_0"))
    profile = GadgetProfile(gadget)
    (effect,) = profile.effects_of([fault])
    assert effect == FaultEffect(["out[0].z[0]", "out[0].stabilizers[0]"])
    assert gadget.resolve("out[0].code.z[0]").value(str) == "Z_0"
    for reference in effect:
        gadget.resolve(reference)
    assert profile.distance(faults=[fault]).lower_bound is None
    assert profile.distance_bounds(faults=[fault]).lower_bound is None
    gadget.checks = [["out[0].stabilizers[0]"]]
    profile = GadgetProfile(gadget)
    (effect,) = profile.effects_of([fault])
    assert effect == FaultEffect(["checks[0]", "out[0].z[0]", "out[0].stabilizers[0]"])
    assert gadget.resolve(qc.Reference("checks[0]")).value(tuple) == gadget.checks[0]
    with pytest.raises(ValueError):
        gadget.checks = [[qc.Reference("checks[0]")]]
    assert profile.distance(faults=[fault]).lower_bound is None
    assert profile.distance_bounds(faults=[fault]).lower_bound is None
    gadget.checks = [["out[0].stabilizers[0:1]", "out[0].stabilizers[00]"]]
    assert GadgetProfile(gadget).distance(faults=[fault]).lower_bound is None


def test_readout_free_distance_requires_combined_logical_residual() -> None:
    operand = qc.instructions.BlockOperand("qubit")
    physical = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[qc.Instruction("idle", inputs=[operand], outputs=[operand])],
    )
    code = qc.Code(
        "C4",
        ["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
        ["X_0 X_1", "X_0 X_2"],
        ["Z_0 Z_2", "Z_0 Z_1"],
    )
    encoding = qc.gadgets.Encoding(code, support=["0", "1", "2", "3"])
    logical = qc.instructions.BlockOperand("C4")
    gadget = qc.Gadget(
        qc.Instruction("idle", inputs=[logical], outputs=[logical]),
        qc.gadgets.Circuit(
            physical,
            "- idle: [0]\n- idle: [1]\n- idle: [2]\n- idle: [3]\n",
            format="yaml",
        ),
        inputs=[encoding],
        outputs=[encoding],
    )
    profile = GadgetProfile(gadget)
    faults = [FaultEvent.after(index, Pauli({index: "X"})) for index in (0, 1)]
    assert not gadget.readouts and not gadget.checks
    assert all(
        not _flipped_indices(effect, "checks") for effect in profile.effects_of(faults)
    )
    for fault in faults:
        assert profile.distance(faults=[fault]).lower_bound is None
        assert profile.distance_bounds(faults=[fault]).lower_bound is None
    distance = profile.distance(faults=faults)
    witness = distance.witness.factors
    bounds = profile.distance_bounds(faults=faults)
    lower, upper = bounds.lower_bound, bounds.upper_bound
    bounded = bounds.witness.factors
    assert distance == lower == upper == 2
    assert witness == bounded == tuple(faults)
    residual = reduce(
        mul,
        (Pauli({faults.index(fault): "X"}) for fault in witness),
        Pauli.identity(),
    )
    assert CodeProfile(code).is_logical(residual)
    logical_fault = FaultEvent.after(0, Pauli("X_0 X_1"))
    distance = profile.distance(faults=[logical_fault])
    assert distance == 1
    assert distance.witness.factors == (logical_fault,)
    stabilizer_fault = FaultEvent.after(0, Pauli("X_0 X_1 X_2 X_3"))
    assert profile.distance(faults=[stabilizer_fault]).lower_bound is None
    assert profile.distance_bounds(faults=[stabilizer_fault]).lower_bound is None
    assert profile.distance().value == 2


@requires_stim
def test_output_syndromes_on_different_blocks_do_not_cancel() -> None:
    gadget = c4().layers[0].gadgets["transversal_cx"]
    gadget.checks = []
    last = len(gadget.circuit.calls()) - 1
    faults = [FaultEvent.after(last, Pauli({qubit: "X"})) for qubit in (0, 4)]
    profile = GadgetProfile(gadget)
    effects = profile.effects_of(faults)
    assert all(not _flipped_indices(effect, "checks") for effect in effects)
    assert "out[0].z[0]" in effects[0] and "out[1].z[0]" in effects[1]
    assert "out[0].stabilizers[1]" in effects[0]
    assert "out[1].stabilizers[1]" in effects[1]
    assert profile.distance(faults=faults).lower_bound is None
    assert profile.distance_bounds(faults=faults).lower_bound is None


@requires_stim
def test_declared_checks_still_exclude_a_codespace_preserving_error() -> None:
    gadget = c4().layers[0].gadgets["idle"]
    last = len(gadget.circuit.calls()) - 1
    fault = FaultEvent.after(last, Pauli("X_0 X_1"))
    profile = GadgetProfile(gadget)
    distance = profile.distance(faults=[fault])
    assert distance == 1
    assert distance.witness.factors == (fault,)
    gadget.checks = [*gadget.checks, ["out[0].z[0]", "in[0].z[0]"]]
    profile = GadgetProfile(gadget)
    (effect,) = profile.effects_of([fault])
    assert _flipped_indices(effect, "checks") == {len(gadget.checks) - 1}
    assert profile.distance(faults=[fault]).lower_bound is None
    assert profile.distance_bounds(faults=[fault]).lower_bound is None


def test_readout_dependencies_are_solved_for_fault_effects(
    rep3_qodec: qc.Qodec,
) -> None:
    base = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 1)
    gadget = qc.Gadget(
        qc.Instruction(
            "measure",
            inputs=list(base.implements.inputs),
            flags=["reject"],
            action=[qc.actions.Observe(["Z_0"])],
        ),
        base.circuit,
        inputs=base.inputs,
        readouts=[["readouts[1]"], ["circuit.readouts[0]"]],
    )
    fault = FaultEvent.after(0, Pauli("X_0"))
    (effect,) = GadgetProfile(gadget).effects_of([fault])
    assert effect == FaultEffect(["readouts[0:2]"])
    distance = GadgetProfile(gadget).distance(faults=[fault])
    assert distance.value is None
    assert GadgetProfile(gadget).distance_bounds(faults=[fault]).value is None
    gadget.readouts = [["readouts[0]"], []]
    for method in ("distance", "distance_bounds"):
        with pytest.raises(ValueError, match="uniquely determine"):
            getattr(GadgetProfile(gadget), method)(faults=[fault])
    gadget.readouts = []
    with pytest.raises(ValueError, match="every logical measurement"):
        GadgetProfile(gadget).distance(faults=[fault])


@requires_stim
def test_identical_logical_effects_cancel_instead_of_forming_a_failure(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = qc.Gadget(
        qc.Instruction("measure", action=[qc.actions.Observe(["Z_0"])]),
        qc.gadgets.Circuit(
            rep3_qodec.layers[1].instruction_set, "R 0 1\nM 0 1", format="stim"
        ),
        checks=[["circuit.readouts[0]", "circuit.readouts[1]"]],
        readouts=[["circuit.readouts[0]", "circuit.readouts[1]"]],
    )
    faults = [FaultEvent.after(0, Pauli("X_0")), FaultEvent.after(1, Pauli("X_1"))]
    profile = GadgetProfile(gadget)
    assert profile.distance(faults=faults).lower_bound is None
    assert profile.distance_bounds(faults=faults).lower_bound is None
    (effect,) = profile.effects_of([faults[0] * faults[1]])
    assert not effect


@requires_stim
@pytest.mark.parametrize(
    "reference", ["circuit.readouts[9]", "readouts[9]", "in[0].z[0]"]
)
def test_distance_rejects_invalid_check_references(
    rep3_qodec: qc.Qodec, reference: str
) -> None:
    gadget = qc.Gadget(
        qc.Instruction("measure", action=[qc.actions.Observe(["Z_0"])]),
        qc.gadgets.Circuit(
            rep3_qodec.layers[1].instruction_set, "R 0\nM 0", format="stim"
        ),
        checks=[[reference]],
        readouts=[["circuit.readouts[0]"]],
    )
    for method in (
        GadgetProfile(gadget).distance,
        GadgetProfile(gadget).distance_bounds,
    ):
        with pytest.raises(ValueError, match="out of bounds"):
            method(faults=[FaultEvent.after(0, Pauli("X_0"))])


@requires_highs
@pytest.mark.parametrize("method", ["distance", "distance_bounds"])
def test_gadget_profile_defaults_to_highs(rep3_qodec: qc.Qodec, method: str) -> None:
    profile = GadgetProfile(
        _measurement_gadget(rep3_qodec.layers[1].instruction_set, 3)
    )
    with patch(
        "qdk.ec._analysis.distance_solvers.import_module", wraps=import_module
    ) as backend:
        distance = getattr(profile, method)()
    assert distance == 3
    (effect,) = profile.effects_of([distance.witness.product])
    assert effect == FaultEffect(["readouts[0]"])
    backend.assert_called_once_with("highspy")


@requires_highs
def test_distance_three_matches_bounds_and_cutoff(rep3_qodec: qc.Qodec) -> None:
    gadget = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 3)
    faults = [FaultEvent.after(index, Pauli({index: "X"})) for index in range(3)]
    profile = GadgetProfile(gadget)
    for selected in (None, faults):
        distance = profile.distance(faults=selected)
        witness = distance.witness.factors
        bounds = profile.distance_bounds(faults=selected)
        lower, upper = bounds.lower_bound, bounds.upper_bound
        bounded = bounds.witness.factors
        assert distance == upper == 3 and lower <= 3
        assert len(witness) == len(bounded) == 3
        for factors in (witness, bounded):
            combined = reduce(mul, factors, FaultEvent({}))
            (effect,) = profile.effects_of([combined])
            assert effect == FaultEffect(["readouts[0]"])
        with pytest.raises(RuntimeError, match="exact distance"):
            profile.distance(faults=selected, upper_bound=2)
    distance = profile.distance_bounds(faults=faults, solver=EnumerationSolverOptions())
    assert distance == 3
    assert distance.witness.factors == tuple(faults)


@requires_stim
def test_bare_circuit_distance_uses_discovered_checks(rep3_qodec: qc.Qodec) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    operand = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction("idle", inputs=[operand], outputs=[operand]),
    ]
    circuit = qc.gadgets.Circuit(physical, "- idle: [5]\n", format="yaml")
    profile = GadgetProfile(circuit)
    distance = profile.distance()
    witness = distance.witness.factors
    assert distance == 1 and len(witness) == 1
    (effect,) = profile.effects_of(witness)
    assert profile._circuit_outputs == tuple(range(6))
    assert effect and all(reference.path.startswith("out[5].") for reference in effect)
    measured = GadgetProfile(qc.gadgets.Circuit(physical, "R 0\nM 0", format="stim"))
    assert measured.checks
    assert measured.distance().lower_bound is None
    assert measured.distance_bounds().lower_bound is None


@requires_stim
@pytest.mark.parametrize("bare", [False, True])
def test_fault_effect_xor_matches_replay_and_uses_snapshot(
    idle_gadget: qc.Gadget, bare: bool
) -> None:
    gadget = deepcopy(idle_gadget)
    profile = GadgetProfile(gadget.circuit if bare else gadget)
    last = len(gadget.circuit.calls()) - 1
    faults = [
        FaultEvent(),
        FaultEvent.after(0, Pauli("X_0")),
        FaultEvent.after(last, Pauli("Z_1")),
        FaultEvent.after(0, Pauli("Y_0")),
    ]
    effects = profile.effects_of(faults)
    pairs = list(product(range(len(faults)), repeat=2))
    combined = profile.effects_of(
        [faults[left] * faults[right] for left, right in pairs]
    )
    assert combined == tuple(effects[left] ^ effects[right] for left, right in pairs)
    if not bare:
        for effect in effects:
            for reference in effect:
                gadget.resolve(reference)
    gadget.circuit.source = "invalid edited source"
    assert profile.effects_of(faults) == effects


def test_bare_circuit_effect_output_positions_skip_prepared_slots(
    rep3_qodec: qc.Qodec,
) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    _measurement_gadget(physical, 1)
    circuit = qc.gadgets.Circuit(physical, "- R: [0]\n- idle: [5]", format="yaml")
    profile = GadgetProfile(circuit)
    assert profile._circuit_outputs == (1, 2, 3, 4, 5)
    effects = profile.effects_of(
        [
            FaultEvent.after(1, Pauli("X_5")),
            FaultEvent.after(1, Pauli("Z_5")),
            FaultEvent.after(1, Pauli("Y_5")),
        ]
    )
    assert effects == (
        FaultEffect(["out[4].z[0]"]),
        FaultEffect(["out[4].x[0]"]),
        FaultEffect(["out[4].x[0]", "out[4].z[0]"]),
    )


@requires_stim
@pytest.mark.parametrize("location", [-1, 2])
def test_invalid_fault_location_raises(rep3_qodec: qc.Qodec, location: int) -> None:
    profile = GadgetProfile(
        qc.gadgets.Circuit(
            rep3_qodec.layers[1].instruction_set, "R 0\nM 0", format="stim"
        )
    )
    for method in (profile.distance, profile.distance_bounds):
        with pytest.raises(ValueError, match="fault call index"):
            method(faults=[FaultEvent.after(location, Pauli("X_0"))])


def test_conditional_circuit_rejected_instead_of_ignored() -> None:
    operand = qc.instructions.BlockOperand("qubit")
    instruction = qc.Instruction(
        "conditional",
        inputs=[operand],
        outputs=[operand],
        parameters=[qc.instructions.Parameter("enabled", "bit")],
        action=[qc.actions.Pauli("X_0", condition=qc.actions.Condition(["enabled"]))],
    )
    physical = qc.InstructionSet(
        "physical",
        blocks=[qc.instructions.Block("qubit", encodes=1)],
        instructions=[instruction],
    )
    profile = GadgetProfile(
        qc.gadgets.Circuit(physical, "- conditional: [0, enabled: 1]\n", format="yaml")
    )
    for method in (profile.distance, profile.distance_bounds):
        with pytest.raises(NotImplementedError, match="conditional or selected"):
            method()
