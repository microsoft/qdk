from . import FIXTURES
from contextlib import closing

import pytest
import qodec
from qodec.actions import Stabilize
from qodec.gadgets import Circuit, Encoding
from qodec.instructions import Block, BlockOperand, InstructionCall

from qdk.simulation._qodec.protocols import (
    Corrections,
    Decoded,
    Invocation,
    Readouts,
    Resources,
)
from ec_tests.testing.optional import requires_stim


def drive(requests, respond):
    with closing(requests):
        reply = None
        while True:
            try:
                request = requests.send(reply)
            except StopIteration as completed:
                return completed.value
            reply = respond(request)


class RecordingDecoder:
    def __init__(self):
        self.invocations = []
        self.discarded_blocks = []

    def decode(
        self, invocation: Invocation, readouts: Readouts
    ) -> Corrections[Decoded]:
        self.invocations.append(invocation)
        yield from ()
        return Decoded(())

    def discarded(self, blocks):
        self.discarded_blocks.extend(blocks)

    def close(self):
        pass


def test_encoded_support_addresses_logical_slots_of_lower_blocks():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.protocols import Correction
    from qdk.simulation._qodec.quantum_operations import (
        FrameUpdate,
        LogicalSlot,
        Operation,
    )

    code = qodec.Code(
        "pair_code", ["Z_0 Z_1", "Z_2 Z_3"], ["X_0 X_1", "X_2 X_3"], ["Z_0", "Z_2"]
    )
    prepare = qodec.Instruction(
        "prepare", outputs=[BlockOperand("data")], action=[Stabilize(["Z_0", "Z_1"])]
    )
    source = qodec.InstructionSet(
        "source", blocks=[Block("data", 2)], instructions=[prepare]
    )
    lower_prepare = qodec.Instruction(
        "P", outputs=[BlockOperand("pair")], action=[Stabilize(["Z_0", "Z_1"])]
    )
    target = qodec.InstructionSet(
        "target", blocks=[Block("pair", 2)], instructions=[lower_prepare]
    )
    gadget = qodec.Gadget(
        prepare,
        Circuit(target, "[{P: [left]}, {P: [right]}]", format="yaml"),
        outputs=[
            Encoding(code, support=["left", "right"], block_types=["pair", "pair"])
        ],
    )
    layer = qodec.Layer(source, codes={"data": code}, gadgets=[gadget])

    class Decoder(RecordingDecoder):
        def decode(
            self, invocation: Invocation, readouts: Readouts
        ) -> Corrections[Decoded]:
            yield Correction(invocation.outputs, Operation("x", (3,)))
            return (yield from super().decode(invocation, readouts))

    runtime = LayerRuntime(LayerPlan(layer), Decoder())
    resources = runtime.required_resources(Resources(blocks={"data": 1}))
    assert resources == Resources(blocks={"pair": 2})
    runtime.start(resources)
    emitted = []

    def respond(request):
        emitted.append(request)
        return ()

    try:
        drive(runtime.handle(InstructionCall("prepare", operands=["data"])), respond)
        assert emitted == [
            InstructionCall("P", operands=[0]),
            InstructionCall("P", operands=[1]),
            FrameUpdate("x", LogicalSlot(1, 1, "pair")),
        ]
        assert runtime.layout.blocks["data"].support == (0, 1)
    finally:
        runtime.close()


