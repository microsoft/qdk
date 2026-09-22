from . import FIXTURES
from contextlib import closing

import pytest
import qodec
from qodec.actions import Observe, Pauli, Rotate
from qodec.gadgets import Circuit
from qodec.instructions import Block, BlockOperand, InstructionCall, Parameter

from qdk.simulation._qodec.circuit_runtime import prepare_call_list
from qdk.simulation._qodec.protocols import Invocation


def instruction_call(request) -> InstructionCall:
    assert isinstance(request, InstructionCall)
    return request


def invocation(circuit, parameters=(), arguments=None, bindings=None):
    instruction = qodec.Instruction("body", parameters=list(parameters))
    gadget = qodec.Gadget(instruction, circuit, parameter_bindings=bindings or {})
    return Invocation(
        0, gadget, InstructionCall("body", arguments=arguments or {}), (), ()
    )


def isa():
    operand = BlockOperand("wire")
    return qodec.InstructionSet(
        "test",
        blocks=[Block("wire", 1)],
        instructions=[
            qodec.Instruction(
                "read",
                inputs=[operand],
                outputs=[operand],
                action=[Observe(["Z_0"])],
                flags=["reject"],
            ),
            qodec.Instruction(
                "rotate",
                inputs=[operand],
                outputs=[operand],
                parameters=[Parameter("theta", "number")],
                action=[Rotate("Z_0", "theta")],
            ),
            qodec.Instruction(
                "conditional",
                inputs=[operand],
                outputs=[operand],
                parameters=[Parameter("bit", "bit")],
                action=[Pauli("X_0")],
            ),
        ],
    )


def test_explicit_parameter_forwarding_and_fixed_values():
    circuit = Circuit(
        isa(),
        "[{rotate: [0, theta: angle]}, {rotate: [0, theta: 0.25]}]",
        format="yaml",
    )
    prepared = prepare_call_list(circuit)
    supplied = invocation(
        circuit, [Parameter("theta", "number")], {"theta": 0.5}, {"theta": "angle"}
    )
    with closing(prepared.create_runtime().run(supplied)) as requests:
        assert instruction_call(next(requests)).arguments == {"theta": 0.5}
        assert instruction_call(requests.send(())).arguments == {"theta": 0.25}
        with pytest.raises(StopIteration) as stopped:
            requests.send(())
        assert stopped.value.value == ()


def test_unforwarded_instruction_parameters_are_not_circuit_parameters():
    circuit = Circuit(isa(), "[{rotate: [0, theta: theta]}]", format="yaml")
    supplied = invocation(circuit, [Parameter("theta", "number")], {"theta": 0.5})
    with closing(prepare_call_list(circuit).create_runtime().run(supplied)) as requests:
        with pytest.raises(TypeError, match="expects number"):
            next(requests)


@pytest.mark.parametrize("selector", ["1", "1:2"])
def test_child_flags_occupy_the_same_record_as_outcomes(selector):
    circuit = Circuit(
        isa(),
        f'- read: [0]\n- conditional: [0, bit: "circuit.readouts[{selector}]"]',
        format="yaml",
    )
    with closing(
        prepare_call_list(circuit).create_runtime().run(invocation(circuit))
    ) as requests:
        assert instruction_call(next(requests)).mnemonic == "read"
        assert instruction_call(requests.send((False, True))).arguments == {"bit": True}
        with pytest.raises(StopIteration) as stopped:
            requests.send(())
        assert stopped.value.value == (False, True)


@pytest.mark.parametrize("reply", [(), (False,), (False, True, False)])
def test_child_record_count_is_checked_before_the_next_call(reply):
    circuit = Circuit(isa(), "[{read: [0]}, {rotate: [0, theta: 0.25]}]", format="yaml")
    with closing(
        prepare_call_list(circuit).create_runtime().run(invocation(circuit))
    ) as requests:
        next(requests)
        with pytest.raises(ValueError, match="records"):
            requests.send(reply)


