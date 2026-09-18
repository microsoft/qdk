import pytest
import qodec
from qodec.instructions import Block, BlockOperand, InstructionCall, Parameter


def test_binding_expands_variadic_blocks_and_logical_offsets():
    from qdk.simulation._qodec.call_binding import bind_call

    operands = [BlockOperand("single"), BlockOperand("pair", is_variadic=True)]
    instruction = qodec.Instruction("joint", inputs=operands, outputs=operands)
    isa = qodec.InstructionSet(
        "mixed",
        blocks=[Block("single", 1), Block("pair", 2)],
        instructions=[instruction],
    )
    bound = bind_call(
        isa, InstructionCall("joint", operands=["ancilla", "first", "second"])
    )

    assert [
        (operand.label, operand.block_type, operand.offset, operand.width)
        for operand in bound.inputs
    ] == [
        ("ancilla", "single", 0, 1),
        ("first", "pair", 1, 2),
        ("second", "pair", 3, 2),
    ]
    assert bound.inputs == bound.outputs
    assert bound.input_capacity == bound.output_capacity == 5


def test_binding_handles_regrouping_by_capacity_not_operand_count():
    from qdk.simulation._qodec.call_binding import bind_call

    instruction = qodec.Instruction(
        "split", inputs=[BlockOperand("pair")], outputs=[BlockOperand("single")] * 2
    )
    isa = qodec.InstructionSet(
        "mixed",
        blocks=[Block("single", 1), Block("pair", 2)],
        instructions=[instruction],
    )
    bound = bind_call(
        isa,
        InstructionCall("split", operands=["old", "new"]),
        input_types={"old": "pair"},
    )

    assert len(bound.inputs) == 1 and len(bound.outputs) == 2
    assert bound.input_capacity == bound.output_capacity == 2
    assert [operand.offset for operand in bound.outputs] == [0, 1]


@pytest.mark.parametrize("variadic", [False, True])
def test_binding_accepts_zero_operands(variadic):
    from qdk.simulation._qodec.call_binding import bind_call

    operands = [BlockOperand("pair", is_variadic=True)] if variadic else []
    instruction = qodec.Instruction("empty", inputs=operands, outputs=operands)
    isa = qodec.InstructionSet(
        "pair", blocks=[Block("pair", 2)], instructions=[instruction]
    )
    bound = bind_call(isa, InstructionCall("empty"))
    assert bound.inputs == bound.outputs == ()
    assert bound.input_capacity == bound.output_capacity == 0


def test_multiple_variadic_groups_use_known_types_or_report_ambiguity():
    from qdk.simulation._qodec.call_binding import bind_call

    instruction = qodec.Instruction(
        "consume",
        inputs=[
            BlockOperand("single", is_variadic=True),
            BlockOperand("pair", is_variadic=True),
        ],
    )
    isa = qodec.InstructionSet(
        "mixed",
        blocks=[Block("single", 1), Block("pair", 2)],
        instructions=[instruction],
    )
    call = InstructionCall("consume", operands=["first", "second"])
    with pytest.raises(ValueError, match="Ambiguous"):
        bind_call(isa, call)
    bound = bind_call(isa, call, input_types={"first": "single", "second": "pair"})
    assert [operand.block_type for operand in bound.inputs] == ["single", "pair"]
    with pytest.raises(ValueError, match="operand"):
        bind_call(isa, call, input_types={"first": "pair", "second": "single"})


@pytest.mark.parametrize(
    "kind, value",
    [
        ("number", 0.5),
        ("number", 1),
        ("integer", -2),
        ("boolean", True),
        ("bit", False),
        ("bit", 1),
        ("string", "true"),
        ("pauli", "-X_0 Z_1"),
        ("number", [1, 2]),
        ("string", ["left", "right"]),
    ],
)
def test_argument_kinds_preserve_values(kind, value):
    from qdk.simulation._qodec.call_binding import validate_arguments

    instruction = qodec.Instruction(
        "parameterized", parameters=[Parameter("value", kind)]
    )
    arguments = {"value": value}
    validate_arguments(instruction, arguments)
    assert arguments == {"value": value}


@pytest.mark.parametrize(
    "kind, value",
    [
        ("number", True),
        ("number", float("inf")),
        ("integer", 0.5),
        ("integer", False),
        ("boolean", 1),
        ("bit", 2),
        ("bit", "circuit.readouts[0]"),
        ("number", "0.5"),
        ("string", 2),
        ("pauli", False),
    ],
)
def test_argument_kind_mismatches_are_explicit(kind, value):
    from qdk.simulation._qodec.call_binding import validate_arguments

    instruction = qodec.Instruction(
        "parameterized", parameters=[Parameter("value", kind)]
    )
    with pytest.raises(TypeError, match="expects"):
        validate_arguments(instruction, {"value": value})