@requires_stim
def test_split_preserves_lifetimes_and_execution_boundaries():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.quantum_operations import Operation

    pair = qodec.Code("pair", [], ["X_0", "X_1"], ["Z_0", "Z_1"])
    single = qodec.Code("single", [], ["X_0"], ["Z_0"])
    prepare = qodec.Instruction(
        "prepare", outputs=[BlockOperand("pair")], action=[Stabilize(["Z_0", "Z_1"])]
    )
    split = qodec.Instruction(
        "split", inputs=[BlockOperand("pair")], outputs=[BlockOperand("single")] * 2
    )
    source = qodec.InstructionSet(
        "mixed",
        blocks=[Block("pair", 2), Block("single", 1)],
        instructions=[prepare, split],
    )
    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    pair_encoding = Encoding(pair, support=["0", "1"])
    layer = qodec.Layer(
        source,
        codes={"pair": pair, "single": single},
        gadgets=[
            qodec.Gadget(
                prepare,
                Circuit(target, "R 0 1", format="stim"),
                outputs=[pair_encoding],
            ),
            qodec.Gadget(
                split,
                Circuit(target, "[]", format="yaml"),
                inputs=[pair_encoding],
                outputs=[
                    Encoding(single, support=["0"]),
                    Encoding(single, support=["1"]),
                ],
            ),
        ],
    )
    events = []

    class Decoder(RecordingDecoder):
        def before(self, invocation: Invocation) -> Corrections[None]:
            events.append("before decoder")
            yield from ()

    decoder = Decoder()
    runtime = LayerRuntime(LayerPlan(layer), decoder)
    resources = runtime.required_resources(Resources(blocks={"pair": 2, "single": 2}))
    runtime.start(resources)

    def respond(request):
        if isinstance(request, Operation) and request.name == "discard":
            events.append("discard lower block")
        return ()

    try:
        drive(
            runtime.handle(InstructionCall("prepare", operands=["data"])),
            respond,
        )
        drive(
            runtime.handle(InstructionCall("prepare", operands=["other"])),
            respond,
        )
        original_data = runtime.layout.blocks["data"].reference
        replaced_other = runtime.layout.blocks["other"].reference
        events.clear()
        drive(
            runtime.handle(InstructionCall("split", operands=["data", "other"])),
            respond,
        )
        assert runtime.layout.blocks["data"].support == (0,)
        assert runtime.layout.blocks["other"].support == (1,)
        assert runtime.layout.blocks["data"].reference.block_type == "single"
        assert runtime.layout.blocks["data"].reference != original_data
        assert events == [
            "discard lower block",
            "discard lower block",
            "before decoder",
        ]
        assert decoder.discarded_blocks == [replaced_other, original_data]
        assert decoder.invocations[-1].inputs == (original_data,)
        assert len(decoder.invocations[-1].outputs) == 2

        runtime.start(resources)
        drive(
            runtime.handle(InstructionCall("prepare", operands=["data"])),
            respond,
        )
        drive(
            runtime.handle(InstructionCall("prepare", operands=["other"])),
            respond,
        )
        failure = RuntimeError("Lower-block discard failed")
        discarded = []

        def fail_second_discard(request):
            discarded.append(request)
            if len(discarded) == 2:
                raise failure
            return ()

        with pytest.raises(RuntimeError, match="Lower-block discard failed") as raised:
            drive(runtime.execute("split", ("data", "other"), {}), fail_second_discard)
        assert raised.value is failure
        assert runtime._failed
        with pytest.raises(RuntimeError, match="failed invocation"):
            drive(runtime.execute("split", ("data", "other"), {}), fail_second_discard)
        assert discarded == [Operation("discard", (2,)), Operation("discard", (3,))]
    finally:
        runtime.close()


@requires_stim
def test_code_change_can_expand_the_encoding_support():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    small = qodec.Code("small", ["Z_0 Z_1"], ["X_0 X_1"], ["Z_0"])
    large = qodec.Code("large", ["Z_0 Z_1", "Z_1 Z_2"], ["X_0 X_1 X_2"], ["Z_0"])
    prepare = qodec.Instruction(
        "prepare", outputs=[BlockOperand("small")], action=[Stabilize(["Z_0"])]
    )
    change = qodec.Instruction(
        "grow", inputs=[BlockOperand("small")], outputs=[BlockOperand("large")]
    )
    source = qodec.InstructionSet(
        "changing",
        blocks=[Block("small", 1), Block("large", 1)],
        instructions=[prepare, change],
    )
    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    initial = Encoding(small, support=["0", "1"])
    layer = qodec.Layer(
        source,
        codes={"small": small, "large": large},
        gadgets=[
            qodec.Gadget(
                prepare,
                Circuit(target, "R 0 1\nCX 0 1", format="stim"),
                outputs=[initial],
            ),
            qodec.Gadget(
                change,
                Circuit(target, "R 2\nCX 1 2", format="stim"),
                inputs=[initial],
                outputs=[Encoding(large, support=["0", "1", "2"])],
            ),
        ],
    )
    runtime = LayerRuntime(LayerPlan(layer), RecordingDecoder())
    runtime.start(
        runtime.required_resources(Resources(blocks={"small": 1, "large": 1}))
    )
    try:
        drive(
            runtime.handle(InstructionCall("prepare", operands=[0])), lambda request: ()
        )
        drive(runtime.handle(InstructionCall("grow", operands=[0])), lambda request: ())
        assert runtime.layout.blocks[0].support == (0, 1, 2)
        assert runtime.layout.blocks[0].reference.block_type == "large"
    finally:
        runtime.close()


