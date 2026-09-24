from . import FIXTURES
from contextlib import closing

import pytest
import qodec
from qodec.actions import Clifford, Condition, Observe, Pauli, Rotate, Stabilize
from qodec.instructions import BlockOperand, Parameter
from ec_tests.testing.optional import requires_stim


def test_action_runtime_preserves_sequential_outcomes_and_xor_guards():
    from qdk.simulation._qodec.action_runtime import prepare_actions
    from qdk.simulation._qodec.quantum_instruments import (
        Observation,
        PauliGate,
        TraceOut,
    )

    instruction = qodec.Instruction(
        "conditional",
        inputs=[BlockOperand("wire")],
        parameters=[Parameter("bit", "bit")],
        action=[
            Observe(["Z_0", "Z_0"]),
            Pauli("X_0", condition=Condition(["bit", "outcomes[1]"])),
        ],
    )
    program = prepare_actions(instruction, 1, 0, {"bit": True})
    with closing(program.run()) as requests:
        assert isinstance(next(requests), Observation)
        assert isinstance(requests.send((False,)), Observation)
        assert isinstance(requests.send((False,)), PauliGate)
        assert requests.send(()) == TraceOut((0,))
        with pytest.raises(StopIteration) as result:
            requests.send(())
        assert result.value.value == (False, False)


def test_sparse_semantic_temporary_qubits_are_compact_and_consumed():
    from qdk.simulation._qodec.action_runtime import prepare_actions
    from qdk.simulation._qodec.quantum_instruments import (
        Allocate,
        Observation,
        Stabilization,
        TraceOut,
    )

    instruction = qodec.Instruction(
        "temporary",
        inputs=[BlockOperand("wire")],
        outputs=[BlockOperand("wire")],
        action=[Stabilize(["Z_7"]), Observe(["Z_7"])],
    )
    program = prepare_actions(instruction, 1, 1, {})
    assert program.num_qubits == 2
    with closing(program.run()) as requests:
        assert next(requests) == Allocate((1,))
        step = requests.send(())
        assert isinstance(step, Stabilization)
        assert step.operators[0].characters == "IZ"
        assert isinstance(requests.send(()), Observation)
        assert requests.send((False,)) == TraceOut((1,))
        with pytest.raises(StopIteration) as result:
            requests.send(())
        assert result.value.value == (False,)


@pytest.mark.parametrize(
    "actions",
    [
        [Observe(["Z_7"])],
        [Stabilize(["Z_7"], condition=Condition(["bit"]))],
        [Stabilize(["Z_7"]), Observe(["Z_6"])],
        [Pauli("X_0", condition=Condition(["outcomes[0]"])), Observe(["Z_0"])],
    ],
)
def test_invalid_action_indices_and_guards_fail_during_preparation(actions):
    from qdk.simulation._qodec.action_runtime import prepare_actions

    instruction = qodec.Instruction(
        "invalid",
        inputs=[BlockOperand("wire")],
        outputs=[BlockOperand("wire")],
        parameters=[Parameter("bit", "bit")],
        action=actions,
    )
    with pytest.raises(ValueError):
        prepare_actions(instruction, 1, 1, {"bit": False})


def test_pauli_and_rotation_parameters_keep_their_full_signed_operator():
    from qdk.simulation._qodec.action_runtime import prepare_actions
    from qdk.simulation._qodec.quantum_instruments import Observation, PauliRotation

    instruction = qodec.Instruction(
        "parameterized",
        inputs=[BlockOperand("pair")],
        outputs=[BlockOperand("pair")],
        parameters=[Parameter("operator", "pauli"), Parameter("theta", "number")],
        action=[Rotate("operator", "theta"), Observe(["operator"])],
    )
    program = prepare_actions(instruction, 2, 2, {"operator": "-X_0 Y_1", "theta": 0.7})
    with closing(program.run()) as requests:
        rotation = next(requests)
        assert isinstance(rotation, PauliRotation)
        assert rotation.operator.characters == "XY" and rotation.operator.phase == -1
        assert rotation.angle == 0.7
        assert isinstance(requests.send(()), Observation)
        with pytest.raises(StopIteration) as result:
            requests.send((True,))
        assert result.value.value == (True,)