@pytest.mark.parametrize(
    "source",
    [
        '- conditional: [0, bit: "circuit.readouts[0]"]',
        '- read: [0]\n- rotate: [0, theta: "circuit.readouts[0]"]',
        '- read: [0]\n- conditional: [0, bit: "circuit.readouts[2]"]',
        '- read: [0]\n- conditional: [0, bit: "circuit.readouts[0:2]"]',
        '- read: [0]\n- conditional: [0, bit: "circuit.readouts[0:0]"]',
    ],
)
def test_record_arguments_require_earlier_records_and_bit_parameters(source):
    circuit = Circuit(isa(), source, format="yaml")
    with pytest.raises(ValueError, match="[Rr]eadout"):
        prepare_call_list(circuit)


def test_unresolved_record_argument_is_an_unresolved_shot_failure():
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    circuit = Circuit(
        isa(),
        '- read: [0]\n- conditional: [0, bit: "circuit.readouts[0]"]',
        format="yaml",
    )
    with closing(
        prepare_call_list(circuit).create_runtime().run(invocation(circuit))
    ) as requests:
        assert instruction_call(next(requests)).mnemonic == "read"
        with pytest.raises(ExecutionUnresolved, match="unresolved readout"):
            requests.send((None, False))


@pytest.mark.parametrize(
    "patterns, flags, expected",
    [
        ([], (None, None), True),
        ([{"first": 0}], (False, None), True),
        ([{"first": 0}], (True, False), False),
        ([{"first": 0}], (None, False), None),
        ([{"first": 0, "second": 0}], (None, True), False),
        ([{"first": 0}, {"second": 1}], (None, True), True),
        ([{"flags[0]": 0, "second": 1}], (False, True), True),
        ([{}], (None, None), True),
    ],
)
def test_selection_is_decisive_three_valued_or_of_and(patterns, flags, expected):
    from qdk.simulation._qodec.selection import prepare_selection

    select = prepare_selection(("first", "second"), patterns)
    assert select.accepts(flags) is expected


@pytest.mark.parametrize(
    "patterns", [[{"missing": 0}], [{"flags[2]": 0}], [{"reject": 0, "flags[0]": 1}]]
)
def test_selection_rejects_invalid_flag_references(patterns):
    from qdk.simulation._qodec.selection import prepare_selection

    with pytest.raises(ValueError, match="flag"):
        prepare_selection(("reject",), patterns)


@pytest.mark.parametrize("flag, error", [(True, "rejected"), (None, "unresolved")])
def test_selected_child_stops_dependent_work(flag, error):
    from qdk.simulation._qodec.protocols import ExecutionRejected, ExecutionUnresolved

    circuit = Circuit(
        isa(),
        "- read: {operands: [0], select: [{reject: 0}]}\n- rotate: [0, theta: 0.25]",
        format="yaml",
    )
    with closing(
        prepare_call_list(circuit).create_runtime().run(invocation(circuit))
    ) as requests:
        assert instruction_call(next(requests)).mnemonic == "read"
        with pytest.raises(
            ExecutionRejected if flag else ExecutionUnresolved, match=error
        ):
            requests.send((False, flag))


def test_selected_child_passes_raw_flags_to_later_bit_parameters():
    circuit = Circuit(
        isa(),
        '- read: {operands: [0], select: [{reject: 1}]}\n- conditional: [0, bit: "circuit.readouts[1]"]',
        format="yaml",
    )
    with closing(
        prepare_call_list(circuit).create_runtime().run(invocation(circuit))
    ) as requests:
        next(requests)
        assert instruction_call(requests.send((False, True))).arguments == {"bit": True}
        with pytest.raises(StopIteration) as stopped:
            requests.send(())
        assert stopped.value.value == (False, True)


def qir_circuit(source):
    import qdk
    import qdk.openqasm
    from qdk.simulation._simulation import preprocess_simulation_input

    qir = qdk.openqasm.compile(source, target_profile=qdk.TargetProfile.Adaptive)
    module, _, _, _ = preprocess_simulation_input(qir)
    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    return Circuit(target, str(module), format="qir")