@requires_stim
def test_layer_resolves_operations_on_the_second_logical_slot():
    from qodec.actions import Pauli
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.quantum_operations import LogicalSlot, Operation

    code = qodec.Code("pair", [], ["X_0", "X_1"], ["Z_0", "Z_1"])
    prepare = qodec.Instruction(
        "prepare", outputs=[BlockOperand("pair")], action=[Stabilize(["Z_0", "Z_1"])]
    )
    second = qodec.Instruction(
        "second_x",
        inputs=[BlockOperand("pair")],
        outputs=[BlockOperand("pair")],
        action=[Pauli("X_1")],
    )
    source = qodec.InstructionSet(
        "pair", blocks=[Block("pair", 2)], instructions=[prepare, second]
    )
    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    layer = qodec.Layer(
        source,
        codes={"pair": code},
        gadgets=[
            qodec.Gadget(
                prepare,
                Circuit(target, "R 0 1", format="stim"),
                outputs=[Encoding(code, support=["0", "1"])],
            ),
            qodec.Gadget(
                second,
                Circuit(target, "[{X: [1]}]", format="yaml"),
                inputs=[Encoding(code, support=["0", "1"])],
                outputs=[Encoding(code, support=["0", "1"])],
            ),
        ],
    )
    runtime = LayerRuntime(LayerPlan(layer), RecordingDecoder())
    runtime.start(runtime.required_resources(Resources(blocks={"pair": 1})))
    emitted = []
    try:
        drive(
            runtime.handle(InstructionCall("prepare", operands=[7])), lambda request: ()
        )
        drive(
            runtime.handle(Operation("x", (LogicalSlot(7, 1, "pair"),))),
            lambda request: emitted.append(request) or (),
        )
        assert emitted == [InstructionCall("X", operands=[1])]
        with pytest.raises(ValueError, match="slot"):
            drive(
                runtime.handle(Operation("x", (LogicalSlot(7, 2, "pair"),))),
                lambda request: (),
            )
    finally:
        runtime.close()


@requires_stim
def test_missing_encoded_input_is_not_implicitly_initialized():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    decoder = RecordingDecoder()
    runtime = LayerRuntime(LayerPlan(layer), decoder)
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    free = list(runtime.layout.free)
    emitted = []
    try:
        with pytest.raises(ValueError, match="not been prepared"):
            drive(
                runtime.handle(
                    InstructionCall("__quantum__qis__x__body", operands=[0])
                ),
                lambda request: emitted.append(request) or (),
            )
        assert emitted == []
        assert runtime.layout.blocks == {}
        assert runtime.layout.free == free
        assert decoder.invocations == []
    finally:
        runtime.close()


@requires_stim
def test_capacity_failure_preserves_live_blocks_and_invocation_identity():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    decoder = RecordingDecoder()
    runtime = LayerRuntime(LayerPlan(layer), decoder)
    runtime.start(Resources(qubits=3))
    try:
        drive(
            runtime.handle(InstructionCall("prepare_z", operands=[0])),
            lambda request: (),
        )
        before = (
            dict(runtime.layout.blocks),
            list(runtime.layout.free),
            dict(runtime.layout.generation_by_label),
            runtime._next_invocation,
        )
        with pytest.raises(ValueError, match="capacity"):
            drive(
                runtime.handle(InstructionCall("prepare_z", operands=[1])),
                lambda request: (),
            )
        assert (
            runtime.layout.blocks,
            runtime.layout.free,
            runtime.layout.generation_by_label,
            runtime._next_invocation,
        ) == before
        assert len(decoder.invocations) == 1
    finally:
        runtime.close()