@pytest.mark.parametrize("bit", [False, True])
def test_unless_guard_has_even_parity_semantics(bit):
    from qdk.simulation._qodec.action_runtime import prepare_actions
    from qdk.simulation._qodec.quantum_instruments import PauliGate

    instruction = qodec.Instruction(
        "unless",
        inputs=[BlockOperand("wire")],
        outputs=[BlockOperand("wire")],
        parameters=[Parameter("bit", "bit")],
        action=[Pauli("X_0", condition=Condition(["bit"], invert=True))],
    )
    requests = prepare_actions(instruction, 1, 1, {"bit": bit}).run()
    with closing(requests):
        if bit:
            with pytest.raises(StopIteration):
                next(requests)
        else:
            assert isinstance(next(requests), PauliGate)


def test_clifford_includes_previously_introduced_temporary_indices():
    from qdk.simulation._qodec.action_runtime import prepare_actions
    from qdk.simulation._qodec.quantum_instruments import CliffordGate

    instruction = qodec.Instruction(
        "temporary_h",
        inputs=[BlockOperand("wire")],
        outputs=[BlockOperand("wire")],
        action=[Stabilize(["Z_7"]), Clifford({"X_7": "Z_7", "Z_7": "X_7"})],
    )
    requests = prepare_actions(instruction, 1, 1, {}).run()
    with closing(requests):
        next(requests)
        requests.send(())
        gate = requests.send(())
        assert isinstance(gate, CliffordGate)
        assert gate.operator.image_z(1).characters == "IX"


@requires_stim
def test_physical_action_execution_supports_temporaries_guards_and_typed_blocks():
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from .test_layer_layout import drive

    operand = BlockOperand("pair")
    instruction_set = qodec.InstructionSet(
        "pair",
        blocks=[Block("pair", 2)],
        instructions=[
            qodec.Instruction(
                "prepare", outputs=[operand], action=[Stabilize(["Z_0", "Z_1"])]
            ),
            qodec.Instruction(
                "joint",
                inputs=[operand],
                outputs=[operand],
                action=[
                    Stabilize(["Z_7"]),
                    Clifford({"X_0": "X_0 X_7", "Z_7": "Z_0 Z_7"}),
                    Observe(["Z_7"]),
                    Pauli("X_1", condition=Condition(["outcomes[0]"])),
                ],
            ),
            qodec.Instruction(
                "x", inputs=[operand], outputs=[operand], action=[Pauli("X_0")]
            ),
            qodec.Instruction(
                "measure", inputs=[operand], action=[Observe(["Z_0", "Z_1"])]
            ),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(instruction_set))
    resources = runtime.required_resources(Resources(blocks={"pair": 1}))
    assert resources.qubits == 3
    runtime.start(resources)
    backend = full_state_backend(None, 7)
    backend.start(resources)
    try:
        drive(
            runtime.handle(InstructionCall("prepare", operands=["data"])),
            backend.execute,
        )
        drive(runtime.handle(InstructionCall("x", operands=["data"])), backend.execute)
        assert drive(
            runtime.handle(InstructionCall("joint", operands=["data"])), backend.execute
        ) == (True,)
        assert drive(
            runtime.handle(InstructionCall("measure", operands=["data"])),
            backend.execute,
        ) == (True, True)
    finally:
        backend.close()
        runtime.close()


@pytest.mark.parametrize("labels", [(0, 4), ("data", "ancilla")])
def test_fresh_physical_circuit_inputs_begin_in_zero(labels):
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from .test_layer_layout import drive

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=2))
    backend = full_state_backend(None, 7)
    backend.start(Resources(qubits=2))
    data, ancilla = labels
    try:
        drive(runtime.handle(InstructionCall("X", operands=[data])), backend.execute)
        drive(
            runtime.handle(InstructionCall("CX", operands=[data, ancilla])),
            backend.execute,
        )
        assert drive(
            runtime.handle(InstructionCall("M", operands=[data])), backend.execute
        ) == (True,)
        assert drive(
            runtime.handle(InstructionCall("M", operands=[ancilla])), backend.execute
        ) == (True,)
    finally:
        runtime.close()
        backend.close()


