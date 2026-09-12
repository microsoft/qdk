"""Distance counts allowed circuit faults, not single-qubit Pauli factors."""

from __future__ import annotations

from itertools import product
from functools import reduce
from operator import mul
from unittest.mock import patch

from binar import BitMatrix
import qodec as qc
import pytest

from qdk.ec import ChannelAction, FaultEvent, GadgetProfile, Pauli, SubsystemCode
from qdk.ec._analysis.distance_solvers import EnumerationSolverOptions
from qdk.ec._analysis.propagation.frames import FrameGroup, PauliFrame
from qdk.ec._analysis.propagation.interpreter import propagate_faults
from qdk.ec._profile import _fault_observables
from ec_tests.testing.optional import requires_highs, requires_mwpf
from ec_tests.testing.qodecs import c4


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


def test_measurement_only_distance_includes_readout_flips(rep3_qodec: qc.Qodec) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    gadget = _measurement_gadget(physical, 3)
    gadget.circuit = qc.gadgets.Circuit(physical, "M 0 1 2", format="stim")
    profile = GadgetProfile(gadget)

    distance, witness = profile.distance()
    assert distance == len(witness) == 3
    lower, upper, bounded = profile.distance_bounds(solver=EnumerationSolverOptions())
    assert lower == upper == len(bounded) == 3
    (effect,) = profile.effects_of([reduce(mul, witness, FaultEvent({}))])
    assert not effect.syndrome
    assert effect.readout_flips == {0}


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
    assert direct_effect.readout_flips == {0}
    assert all(not pauli.weight for pauli in direct_effect.output_error.values())
    assert profile.distance(faults=[readout_fault]) == (1, [readout_fault])
    post_fault = FaultEvent.after(1, Pauli(error))
    (post_effect,) = profile.effects_of([post_fault])
    assert not post_effect.readout_flips
    assert any(pauli.weight for pauli in post_effect.output_error.values())
    assert readout_fault in profile._circuit_faults()
    assert readout_fault * post_fault in profile._circuit_faults()
    assert readout_fault in dict(profile.fault_effects)


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
    assert first.readout_flips == {0}
    assert second.readout_flips == {1}
    assert both.readout_flips == {0, 1}
    assert first.output_error == second.output_error == both.output_error == {}
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
    assert effect.readout_flips == expected
    assert all(not error.weight for error in effect.output_error.values())
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
    distance, witness = profile.distance()
    assert distance == len(witness) == 1
    assert profile.distance(faults=[fault]) == (1, [fault])
    (effect,) = profile.effects_of(witness)
    assert not effect.syndrome and effect.readout_flips == {0}


def test_c4_z_measurement_has_distance_two_with_readout_noise() -> None:
    gadget = c4().layers[0].gadgets["measure_zz"]
    gadget.readouts = [
        [*readout.equation, f"in[0].z[{index}]"]
        for index, readout in enumerate(gadget.readouts)
    ]
    profile = GadgetProfile(gadget)
    distance, witness = profile.distance()
    assert distance == len(witness) == 2
    (effect,) = profile.effects_of([reduce(mul, witness, FaultEvent())])
    assert not effect.syndrome and effect.readout_flips


@requires_highs
def test_highs_gadget_distance_returns_replayable_witness(rep3_qodec: qc.Qodec) -> None:
    gadget = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 3)
    profile = GadgetProfile(gadget)
    distance, witness = profile.distance(solver="highs")
    lower, upper, bounded = profile.distance_bounds(solver="highs")
    assert distance == lower == upper == 3
    for factors in (witness, bounded):
        assert len(factors) == 3
        (effect,) = profile.effects_of([reduce(mul, factors, FaultEvent({}))])
        assert not effect.syndrome and effect.readout_flips == {0}
    with pytest.raises(RuntimeError, match="exact distance"):
        profile.distance(solver="highs", upper_bound=2)


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
    assert profile.distance(faults=[harmless])[1] == []
    assert profile.distance_bounds(faults=[harmless])[2] == []
    assert profile.distance(faults=[harmful]) == (1, [harmful])
    assert profile.distance_bounds(faults=[harmful]) == (1, 1, [harmful])
    distance, witness = profile.distance()
    lower, upper, bounded = profile.distance_bounds(solver=EnumerationSolverOptions())
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
    assert [effect.readout_flips for effect in effects] == [{0, 1}, {0, 1}, set()]
    assert profile.distance(faults=faults)[0] == 1


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
        assert profile.distance(faults=[fault]) == (1, [fault])
        assert profile.distance_bounds(faults=[fault]) == (1, 1, [fault])
    combined = record_fault * output_fault
    assert profile.distance(faults=[combined])[1] == []
    assert profile.distance_bounds(faults=[combined])[2] == []
    combined_readout = readout_fault * output_fault
    assert profile.distance(faults=[combined_readout])[1] == []
    assert profile.distance_bounds(faults=[combined_readout])[2] == []


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
    assert profile.distance(faults=[harmless])[1] == []
    assert profile.distance_bounds(faults=[harmless])[2] == []
    assert profile.distance(faults=[harmful]) == (1, [harmful])
    assert profile.distance_bounds(faults=[harmful]) == (1, 1, [harmful])