def test_mixed_lower_types_require_their_own_capacity():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.protocols import PreparedCircuit

    code = qodec.Code("three", [], ["X_0", "X_1", "X_2"], ["Z_0", "Z_1", "Z_2"])
    instruction = qodec.Instruction("prepare", outputs=[BlockOperand("data")])
    source = qodec.InstructionSet(
        "source", blocks=[Block("data", 3)], instructions=[instruction]
    )
    target = qodec.InstructionSet(
        "target", blocks=[Block("pair", 2), Block("single", 1)], instructions=[]
    )
    gadget = qodec.Gadget(
        instruction,
        Circuit(target, "opaque", format="custom"),
        outputs=[
            Encoding(code, support=["left", "right"], block_types=["pair", "single"]),
        ],
    )
    layer = qodec.Layer(source, codes={"data": code}, gadgets=[gadget])

    class Body:
        def required_resources(self, invocation):
            return Resources(blocks={"pair": 1, "single": 1})

        def run(self, invocation):
            yield from ()
            return ()

    plan = LayerPlan(
        layer, prepare_circuit=lambda circuit: PreparedCircuit(("left", "right"), Body)
    )
    runtime = LayerRuntime(plan, RecordingDecoder())
    assert runtime.required_resources(Resources(blocks={"data": 1})) == Resources(
        blocks={"pair": 1, "single": 1}
    )
    runtime.start(Resources(blocks={"single": 2}))
    try:
        with pytest.raises(ValueError, match="capacity"):
            drive(
                runtime.handle(InstructionCall("prepare", operands=[0])),
                lambda request: (),
            )
        assert runtime.layout.blocks == {}
    finally:
        runtime.close()


@requires_stim
def test_failed_body_invalidates_the_layer_until_closed():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    runtime = LayerRuntime(LayerPlan(layer), RecordingDecoder())
    runtime.start(runtime.required_resources(Resources(qubits=1)))

    def fail(request):
        raise RuntimeError("lower execution failed")

    try:
        with pytest.raises(RuntimeError, match="lower execution failed"):
            drive(runtime.handle(InstructionCall("prepare_z", operands=[0])), fail)
        with pytest.raises(RuntimeError, match="failed invocation"):
            drive(
                runtime.handle(InstructionCall("prepare_z", operands=[0])),
                lambda request: (),
            )
    finally:
        runtime.close()


def test_discard_preserves_the_type_of_a_lower_block():
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.quantum_operations import LogicalSlot, Operation

    code = qodec.Code("pair", [], ["X_0", "X_1"], ["Z_0", "Z_1"])
    prepare = qodec.Instruction("prepare", outputs=[BlockOperand("data")])
    lower_prepare = qodec.Instruction("P", outputs=[BlockOperand("pair")])
    source = qodec.InstructionSet(
        "source", blocks=[Block("data", 2)], instructions=[prepare]
    )
    target = qodec.InstructionSet(
        "target", blocks=[Block("pair", 2)], instructions=[lower_prepare]
    )
    gadget = qodec.Gadget(
        prepare,
        Circuit(target, "[{P: [0]}]", format="yaml"),
        outputs=[Encoding(code, support=["0"], block_types=["pair"])],
    )
    runtime = LayerRuntime(
        LayerPlan(qodec.Layer(source, codes={"data": code}, gadgets=[gadget])),
        RecordingDecoder(),
    )
    runtime.start(runtime.required_resources(Resources(blocks={"data": 1})))
    emitted = []
    try:
        drive(
            runtime.handle(InstructionCall("prepare", operands=[0])), lambda request: ()
        )
        drive(runtime.discard(0), lambda request: emitted.append(request) or ())
        assert emitted == [Operation("discard", (LogicalSlot(0, 0, "pair"),))]
        assert runtime.layout.blocks == {}
    finally:
        runtime.close()