@pytest.mark.parametrize("primitive", [False, True])
def test_consumed_physical_labels_cannot_be_used_as_fresh_inputs(primitive):
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_operations import Operation
    from .test_layer_layout import drive

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=1))
    emitted = []

    def respond(operation):
        emitted.append(operation)
        return (False,) if operation.name == "measure" else ()

    try:
        drive(runtime.handle(InstructionCall("M", operands=[0])), respond)
        emitted.clear()
        request = (
            Operation("x", (0,)) if primitive else InstructionCall("X", operands=[0])
        )
        with pytest.raises(ValueError, match="consumed"):
            drive(runtime.handle(request), respond)
        assert emitted == []
        assert runtime.layout is not None and runtime.layout.blocks == {}
    finally:
        runtime.close()


@pytest.mark.parametrize("primitive", [False, True])
def test_explicit_preparation_recreates_a_consumed_physical_label(primitive):
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from qdk.simulation._qodec.quantum_operations import Operation
    from .test_layer_layout import drive

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=1))
    backend = full_state_backend(None, 7)
    backend.start(Resources(qubits=1))
    try:
        drive(runtime.handle(InstructionCall("X", operands=[0])), backend.execute)
        assert drive(
            runtime.handle(InstructionCall("M", operands=[0])), backend.execute
        ) == (True,)
        prepare = (
            Operation("prepare", (0,))
            if primitive
            else InstructionCall("R", operands=[0])
        )
        drive(runtime.handle(prepare), backend.execute)
        assert drive(
            runtime.handle(InstructionCall("M", operands=[0])), backend.execute
        ) == (False,)
    finally:
        runtime.close()
        backend.close()


def test_fresh_storage_is_clean_without_adding_reset_noise():
    from qdk.simulation import NoiseConfig
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from .test_layer_layout import drive

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=1))
    noise = NoiseConfig()
    noise.mresetz.x = 1
    backend = full_state_backend(noise, 7)
    backend.start(Resources(qubits=1))
    try:
        assert drive(
            runtime.handle(InstructionCall("M", operands=["fresh"])), backend.execute
        ) == (False,)
        drive(runtime.handle(InstructionCall("X", operands=["used"])), backend.execute)
        assert drive(
            runtime.handle(InstructionCall("M", operands=["used"])), backend.execute
        ) == (True,)
        assert drive(
            runtime.handle(InstructionCall("M", operands=["recycled"])), backend.execute
        ) == (False,)
        drive(
            runtime.handle(InstructionCall("R", operands=["recycled"])), backend.execute
        )
        assert drive(
            runtime.handle(InstructionCall("M", operands=["recycled"])), backend.execute
        ) == (True,)
    finally:
        runtime.close()
        backend.close()


def test_release_allows_a_new_circuit_to_reuse_a_consumed_label():
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from qdk.simulation._qodec.quantum_operations import Operation
    from .test_layer_layout import drive

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=1))
    backend = full_state_backend(None, 7)
    backend.start(Resources(qubits=1))
    try:
        for _ in range(2):
            drive(runtime.handle(InstructionCall("X", operands=[0])), backend.execute)
            assert drive(
                runtime.handle(InstructionCall("M", operands=[0])), backend.execute
            ) == (True,)
            drive(runtime.handle(Operation("discard", (0,))), backend.execute)
    finally:
        runtime.close()
        backend.close()


def test_replaced_physical_output_does_not_dirty_a_fresh_input():
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from .test_layer_layout import drive

    operand = BlockOperand("wire")
    isa = qodec.InstructionSet(
        "replacement",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "x", inputs=[operand], outputs=[operand], action=[Pauli("X_0")]
            ),
            qodec.Instruction(
                "extend",
                inputs=[operand],
                outputs=[operand, operand],
                action=[
                    Observe(["Z_0"]),
                    Stabilize(["Z_1"]),
                ],
            ),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=2))
    backend = full_state_backend(None, 7)
    backend.start(Resources(qubits=2))
    try:
        drive(
            runtime.handle(InstructionCall("x", operands=["replaced"])), backend.execute
        )
        assert drive(
            runtime.handle(InstructionCall("extend", operands=["fresh", "replaced"])),
            backend.execute,
        ) == (False,)
    finally:
        runtime.close()
        backend.close()


def test_fresh_input_capacity_failure_does_not_allocate_a_partial_call():
    from qdk.simulation._qodec.call_binding import bind_call
    from qdk.simulation._qodec.physical_layout import PhysicalLayout
    from qodec.instructions import InstructionCall

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    layout = PhysicalLayout(2)
    retained = layout.allocate("retained", "qubit", 1)
    before = (dict(layout.blocks), list(layout.free))
    binding = bind_call(isa, InstructionCall("CX", operands=["first", "second"]))
    with pytest.raises(ValueError, match="capacity"):
        layout.begin(binding, 2)
    assert (layout.blocks, layout.free) == before
    assert layout.blocks["retained"] == retained


