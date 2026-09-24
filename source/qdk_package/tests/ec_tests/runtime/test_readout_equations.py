from . import FIXTURES
from collections.abc import Hashable
from itertools import product

import pytest
from qodec import Reference


def test_prepared_terminal_readout_table_matches_all_repetition_measurements():
    from qodec import Qodec
    from qdk.simulation._qodec.decoding import SyndromeModel

    layer = Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    model = SyndromeModel(layer)
    table = model.readout_table(layer.gadgets["measure_z"], 3)
    assert table == tuple((pattern.bit_count() >= 2,) for pattern in range(8))
    assert model.readout_table(layer.gadgets["measure_z"], 3) is table
    assert model.readout_table(layer.gadgets["idle"], 2) is None
    assert model.readout_table(layer.gadgets["measure_z"], 11) is None


def test_readout_table_rejects_dependence_on_an_incoming_boundary():
    from qodec import Qodec
    from qdk.simulation._qodec.decoding import SyndromeModel

    layer = Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    gadget = layer.gadgets["measure_z"]
    gadget.checks = []
    assert SyndromeModel(layer).readout_table(gadget, 3) is None


@pytest.mark.parametrize(
    "text, expected",
    [
        ("circuit.readouts[2]", ("circuit_readout", None, None, None, 2)),
        ("readouts[3]", ("readout", None, None, None, 3)),
        ("in[0].stabilizers[1]", ("encoding", "in", 0, "stabilizers", 1)),
        ("out[1].stabilizers[2]", ("encoding", "out", 1, "stabilizers", 2)),
        ("in[1].x[2]", ("encoding", "in", 1, "x", 2)),
        ("out[2].z[3]", ("encoding", "out", 2, "z", 3)),
    ],
)
def test_reference_keys_preserve_decoder_coordinates(text, expected):
    from qdk.simulation._qodec.readout_equations import reference_key

    assert reference_key(Reference(text)) == expected


@pytest.mark.parametrize(
    "text",
    ["circuit.readouts[0:2]", "readouts[0,1]", "codes[0]", "out[0].support[0]"],
)
def test_reference_keys_require_one_supported_parity_term(text):
    from qdk.simulation._qodec.readout_equations import reference_key

    with pytest.raises(ValueError, match="one parity term"):
        reference_key(Reference(text))


def test_parity_preserves_constants_and_cancels_duplicate_selectors():
    from qdk.simulation._qodec.readout_equations import (
        BinarySystem,
        expression,
        reference_key,
    )

    parity = expression([1, Reference("circuit.readouts[0,0,2]"), 0])
    known = expression([Reference("circuit.readouts[2]"), 1])
    system = BinarySystem([known])
    assert parity.variables == frozenset(
        {reference_key(Reference("circuit.readouts[2]"))}
    )
    assert system.value(parity) is False
    assert system.value(expression([Reference("circuit.readouts[1]")])) is None


def test_readout_definitions_can_reference_later_readouts():
    from qdk.simulation._qodec.readout_equations import BinarySystem, expression

    system = BinarySystem(
        [
            expression([Reference("readouts[0]"), Reference("readouts[1]")]),
            expression([Reference("readouts[1]"), Reference("circuit.readouts[0]")]),
            expression([Reference("circuit.readouts[0]"), 1]),
        ]
    )
    assert system.value(expression([Reference("readouts[0]")])) is True
    assert system.value(expression([Reference("readouts[1]")])) is True


def test_uniquely_solvable_cycle_has_exact_values():
    from qdk.simulation._qodec.readout_equations import BinarySystem, expression

    system = BinarySystem(
        [
            expression([Reference("readouts[0]"), Reference("readouts[1]")]),
            expression(
                [
                    Reference("readouts[1]"),
                    Reference("readouts[0]"),
                    Reference("readouts[2]"),
                ]
            ),
            expression([Reference("readouts[2]"), Reference("readouts[1]"), 1]),
        ]
    )
    assert [
        system.value(expression([Reference(f"readouts[{index}]")]))
        for index in range(3)
    ] == [True, True, False]


def test_underdetermined_individual_bits_can_have_a_determined_parity():
    from qdk.simulation._qodec.readout_equations import BinarySystem, expression

    first, second = Reference("in[0].x[0]"), Reference("out[0].x[0]")
    system = BinarySystem([expression([first, second, 1])])
    assert system.value(expression([first])) is None
    assert system.value(expression([second])) is None
    assert system.value(expression([first, second])) is True


def test_inconsistent_equations_are_not_unavailable_bits():
    from qdk.simulation._qodec.readout_equations import (
        BinarySystem,
        InconsistentParity,
        expression,
    )

    reference = Reference("readouts[0]")
    with pytest.raises(InconsistentParity):
        BinarySystem([expression([reference]), expression([reference, 1])])