def test_profile_keeps_readout_dependencies_in_snapshot(rep3_qodec: qc.Qodec) -> None:
    gadget = rep3_qodec.layers[0].gadgets["measure_z"]
    gadget.readouts = [["readouts[0]", "circuit.readouts[0]"]]
    profile = GadgetProfile(gadget)
    assert isinstance(profile._target, qc.Gadget)
    assert profile._target.readouts == gadget.readouts


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
    assert effect.readout_flips == {0}


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
    distance, witness = profile.distance()
    assert distance == 1 and len(witness) == 1
    assert witness[0].weight == 2
    assert witness[0] in {
        FaultEvent.after(0, Pauli({0: control, 1: target}))
        for control, target in product(("X", "Y"), repeat=2)
    }
    assert profile.distance(faults=single_qubit)[0] == 2
    assert profile.distance_bounds()[:2] == (1, 1)
    assert profile.distance_bounds(faults=single_qubit)[:2] == (2, 2)
    (effect,) = profile.effects_of(witness)
    assert not effect.syndrome and effect.readout_flips == {0}


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
    assert profile.distance(faults=[fault]) == (1, [fault])
    assert profile.distance_bounds(faults=[fault]) == (1, 1, [fault])
    assert fault.weight == 2
    distance, witness = profile.distance()
    lower, upper, bounded = profile.distance_bounds()
    assert distance == lower == upper == 1
    assert len(witness) == len(bounded) == 1


def test_distance_finds_combinations_and_returns_replayable_faults(
    rep3_qodec: qc.Qodec,
) -> None:
    gadget = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 2)
    faults = [FaultEvent.after(0, Pauli("X_0")), FaultEvent.after(1, Pauli("X_1"))]
    profile = GadgetProfile(gadget)
    distance, witness = profile.distance(faults=faults)
    assert distance == 2 and witness == faults
    assert profile.distance_bounds(faults=faults) == (2, 2, faults)
    assert profile.distance_bounds(
        faults=faults, solver=EnumerationSolverOptions()
    ) == (
        2,
        2,
        faults,
    )
    (combined,) = profile.effects_of([witness[0] * witness[1]])
    assert not combined.syndrome and combined.readout_flips == {0}
    event = witness[0] * witness[1]
    assert event == FaultEvent({0: Pauli("X_0"), 1: Pauli("X_1")})
    assert profile.distance(faults=[event]) == (1, [event])
    assert profile.distance_bounds(faults=[event]) == (1, 1, [event])
    assert profile.distance(faults=[])[1] == []
    assert profile.distance_bounds(faults=[])[2] == []