def test_physical_variadic_pauli_observation_binds_call_time_width():
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from .test_layer_layout import drive

    operand = BlockOperand("wire")
    many = BlockOperand("wire", is_variadic=True)
    instruction_set = qodec.InstructionSet(
        "many",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "prepare", outputs=[operand], action=[Stabilize(["Z_0"])]
            ),
            qodec.Instruction(
                "mpp",
                inputs=[many],
                outputs=[many],
                parameters=[Parameter("p", "pauli")],
                action=[Observe(["p"])],
            ),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(instruction_set))
    resources = runtime.required_resources(Resources(qubits=3))
    runtime.start(resources)
    backend = full_state_backend(None, 7)
    backend.start(resources)
    try:
        for target in range(3):
            drive(
                runtime.handle(InstructionCall("prepare", operands=[target])),
                backend.execute,
            )
        assert drive(
            runtime.handle(
                InstructionCall(
                    "mpp", operands=[0, 1, 2], arguments={"p": "-Z_0 Z_1 Z_2"}
                )
            ),
            backend.execute,
        ) == (True,)
    finally:
        backend.close()
        runtime.close()


def test_parameterized_action_reserves_literal_temporary_workspace():
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_backend import full_state_backend
    from .test_layer_layout import drive

    operand = BlockOperand("wire")
    isa = qodec.InstructionSet(
        "temporary_parameter",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "prepare", outputs=[operand], action=[Stabilize(["Z_0"])]
            ),
            qodec.Instruction(
                "probe",
                inputs=[operand],
                outputs=[operand],
                parameters=[Parameter("theta", "number")],
                action=[
                    Stabilize(["Z_7"]),
                    Rotate("Y_7", "theta"),
                    Observe(["Z_7"]),
                ],
            ),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    resources = runtime.required_resources(Resources(qubits=1))
    assert resources == Resources(qubits=2)
    runtime.start(resources)
    backend = full_state_backend(None, 7)
    backend.start(resources)
    try:
        drive(runtime.handle(InstructionCall("prepare", operands=[0])), backend.execute)
        assert drive(
            runtime.handle(
                InstructionCall("probe", operands=[0], arguments={"theta": 0})
            ),
            backend.execute,
        ) == (False,)
    finally:
        runtime.close()
        backend.close()


@pytest.mark.parametrize("capacity", [1, 2])
def test_caller_budget_covers_argument_defined_temporary_qubits(capacity):
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from .test_layer_layout import drive

    operand = BlockOperand("wire")
    isa = qodec.InstructionSet(
        "dynamic_temporary",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "prepare", outputs=[operand], action=[Stabilize(["Z_0"])]
            ),
            qodec.Instruction(
                "temporary",
                inputs=[operand],
                outputs=[operand],
                parameters=[Parameter("p", "pauli")],
                action=[Stabilize(["p"]), Observe(["p"])],
            ),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=capacity))
    emitted = []

    def respond(operation):
        emitted.append(operation)
        return (False,) if operation.name == "measure" else ()

    try:
        drive(runtime.handle(InstructionCall("prepare", operands=[0])), respond)
        emitted.clear()
        call = InstructionCall("temporary", operands=[0], arguments={"p": "Z_7"})
        if capacity == 1:
            with pytest.raises(ValueError, match="capacity"):
                drive(runtime.handle(call), respond)
            assert emitted == []
            assert runtime.layout is not None and runtime.layout.blocks[0].qubits == (
                0,
            )
        else:
            assert drive(runtime.handle(call), respond) == (False,)
            assert runtime.layout is not None and runtime.layout.free == [1]
    finally:
        runtime.close()