def test_binary_solver_matches_exhaustive_small_systems():
    from qdk.simulation._qodec.readout_equations import (
        BinarySystem,
        InconsistentParity,
        Parity,
    )

    variables = ("first", "second", "third")
    all_rows = [
        Parity(
            frozenset(
                variable
                for variable, present in zip(variables, coefficients)
                if present
            ),
            constant,
        )
        for coefficients in product((False, True), repeat=3)
        for constant in (False, True)
    ]
    assignments: list[dict[Hashable, bool]] = [
        dict(zip(variables, values)) for values in product((False, True), repeat=3)
    ]
    for rows in product(all_rows, repeat=2):
        solutions = [
            assignment
            for assignment in assignments
            if all(
                sum(assignment[variable] for variable in row.variables) % 2
                == row.constant
                for row in rows
            )
        ]
        if not solutions:
            with pytest.raises(InconsistentParity):
                BinarySystem(rows)
            continue
        system = BinarySystem(rows)
        for query in all_rows:
            outcomes = {
                bool(sum(assignment[variable] for variable in query.variables) % 2)
                ^ query.constant
                for assignment in solutions
            }
            assert system.value(query) == (
                outcomes.pop() if len(outcomes) == 1 else None
            )


@pytest.mark.parametrize("cyclic", [False, True])
def test_syndrome_decoder_solves_readout_aliases_without_declaration_order(cyclic):
    import qodec
    from qodec.actions import Observe

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from .test_execution_pipeline import decode_gadget

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    layer = codec.layers[0]
    original = layer.gadgets["measure_z"]
    instruction = qodec.Instruction(
        "measure_z",
        inputs=original.implements.inputs,
        action=[Observe(["Z_0", "Z_0", "I"] if cyclic else ["Z_0", "Z_0"])],
    )
    declarations = layer.instruction_set.instructions
    declarations["measure_z"] = instruction
    layer.instruction_set.instructions = declarations
    original.implements = instruction
    original.readouts = (
        [
            ["readouts[1]"],
            ["readouts[0]", "readouts[2]"],
            ["readouts[1]", "circuit.readouts[0]", "in[0].z[0]"],
        ]
        if cyclic
        else [["readouts[1]"], ["circuit.readouts[0]", "in[0].z[0]"]]
    )
    codec.validate()
    session = prepare_syndrome_decoder(layer)(7)
    try:
        assert decode_gadget(session, original, (True, True, True)).readouts == (
            (True, True, False) if cyclic else (True, True)
        )
    finally:
        session.close()


def framed_gadget(frames, readouts=()):
    import qodec
    from qodec.actions import Observe, Stabilize
    from qodec.gadgets import Circuit, Encoding
    from qodec.instructions import Block, BlockOperand

    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    code = qodec.Code("wire", [], ["X_0"], ["Z_0"])
    instruction = qodec.Instruction(
        "prepare",
        outputs=[BlockOperand("wire")],
        action=[Stabilize(["Z_0"]), Observe(["Z_0"] * len(readouts))],
    )
    source = qodec.InstructionSet(
        "wire", blocks=[Block("wire", 1)], instructions=[instruction]
    )
    gadget = qodec.Gadget(
        instruction,
        Circuit(target, "R 0 1\nM 1", format="stim"),
        outputs=[Encoding(code, support=["0"])],
        frames=frames,
        readouts=list(readouts),
    )
    return qodec.Layer(source, codes={"wire": code}, gadgets=[gadget]), gadget


@pytest.mark.parametrize("bit", [False, True])
def test_explicit_output_frame_emits_the_corresponding_logical_correction(bit):
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.protocols import Correction
    from qdk.simulation._qodec.quantum_operations import Operation
    from .test_execution_pipeline import decode_gadget, invocation_for

    layer, gadget = framed_gadget({"out[0].z[0]": ["circuit.readouts[0]", 1]})
    session = prepare_syndrome_decoder(layer)(7)
    corrections = []
    try:
        assert decode_gadget(session, gadget, (bit,), corrections).readouts == ()
        assert corrections == (
            []
            if bit
            else [Correction(invocation_for(gadget).outputs, Operation("x", (0,)))]
        )
    finally:
        session.close()


def test_unavailable_frame_bit_is_an_unresolved_shot_failure():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.protocols import ExecutionUnresolved
    from .test_execution_pipeline import decode_gadget

    layer, gadget = framed_gadget({"out[0].z[0]": ["circuit.readouts[0]"]})
    session = prepare_syndrome_decoder(layer)(7)
    try:
        with pytest.raises(ExecutionUnresolved, match="unavailable circuit readouts"):
            decode_gadget(session, gadget, (None,))
    finally:
        session.close()