def test_flag_alone_is_not_a_logical_failure_or_a_detector(
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
    assert GadgetProfile(flag_only).distance(faults=[fault])[1] == []
    assert GadgetProfile(flag_only).distance_bounds(faults=[fault])[2] == []
    base = _measurement_gadget(physical, 1)
    measurement = qc.Gadget(
        qc.Instruction(
            "measure",
            inputs=list(base.implements.inputs),
            flags=["reject"],
            action=[qc.actions.Observe(["Z_0"])],
        ),
        base.circuit,
        inputs=base.inputs,
        readouts=[["circuit.readouts[0]"], {"reject": ["circuit.readouts[0]"]}],
    )
    assert GadgetProfile(measurement).distance(faults=[fault]) == (1, [fault])
    measurement.checks = [["circuit.readouts[0]"]]
    assert GadgetProfile(measurement).distance(faults=[fault])[1] == []
    measurement.checks = [["readouts[1]"]]
    assert GadgetProfile(measurement).distance(faults=[fault])[1] == []


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
    assert not effect.syndrome and effect.output_error[0].weight
    assert profile.distance(faults=[fault])[1] == []
    assert profile.distance_bounds(faults=[fault])[2] == []
    gadget.checks = [["out[0].stabilizers[0]"]]
    profile = GadgetProfile(gadget)
    (effect,) = profile.effects_of([fault])
    assert effect.syndrome == {0} and effect.output_error[0].weight
    assert profile.distance(faults=[fault])[1] == []
    assert profile.distance_bounds(faults=[fault])[2] == []
    gadget.checks = [["out[0].stabilizers[0:1]", "out[0].stabilizers[00]"]]
    assert GadgetProfile(gadget).distance(faults=[fault])[1] == []


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
    assert all(not effect.syndrome for effect in profile.effects_of(faults))
    for fault in faults:
        assert profile.distance(faults=[fault])[1] == []
        assert profile.distance_bounds(faults=[fault])[2] == []
    distance, witness = profile.distance(faults=faults)
    lower, upper, bounded = profile.distance_bounds(faults=faults)
    assert distance == lower == upper == 2
    assert witness == bounded == faults
    residual = reduce(
        mul,
        (Pauli({faults.index(fault): "X"}) for fault in witness),
        Pauli.identity(),
    )
    assert SubsystemCode.of(code).is_non_trivial_logical_error(residual)
    logical_fault = FaultEvent.after(0, Pauli("X_0 X_1"))
    assert profile.distance(faults=[logical_fault]) == (1, [logical_fault])
    stabilizer_fault = FaultEvent.after(0, Pauli("X_0 X_1 X_2 X_3"))
    assert profile.distance(faults=[stabilizer_fault])[1] == []
    assert profile.distance_bounds(faults=[stabilizer_fault])[2] == []
    assert profile.distance()[0] == 2


def test_output_syndromes_on_different_blocks_do_not_cancel() -> None:
    gadget = c4().layers[0].gadgets["transversal_cx"]
    gadget.checks = []
    last = len(gadget.circuit.calls()) - 1
    faults = [FaultEvent.after(last, Pauli({qubit: "X"})) for qubit in (0, 4)]
    profile = GadgetProfile(gadget)
    effects = profile.effects_of(faults)
    assert all(not effect.syndrome for effect in effects)
    assert effects[0].output_error[0].weight and effects[1].output_error[1].weight
    assert profile.distance(faults=faults)[1] == []
    assert profile.distance_bounds(faults=faults)[2] == []


def test_declared_checks_still_exclude_a_codespace_preserving_error() -> None:
    gadget = c4().layers[0].gadgets["idle"]
    last = len(gadget.circuit.calls()) - 1
    fault = FaultEvent.after(last, Pauli("X_0 X_1"))
    profile = GadgetProfile(gadget)
    assert profile.distance(faults=[fault]) == (1, [fault])
    gadget.checks = [*gadget.checks, ["out[0].z[0]", "in[0].z[0]"]]
    profile = GadgetProfile(gadget)
    (effect,) = profile.effects_of([fault])
    assert effect.syndrome == {len(gadget.checks) - 1}
    assert profile.distance(faults=[fault])[1] == []
    assert profile.distance_bounds(faults=[fault])[2] == []


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
    assert effect.readout_flips == {0, 1}
    assert GadgetProfile(gadget).distance(faults=[fault]) == (1, [fault])
    gadget.readouts = [["readouts[0]"], []]
    for method in ("distance", "distance_bounds"):
        with pytest.raises(ValueError, match="uniquely determine"):
            getattr(GadgetProfile(gadget), method)(faults=[fault])
    gadget.readouts = []
    with pytest.raises(ValueError, match="every logical measurement"):
        GadgetProfile(gadget).distance(faults=[fault])


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
    assert profile.distance(faults=faults)[1] == []
    assert profile.distance_bounds(faults=faults)[2] == []
    (effect,) = profile.effects_of([faults[0] * faults[1]])
    assert not effect.syndrome and not effect.readout_flips


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


@requires_mwpf
def test_distance_three_matches_bounds_and_cutoff(rep3_qodec: qc.Qodec) -> None:
    gadget = _measurement_gadget(rep3_qodec.layers[1].instruction_set, 3)
    faults = [FaultEvent.after(index, Pauli({index: "X"})) for index in range(3)]
    profile = GadgetProfile(gadget)
    for selected in (None, faults):
        distance, witness = profile.distance(faults=selected)
        lower, upper, bounded = profile.distance_bounds(faults=selected)
        assert distance == upper == 3 and lower <= 3
        assert len(witness) == len(bounded) == 3
        for factors in (witness, bounded):
            combined = reduce(mul, factors, FaultEvent({}))
            (effect,) = profile.effects_of([combined])
            assert not effect.syndrome and effect.readout_flips == {0}
        with pytest.raises(RuntimeError, match="exact distance"):
            profile.distance(faults=selected, upper_bound=2)
    assert profile.distance_bounds(
        faults=faults, solver=EnumerationSolverOptions()
    ) == (
        3,
        3,
        faults,
    )


def test_bare_circuit_distance_uses_discovered_checks(rep3_qodec: qc.Qodec) -> None:
    physical = rep3_qodec.layers[1].instruction_set
    operand = qc.instructions.BlockOperand("qubit")
    physical.instructions = [
        *physical.instructions.values(),
        qc.Instruction("idle", inputs=[operand], outputs=[operand]),
    ]
    circuit = qc.gadgets.Circuit(physical, "- idle: [5]\n", format="yaml")
    profile = GadgetProfile(circuit)
    distance, witness = profile.distance()
    assert distance == 1 and len(witness) == 1
    (effect,) = profile.effects_of(witness)
    assert not effect.syndrome and any(
        error.weight for error in effect.output_error.values()
    )
    measured = GadgetProfile(qc.gadgets.Circuit(physical, "R 0\nM 0", format="stim"))
    assert measured.checks
    assert measured.distance()[1] == []
    assert measured.distance_bounds()[2] == []


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