def test_zero_operand_identity_observations_need_no_physical_qubits():
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from .test_layer_layout import drive

    isa = qodec.InstructionSet(
        "padding",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction("zeros_and_ones", action=[Observe(["I", "-I", "I"])]),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    resources = runtime.required_resources(Resources())
    assert resources == Resources()
    runtime.start(resources)
    emitted = []
    try:
        assert drive(
            runtime.handle(InstructionCall("zeros_and_ones")),
            lambda operation: emitted.append(operation) or (),
        ) == (False, True, False)
        assert emitted == []
    finally:
        runtime.close()


@pytest.mark.parametrize("source", ["-X_0", "Y_0", "X_0 Z_1", "I"])
def test_action_clifford_keys_must_be_positive_individual_generators(source):
    from qdk.simulation._qodec.action_runtime import prepare_actions

    instruction = qodec.Instruction(
        "invalid_clifford", action=[Clifford({source: "X_0"})]
    )
    with pytest.raises(ValueError, match="generator"):
        prepare_actions(instruction, 2, 2, {})


@pytest.mark.parametrize("observable", ["Z_0", "X_0"])
def test_physical_flags_follow_outcomes_and_are_ideal_zero(observable):
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import ExecutionRejected, Resources
    from .test_layer_layout import drive

    operand = BlockOperand("wire")
    isa = qodec.InstructionSet(
        "flagged",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "prepare", outputs=[operand], action=[Stabilize(["Z_0"])]
            ),
            qodec.Instruction(
                "checked",
                inputs=[operand],
                outputs=[operand],
                action=[Observe([observable])],
                flags=["reject", "leak"],
            ),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(runtime.required_resources(Resources(qubits=1)))

    def respond(operation):
        return (True,) if operation.name == "measure" else ()

    try:
        drive(runtime.handle(InstructionCall("prepare", operands=[0])), respond)
        accepted = InstructionCall(
            "checked", operands=[0], select=[{"reject": 0, "leak": 0}]
        )
        assert drive(runtime.handle(accepted), respond) == (True, False, False)
        with pytest.raises(ExecutionRejected):
            drive(
                runtime.handle(
                    InstructionCall("checked", operands=[0], select=[{"reject": 1}])
                ),
                respond,
            )
    finally:
        runtime.close()


def test_ideal_flag_only_instruction_needs_no_qubits_or_backend_work():
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from .test_layer_layout import drive

    isa = qodec.InstructionSet(
        "flags", instructions=[qodec.Instruction("status", flags=["reject"])]
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources())
    emitted = []
    try:
        assert drive(
            runtime.handle(InstructionCall("status")),
            lambda operation: emitted.append(operation) or (),
        ) == (False,)
        assert emitted == []
    finally:
        runtime.close()


@pytest.mark.parametrize("name", ["x", "discard"])
def test_failed_direct_physical_operations_cannot_be_resumed(name):
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_operations import Operation
    from .test_layer_layout import drive

    isa = qodec.InstructionSet(
        "wire",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "prepare", outputs=[BlockOperand("wire")], action=[Stabilize(["Z_0"])]
            ),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=1))

    def fail(operation):
        raise RuntimeError("physical operation failed")

    try:
        drive(
            runtime.handle(InstructionCall("prepare", operands=[0])),
            lambda operation: (),
        )
        with pytest.raises(RuntimeError, match="physical operation failed"):
            drive(runtime.handle(Operation(name, (0,))), fail)
        with pytest.raises(RuntimeError, match="runtime has failed"):
            drive(
                runtime.handle(InstructionCall("prepare", operands=[0])),
                lambda operation: (),
            )
    finally:
        runtime.close()


def test_primitive_consuming_instruction_traces_out_without_exporting_a_bit():
    from qodec.instructions import Block, InstructionCall

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_operations import Operation
    from .test_layer_layout import drive

    operand = BlockOperand("wire")
    isa = qodec.InstructionSet(
        "consume",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "prepare", outputs=[operand], action=[Stabilize(["Z_0"])]
            ),
            qodec.Instruction("consume", inputs=[operand], action=[Pauli("X_0")]),
        ],
    )
    runtime = InstructionRuntime(InstructionSet(isa))
    runtime.start(Resources(qubits=1))
    emitted = []
    try:
        drive(
            runtime.handle(InstructionCall("prepare", operands=[0])),
            lambda operation: (),
        )
        assert (
            drive(
                runtime.handle(InstructionCall("consume", operands=[0])),
                lambda operation: emitted.append(operation) or (),
            )
            == ()
        )
        assert emitted == [Operation("x", (0,)), Operation("discard", (0,))]
        assert runtime.layout is not None and runtime.layout.blocks == {}
    finally:
        runtime.close()