def test_frame_aliases_can_resolve_forward_to_record_only_equations():
    from qdk.simulation._qodec.readout_equations import (
        BinarySystem,
        expression,
        prepare_frames,
    )

    _, gadget = framed_gadget(
        {"out[0].x[0]": ["readouts[0]"]}, [["readouts[1]"], ["circuit.readouts[0]", 1]]
    )
    (frame,) = prepare_frames(gadget)
    assert (frame.output, frame.basis, frame.logical) == (0, "x", 0)
    system = BinarySystem([expression([Reference("circuit.readouts[0]")])])
    assert system.value(frame.parity) is True


@pytest.mark.parametrize(
    "frames, readouts",
    [
        ({"out[0].z[0]": ["out[0].z[0]", "out[0].z[0]"]}, []),
        ({"out[0].z[0]": ["readouts[0]"]}, [["in[0].x[0]"]]),
        ({"out[0].z[0]": ["readouts[0]"]}, [["readouts[1]"], ["readouts[0]"]]),
        ({"out[0].stabilizers[0]": []}, []),
        ({"out[0].z[4]": []}, []),
    ],
)
def test_invalid_frame_dependencies_are_rejected_before_execution(frames, readouts):
    from qdk.simulation._qodec.readout_equations import prepare_frames

    _, gadget = framed_gadget(frames, readouts)
    with pytest.raises(ValueError, match="[Ff]rame"):
        prepare_frames(gadget)


def test_partial_css_measurement_syndrome_can_correct_a_logical_readout():
    import qodec
    from qodec.actions import Observe
    from qodec.gadgets import Circuit, Encoding
    from qodec.instructions import Block, BlockOperand

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from .test_execution_pipeline import decode_gadget

    code = qodec.Code(
        "css", ["X_0 X_1 X_2 X_3", "Z_0 Z_1", "Z_2 Z_3"], ["X_0 X_1"], ["Z_0 Z_2"]
    )
    instruction = qodec.Instruction(
        "measure", inputs=[BlockOperand("css")], action=[Observe(["Z_0"])]
    )
    source = qodec.InstructionSet(
        "css", blocks=[Block("css", 1)], instructions=[instruction]
    )
    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    gadget = qodec.Gadget(
        instruction,
        Circuit(target, "M 0 1 2 3", format="stim"),
        inputs=[Encoding(code, support=["0", "1", "2", "3"])],
        checks=[
            ["circuit.readouts[0,1]", "in[0].stabilizers[1]"],
            ["circuit.readouts[2,3]", "in[0].stabilizers[2]"],
        ],
        readouts=[["circuit.readouts[0,2]", "in[0].z[0]"]],
    )
    session = prepare_syndrome_decoder(
        qodec.Layer(source, codes={"css": code}, gadgets=[gadget])
    )(7)
    try:
        assert decode_gadget(
            session, gadget, (False, False, False, False)
        ).readouts == (False,)
        assert decode_gadget(session, gadget, (True, True, False, False)).readouts == (
            True,
        )
    finally:
        session.close()


def test_boundary_transport_uses_prior_output_evidence_and_lifetime_identity():
    import qodec
    from qodec.gadgets import Circuit
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from qdk.simulation._qodec.protocols import BlockReference, Invocation
    from .test_execution_pipeline import drive_requests

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    prepare = layer.gadgets["prepare_z"]
    prepare.checks = [["out[0].stabilizers[0]"], ["out[0].stabilizers[1]"]]
    idle = layer.gadgets["idle"]
    idle.circuit = Circuit(idle.circuit.instruction_set, "[]", format="yaml")
    idle.checks = [
        ["in[0].stabilizers[0]", "out[0].stabilizers[0]"],
        ["in[0].stabilizers[1]", "out[0].stabilizers[1]"],
    ]
    session = prepare_syndrome_decoder(layer)(7)
    assert isinstance(session, SyndromeSession)
    block = BlockReference("data", 1, "repetition3")
    fresh = BlockReference("data", 2, "repetition3")
    try:
        drive_requests(
            session.decode(
                Invocation(
                    0,
                    prepare,
                    InstructionCall("prepare_z", operands=["data"]),
                    (),
                    (block,),
                ),
                (),
            ),
            lambda correction: (),
        )
        assert (
            drive_requests(
                session.decode(
                    Invocation(
                        1,
                        idle,
                        InstructionCall("idle", operands=["data"]),
                        (block,),
                        (block,),
                    ),
                    (),
                ),
                lambda correction: (),
            ).readouts
            == ()
        )
        assert session.boundaries[block][("stabilizers", 0)] is False
        assert fresh not in session.boundaries
        session.discarded((block,))
        assert block not in session.boundaries
    finally:
        session.close()