@pytest.mark.parametrize("measured", [False, True])
def test_adaptive_qir_gadget_preserves_inputs_and_uses_returned_readouts(measured):
    from qdk.simulation._qodec.circuit_runtime import prepare_circuit
    from qdk.simulation._qodec.quantum_operations import Operation

    circuit = qir_circuit(
        'include "stdgates.inc"; qubit[2] data; bit first = measure data[0]; if (first) { x data[1]; } bit second = measure data[1];'
    )
    prepared = prepare_circuit(circuit)
    assert prepared.labels == ("0", "1")
    for _ in range(2):
        with closing(prepared.create_runtime().run(invocation(circuit))) as requests:
            assert next(requests) == Operation("measure", (0,))
            request = requests.send((measured,))
            if measured:
                assert request == Operation("x", (1,))
                request = requests.send(())
            assert request == Operation("measure", (1,))
            with pytest.raises(StopIteration) as stopped:
                requests.send((True,))
            assert stopped.value.value == (measured, True)


def test_qir_gadget_invokes_lower_layer_without_resetting_live_inputs():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qodec.actions import Pauli
    from .test_execution_pipeline import drive_requests
    from qdk.simulation._qodec.protocols import Resources
    from qdk.simulation._qodec.quantum_operations import Operation

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    gadget = layer.gadgets["x"]
    gadget.circuit = qir_circuit(
        'include "stdgates.inc"; qubit[3] data; x data[0]; x data[1]; x data[2];'
    )
    runtime = LayerRuntime(LayerPlan(layer), prepare_syndrome_decoder(layer)(7))
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    emitted = []
    try:
        drive_requests(runtime.prepare(0), lambda request: ())
        drive_requests(
            runtime.apply("x", (0,)), lambda request: emitted.append(request) or ()
        )
        assert emitted == [
            Operation("x", (0,)),
            Operation("x", (1,)),
            Operation("x", (2,)),
        ]
    finally:
        runtime.close()


def test_adaptive_gadget_loop_uses_fresh_measurements_and_local_state():
    from qdk.simulation._qodec.circuit_runtime import prepare_circuit
    from qdk.simulation._qodec.quantum_operations import Operation, RestoreMeasured

    circuit = qir_circuit(
        'OPENQASM 3.0; include "stdgates.inc"; qubit data; for int count in [0:2] { bit readout = measure data; if (readout) { x data; } }'
    )
    prepared = prepare_circuit(circuit)
    for _ in range(2):
        with closing(prepared.create_runtime().run(invocation(circuit))) as requests:
            request = next(requests)
            for index in range(3):
                assert request == Operation("measure", (0,))
                assert requests.send((True,)) == RestoreMeasured(0, True)
                assert requests.send(()) == Operation("x", (0,))
                if index < 2:
                    request = requests.send(())
                else:
                    with pytest.raises(StopIteration):
                        requests.send(())


def test_openqasm_gadget_preserves_register_labels_and_decoded_feedback():
    from qdk.simulation._qodec.circuit_runtime import prepare_circuit
    from qdk.simulation._qodec.quantum_operations import LogicalSlot, Operation

    circuit = Circuit(
        isa(),
        'OPENQASM 3.0; include "stdgates.inc"; qubit[2] data; qubit ancilla; bit first = measure data[1]; if (first) { x data[0]; } bit second = measure ancilla;',
        format="openqasm",
    )
    prepared = prepare_circuit(circuit)
    assert prepared.labels == ("data[0]", "data[1]", "ancilla")
    with closing(prepared.create_runtime().run(invocation(circuit))) as requests:
        assert next(requests) == Operation(
            "measure", (LogicalSlot("data[1]", 0, "wire"),)
        )
        assert requests.send((True,)) == Operation(
            "x", (LogicalSlot("data[0]", 0, "wire"),)
        )
        assert requests.send(()) == Operation(
            "measure", (LogicalSlot("ancilla", 0, "wire"),)
        )
        with pytest.raises(StopIteration) as stopped:
            requests.send((False,))
        assert stopped.value.value == (True, False)


def test_direct_physical_calls_validate_selection_before_work():
    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from .test_layer_layout import drive

    target = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    runtime = InstructionRuntime(InstructionSet(target))
    emitted = []
    with pytest.raises(ValueError, match="Unknown selection flag"):
        drive(
            runtime.handle(InstructionCall("R", operands=[0], select=[{"reject": 0}])),
            lambda request: emitted.append(request) or (),
        )
    assert emitted == []