def test_wrong_operand_type_and_count_do_not_bind():
    from qdk.simulation._qodec.call_binding import bind_call

    instruction = qodec.Instruction(
        "pair_gate", inputs=[BlockOperand("pair")], outputs=[BlockOperand("pair")]
    )
    isa = qodec.InstructionSet(
        "mixed",
        blocks=[Block("single", 1), Block("pair", 2)],
        instructions=[instruction],
    )
    with pytest.raises(ValueError, match="operand"):
        bind_call(
            isa,
            InstructionCall("pair_gate", operands=["data"]),
            input_types={"data": "single"},
        )
    with pytest.raises(ValueError, match="operand count"):
        bind_call(isa, InstructionCall("pair_gate", operands=["first", "second"]))


def test_operation_binding_selects_a_logical_slot_inside_a_block():
    from qodec.actions import Pauli

    from qdk.simulation._qodec.instruction_set import InstructionSet
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qdk.simulation._qodec.quantum_operations import (
        LogicalSlot,
        decompose_rotations,
    )

    operand = BlockOperand("pair")
    isa = qodec.InstructionSet(
        "pair",
        blocks=[Block("pair", 2)],
        instructions=[
            qodec.Instruction(
                "first", inputs=[operand], outputs=[operand], action=[Pauli("X_0")]
            ),
            qodec.Instruction(
                "second", inputs=[operand], outputs=[operand], action=[Pauli("X_1")]
            ),
            qodec.Instruction("consume", inputs=[operand], action=[Pauli("X_1")]),
        ],
    )
    resolve = prepare_resolver(InstructionSet(isa), decompose_rotations)
    assert resolve("x", (LogicalSlot("data", 1, "pair"),), None) == (
        InstructionCall("second", operands=["data"]),
    )


def test_consuming_action_does_not_bind_as_a_unitary_gate():
    from qodec.actions import Pauli

    from qdk.simulation._qodec.instruction_set import InstructionSet, UnboundOperation

    isa = qodec.InstructionSet(
        "one",
        blocks=[Block("one", 1)],
        instructions=[
            qodec.Instruction(
                "consume", inputs=[BlockOperand("one")], action=[Pauli("X_0")]
            ),
        ],
    )
    with pytest.raises(UnboundOperation):
        InstructionSet(isa).bind("x", 1)


def test_joint_rotation_can_address_two_slots_of_the_same_block():
    from qodec.actions import Rotate

    from qdk.simulation._qodec.instruction_set import InstructionSet
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qdk.simulation._qodec.quantum_operations import (
        LogicalSlot,
        decompose_rotations,
    )

    operand = BlockOperand("pair")
    isa = qodec.InstructionSet(
        "pair",
        blocks=[Block("pair", 2)],
        instructions=[
            qodec.Instruction(
                "joint",
                inputs=[operand],
                outputs=[operand],
                parameters=[Parameter("theta", "number")],
                action=[Rotate("X_0 X_1", "theta")],
            ),
        ],
    )
    resolve = prepare_resolver(InstructionSet(isa), decompose_rotations)
    assert resolve(
        "rxx", (LogicalSlot("data", 0, "pair"), LogicalSlot("data", 1, "pair")), 0.5
    ) == (
        InstructionCall("joint", operands=["data"], arguments={"theta": 0.5}),
    )


@pytest.mark.parametrize("operation", ["prepare", "measure"])
def test_single_slot_binding_rejects_unrequested_block_lifetime_changes(operation):
    from qodec.actions import Observe, Stabilize

    from qdk.simulation._qodec.instruction_set import InstructionSet, UnboundOperation
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qdk.simulation._qodec.quantum_operations import (
        LogicalSlot,
        decompose_rotations,
    )

    operand = BlockOperand("pair")
    instruction = (
        qodec.Instruction(
            "prepare_partial", outputs=[operand], action=[Stabilize(["Z_0"])]
        )
        if operation == "prepare"
        else qodec.Instruction(
            "consume_pair", inputs=[operand], action=[Observe(["Z_0"])]
        )
    )
    isa = qodec.InstructionSet(
        "pair", blocks=[Block("pair", 2)], instructions=[instruction]
    )
    resolve = prepare_resolver(InstructionSet(isa), decompose_rotations)
    with pytest.raises(UnboundOperation):
        resolve(operation, (LogicalSlot("data", 0, "pair"),), None)


def test_single_slot_nondestructive_measurement_remains_bindable():
    from qodec.actions import Observe

    from qdk.simulation._qodec.instruction_set import InstructionSet
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qdk.simulation._qodec.quantum_operations import (
        LogicalSlot,
        decompose_rotations,
    )

    operand = BlockOperand("pair")
    isa = qodec.InstructionSet(
        "pair",
        blocks=[Block("pair", 2)],
        instructions=[
            qodec.Instruction(
                "measure_first",
                inputs=[operand],
                outputs=[operand],
                action=[Observe(["Z_0"])],
            ),
        ],
    )
    resolve = prepare_resolver(InstructionSet(isa), decompose_rotations)
    assert resolve("measure", (LogicalSlot("data", 0, "pair"),), None) == (
        InstructionCall("measure_first", operands=["data"]),
    )