def test_an_unobserved_required_readout_remains_unavailable():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from .test_execution_pipeline import decode_gadget
    import qodec

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    session = prepare_syndrome_decoder(layer)(7)
    try:
        assert decode_gadget(
            session, layer.gadgets["measure_z"], (None, False, False)
        ).readouts == (None,)
    finally:
        session.close()


@pytest.mark.parametrize("x_sign, z_sign", list(product((False, True), repeat=2)))
def test_declared_logical_boundary_relations_honor_incoming_signs(x_sign, z_sign):
    import qodec
    from qodec.gadgets import Circuit
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from qdk.simulation._qodec.protocols import BlockReference, Invocation
    from .test_execution_pipeline import drive_requests

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    gadget = layer.gadgets["idle"]
    gadget.circuit = Circuit(gadget.circuit.instruction_set, "[]", format="yaml")
    gadget.checks = [
        [f"in[0].{basis}[{index}]", f"out[0].{basis}[{index}]"]
        for basis, count in (("stabilizers", 2), ("x", 1), ("z", 1))
        for index in range(count)
    ]
    session = prepare_syndrome_decoder(layer)(7)
    assert isinstance(session, SyndromeSession)
    block = BlockReference("data", 1, "repetition3")
    session.boundaries[block] = {
        ("stabilizers", 0): False,
        ("stabilizers", 1): False,
        ("x", 0): x_sign,
        ("z", 0): z_sign,
    }
    corrections = []
    try:
        result = drive_requests(
            session.decode(
                Invocation(
                    0,
                    gadget,
                    InstructionCall("idle", operands=["data"]),
                    (block,),
                    (block,),
                ),
                (),
            ),
            lambda correction: corrections.append(correction) or (),
        )
        assert result.readouts == ()
        assert session.boundaries[block][("x", 0)] is False
        assert session.boundaries[block][("z", 0)] is False
        assert len(corrections) == (3 if z_sign else int(x_sign))
    finally:
        session.close()


def test_failed_explicit_frame_correction_does_not_commit_output_state():
    from contextlib import closing

    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from .test_execution_pipeline import invocation_for

    layer, gadget = framed_gadget({"out[0].z[0]": [1]})
    session = prepare_syndrome_decoder(layer)(7)
    assert isinstance(session, SyndromeSession)
    invocation = invocation_for(gadget)
    try:
        with closing(session.decode(invocation, (False,))) as requests:
            next(requests)
            assert invocation.outputs[0] not in session.boundaries
        assert invocation.outputs[0] not in session.boundaries
    finally:
        session.close()


@pytest.mark.parametrize(
    "reference", ["in[4].x[0]", "in[0].z[1]", "out[0].z[0]", "readouts[1]"]
)
def test_invalid_parity_references_fail_during_decoder_preparation(reference):
    import qodec

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    layer.gadgets["measure_z"].readouts = [[reference]]
    with pytest.raises(ValueError, match="reference"):
        prepare_syndrome_decoder(layer)


def test_missing_readout_equations_are_not_constant_zero():
    import qodec

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    layer.gadgets["measure_z"].readouts = []
    with pytest.raises(ValueError, match="readout equations"):
        prepare_syndrome_decoder(layer)


def test_out_of_range_circuit_reference_is_not_an_erasure():
    import qodec

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from .test_execution_pipeline import decode_gadget

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    gadget = layer.gadgets["measure_z"]
    gadget.readouts = [["circuit.readouts[9]"]]
    session = prepare_syndrome_decoder(layer)(7)
    try:
        with pytest.raises(ValueError, match="reference"):
            decode_gadget(session, gadget, (False, False, False))
    finally:
        session.close()


def test_dependent_stabilizer_positions_are_preserved():
    import qodec

    from qdk.simulation._qodec.decoding import CodeDecoder

    code = qodec.Code(
        "dependent", ["Z_0 Z_1", "Z_1 Z_2", "Z_0 Z_1"], ["X_0 X_1 X_2"], ["Z_0"]
    )
    decoder = CodeDecoder(code)
    correction = decoder.correct((True, False, True))
    assert len(decoder.stabilizers) == 3
    assert tuple(
        not correction.commutes_with(stabilizer) for stabilizer in decoder.stabilizers
    ) == (True, False, True)
    with pytest.raises(ValueError, match="matches syndrome"):
        decoder.correct((True, False, False))
