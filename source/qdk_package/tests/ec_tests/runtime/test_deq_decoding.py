import asyncio
from itertools import product

import pytest
import pyqir
import pyqir.rt
import qodec
from qodec import actions

from qdk import Result
from qdk.simulation import NoiseConfig
from qdk.simulation._qodec.decoding import prepare_deq_decoder
from qdk.simulation._qodec.native_batch import prepare_batch
from . import FIXTURES
from .test_execution_pipeline import compile_qasm
from .test_native_batch import make_factory


@pytest.fixture(params=[32, 1024])
def circuit_decoder(request, monkeypatch):
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec import deq_composition

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", request.param)
    return prepare_deq_decoder


def circuit_batch(circuit_decoder, noise=None, codec=None, program=None):
    if program is None:
        program = compile_qasm(
            'include "stdgates.inc"; qubit data; bit result = measure data;'
        )
    elif isinstance(program, str):
        from qdk.simulation._simulation import preprocess_simulation_input
        from qdk.simulation._qodec.bytecode import compile

        module, _, _, _ = preprocess_simulation_input(program)
        program = compile(module, codec.layers[0].instruction_set.instructions)
    batch = prepare_batch(program, make_factory(noise, circuit_decoder, codec))
    assert batch is not None
    return batch


def decode_records(batch, rows, *, seed=42, policy="raise"):
    return asyncio.run(batch._decode(rows, seed, policy))


def qir_program(calls, outputs):
    qubit_count = 1 + max(index for _, qubits, _ in calls for index in qubits)
    result_count = 1 + max(index for _, _, results in calls for index in results)
    module = pyqir.SimpleModule("deq_tests", qubit_count, result_count)
    functions = {}
    for name, qubits, results in calls:
        operands = [module.qubits[index] for index in qubits] + [
            module.results[index] for index in results
        ]
        if name not in functions:
            functions[name] = module.add_external_function(
                name,
                pyqir.FunctionType(
                    pyqir.Type.void(module.context),
                    [operand.type for operand in operands],
                ),
            )
        module.builder.call(functions[name], operands)
    label = pyqir.Constant.null(pyqir.PointerType(pyqir.IntType(module.context, 8)))
    pyqir.rt.array_record_output(
        module.builder,
        pyqir.const(pyqir.IntType(module.context, 64), len(outputs)),
        label,
    )
    for index in outputs:
        pyqir.rt.result_record_output(module.builder, module.results[index], label)
    return module.ir()


def primitive_gadgets(batch, noise=None):
    from qdk.simulation._qodec.deq_decoding import _connected_library

    return _connected_library(batch.trace, noise)[2]


def repetition_with_parity_flag():
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.implements.flags = ["reject"]
    measure.readouts.append({"reject": ["circuit.readouts[0]", "circuit.readouts[1]"]})
    return codec


def unencoded_codec(width=1):
    from qodec.gadgets import Circuit, Encoding
    from qodec.instructions import Block, BlockOperand

    physical = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    for name, generators in (
        ("H", {"X_0": "Z_0", "Z_0": "X_0"}),
        ("S", {"X_0": "Y_0", "Z_0": "Z_0"}),
    ):
        physical.instructions[name] = qodec.Instruction(
            name,
            inputs=[BlockOperand("qubit")],
            outputs=[BlockOperand("qubit")],
            action=[actions.Clifford(generators)],
        )
    xs = [f"X_{index}" for index in range(width)]
    zs = [f"Z_{index}" for index in range(width)]
    encoding = Encoding(
        qodec.Code("Unencoded", [], xs, zs),
        support=[str(index) for index in range(width)],
    )
    operand = BlockOperand("data")
    prepare = qodec.Instruction(
        "prepare", outputs=[operand], action=[actions.Stabilize(zs)]
    )
    step = qodec.Instruction("step", inputs=[operand], outputs=[operand])
    measure = qodec.Instruction(
        "measure", inputs=[operand], action=[actions.Observe(zs)]
    )
    targets = " ".join(encoding.support)
    logical = qodec.InstructionSet(
        "Unencoded",
        blocks=[Block("data", width)],
        instructions=[prepare, step, measure],
    )
    return qodec.Qodec(
        [
            qodec.Layer(
                logical,
                codes={"data": encoding.code},
                gadgets=[
                    qodec.Gadget(
                        prepare,
                        Circuit(physical, f"R {targets}", format="stim"),
                        outputs=[encoding],
                    ),
                    qodec.Gadget(
                        step,
                        Circuit(physical, "", format="stim"),
                        inputs=[encoding],
                        outputs=[encoding],
                    ),
                    qodec.Gadget(
                        measure,
                        Circuit(physical, f"M {targets}", format="stim"),
                        inputs=[encoding],
                        readouts=[
                            [f"circuit.readouts[{index}]"] for index in range(width)
                        ],
                    ),
                ],
            ),
            qodec.Layer(physical),
        ]
    )


def physical_faults(trace, noise):
    from qdk._native import QirInstructionId
    from qdk.simulation._qodec.deq_decoding import _channel
    from qdk.simulation._qodec.native_batch import _FrameMasks, _GATES

    masks = _FrameMasks(trace.instructions)
    for position, (opcode, *operands) in enumerate(trace.instructions):
        if opcode in (QirInstructionId.MZ, QirInstructionId.RESET):
            name, targets = "mresetz", operands[:1]
        else:
            name = next(name for name, value in _GATES.items() if value == opcode)
            targets = operands
        for axes, probability in _channel(noise, name, len(targets)):
            effect = 0
            for target, axis in zip(targets, axes):
                if axis != "I":
                    effect ^= masks.mask(position + 1, target, axis.lower())
            if effect:
                yield effect, probability


def test_circuit_deq_never_constructs_or_replays_syndrome_decoder(
    circuit_decoder, monkeypatch
):
    from qdk.simulation._qodec.decoding import SyndromeModel, SyndromeSession
    from qdk.simulation._qodec.readout_equations import BinarySystem

    def forbidden(*args, **kwargs):
        pytest.fail("deq must not use the QDK syndrome/parity decoder")

    monkeypatch.setattr(SyndromeModel, "__init__", forbidden)
    monkeypatch.setattr(SyndromeSession, "decode", forbidden)
    monkeypatch.setattr(BinarySystem, "reduce", forbidden)
    program = compile_qasm(
        'include "stdgates.inc"; qubit[2] data; x data[1]; bit[2] result = measure data;'
    )
    batch = circuit_batch(circuit_decoder, program=program)
    assert batch.run(8, None, seed=9) == [[Result.Zero, Result.One]] * 8


def test_circuit_deq_records_once_without_constructing_a_replay_batch(
    circuit_decoder, monkeypatch
):
    from qdk.simulation._qodec import native_batch

    def forbidden(*args, **kwargs):
        pytest.fail("deq must consume a trace, not a replay decoder")

    recordings = []
    record = native_batch._trace

    def trace(*args):
        recordings.append(args[0])
        return record(*args)

    monkeypatch.setattr(native_batch, "ReplayBatch", forbidden)
    monkeypatch.setattr(native_batch, "_trace", trace)
    batch = circuit_batch(circuit_decoder)
    assert len(recordings) == 1
    assert not hasattr(batch.trace, "create_session")
    assert batch.run(3, None, seed=9) == [[Result.Zero]] * 3


def test_circuit_deq_connects_reusable_local_gadget_types(circuit_decoder):
    from qdk.simulation._qodec.deq_decoding import _connected_library

    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; x data; bit r = measure data;'
    )
    batch = circuit_batch(circuit_decoder, program=program)
    _, artifacts, gadgets = _connected_library(batch.trace, None)
    local_library = artifacts.jit_library
    assert len(local_library.gadget_types) == 3
    assert len(gadgets) == 4
    assert gadgets[1].gtype == gadgets[2].gtype
    assert [gadget.connectors for gadget in gadgets] == [
        (),
        ((1, 0),),
        ((2, 0),),
        ((3, 0),),
    ]
    assert [gadget.width for gadget in gadgets] == [0, 0, 0, 3]
    assert batch.run(3, None, seed=9) == [[Result.Zero]] * 3


def test_composition_reuses_primitives_without_patching_or_recompiling(
    circuit_decoder, monkeypatch
):
    from qdk.simulation._qodec import deq_decoding, deq_composition

    compiled = []
    original_models = []
    compositions = []
    build = deq_decoding.build_jit_library_artifacts
    compose = deq_composition.transpile_compose_jit_gadget_type
    contracts = []
    compile_contract = deq_decoding._local_contract

    def record_contract(event):
        key = (id(event.invocation.gadget), event.width)
        assert key not in contracts
        contracts.append(key)
        return compile_contract(event)

    def build_primitives(*args, **kwargs):
        from deq.circuit.model import (
            CodeDefinition,
            GadgetDefinition,
            OutputPort,
            PropagateStatement,
        )

        (source,) = args
        codes = {
            definition.name: definition
            for definition in source.definitions
            if isinstance(definition, CodeDefinition)
        }
        for definition in source.definitions:
            if isinstance(definition, GadgetDefinition):
                outputs = [
                    item for item in definition.body if isinstance(item, OutputPort)
                ]
                assert sum(
                    isinstance(item, PropagateStatement) for item in definition.body
                ) == sum(2 * len(codes[port.code_name].logicals) for port in outputs)
        artifacts = build(*args, **kwargs)
        compiled.append(artifacts)
        original_models.extend(
            kind.SerializeToString() for kind in artifacts.jit_library.gadget_types
        )
        return artifacts

    def compose_primitives(definition, **kwargs):
        (artifacts,) = compiled
        assert (
            kwargs["jit_gadget_artifacts_by_name"] is artifacts.gadget_artifacts_by_name
        )
        for kind in artifacts.jit_library.gadget_types:
            assert artifacts.gadget_artifacts_by_name[kind.base.name].jit_type == kind
        compositions.append(definition)
        return compose(definition, **kwargs)

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", 1)
    monkeypatch.setattr(deq_decoding, "_local_contract", record_contract)
    monkeypatch.setattr(deq_decoding, "build_jit_library_artifacts", build_primitives)
    monkeypatch.setattr(
        deq_composition, "transpile_compose_jit_gadget_type", compose_primitives
    )
    program = compile_qasm(
        'include "stdgates.inc"; qubit[2] data; '
        "x data[1]; x data[1]; x data[1]; bit[2] result = measure data;"
    )
    batch = circuit_batch(
        circuit_decoder, codec=repetition_with_parity_flag(), program=program
    )
    (artifacts,) = compiled
    assert all(
        original == kind.SerializeToString()
        for original, kind in zip(
            original_models, artifacts.jit_library.gadget_types, strict=True
        )
    )
    assert len(compositions) == len(batch.library.gadget_types)
    assert len(compositions) < len(batch.composites)
    assert batch.run(3, None, seed=9) == [[Result.Zero, Result.One]] * 3
    assert len(contracts) == 3


@pytest.mark.parametrize(
    "action, body, basis, flips",
    [
        pytest.param([], "H 0", "z", (1,), id="declared-action-wins"),
        ([actions.Pauli("X_0")], "X 0", "z", (1,)),
        ([actions.Pauli("Y_0")], "Y 0", "x", (2,)),
        ([actions.Clifford({"X_0": "X_0", "Z_0": "-Z_0"})], "X 0", "z", (1,)),
        ([actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})], "H 0", "z", (2,)),
        ([actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})], "H 0", "x", (1,)),
        ([actions.Clifford({"X_0": "Y_0", "Z_0": "Z_0"})], "S 0", "x", (3,)),
        (
            [
                actions.Clifford({"X_0": "Y_0", "Z_0": "Z_0"}),
                actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"}),
            ],
            "S 0\nH 0",
            "z",
            (3,),
        ),
        (
            [actions.Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
            "CX 0 1",
            "z",
            (1, 5),
        ),
        (
            [actions.Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
            "CX 0 1",
            "x",
            (10, 8),
        ),
    ],
)
def test_circuit_deq_propagates_incoming_frames_through_actions(
    circuit_decoder, action, body, basis, flips
):
    from qodec.gadgets import Circuit

    width = len(flips)
    codec = unencoded_codec(width)
    gadgets = codec.layers[0].gadgets
    physical = codec.layers[-1].instruction_set
    gadgets["step"].implements.action = action
    gadgets["step"].circuit = Circuit(physical, body, format="stim")
    targets = " ".join(map(str, range(width)))
    if basis == "x":
        gadgets["measure"].implements.action = [
            actions.Observe([f"X_{index}" for index in range(width)])
        ]
        gadgets["measure"].circuit = Circuit(
            physical, f"H {targets}\nM {targets}", format="stim"
        )
    program = qir_program(
        [("prepare", [0], []), ("step", [0], []), ("measure", [0], list(range(width)))],
        list(range(width)),
    )
    for incoming in range(1 << (2 * width)):
        gadgets["prepare"].frames = {
            f"out[0].{axis}[{index}]": [1]
            for index in range(width)
            for offset, axis in enumerate(("z", "x"))
            if incoming >> (2 * index + offset) & 1
        }
        batch = circuit_batch(circuit_decoder, codec=codec, program=program)
        rows = batch.trace.sample(4, None, seed=13)
        expected = [
            [
                (
                    Result.One
                    if (bit == Result.One) ^ bool((incoming & mask).bit_count() % 2)
                    else Result.Zero
                )
                for bit, mask in zip(row, flips)
            ]
            for row in rows
        ]
        assert decode_records(batch, rows) == expected


@pytest.mark.parametrize("explicit_frames", [False, True])
def test_circuit_deq_preserves_teleportation_measurement_frames(
    circuit_decoder, explicit_frames
):
    from qodec.gadgets import Circuit, Encoding

    codec = unencoded_codec()
    step = codec.layers[0].gadgets["step"]
    step.circuit = Circuit(
        codec.layers[-1].instruction_set,
        "R 1 2\nH 1\nCX 1 2\nCX 0 1\nH 0\nM 0 1",
        format="stim",
    )
    step.outputs = [Encoding(step.outputs[0].code, support=["2"])]
    if explicit_frames:
        step.checks = [
            ["out[0].x[0]", "in[0].x[0]"],
            ["out[0].z[0]", "in[0].z[0]"],
        ]
        step.frames = {
            "out[0].x[0]": ["circuit.readouts[0]"],
            "out[0].z[0]": ["circuit.readouts[1]"],
        }
    program = qir_program(
        [("prepare", [0], []), ("step", [0], []), ("measure", [0], [0])], [0]
    )
    batch = circuit_batch(circuit_decoder, codec=codec, program=program)
    rows = batch.trace.sample(32, None, seed=13)
    assert {tuple(row[:2]) for row in rows} == set(
        product((Result.Zero, Result.One), repeat=2)
    )
    assert decode_records(batch, rows) == [[Result.Zero]] * len(rows)


def test_circuit_deq_preserves_stabilizer_contributions_to_logical_frames(
    circuit_decoder,
):
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = codec.layers[-1].instruction_set
    physical.instructions["H"] = (
        unencoded_codec().layers[-1].instruction_set.instructions["H"]
    )
    prepare = codec.layers[0].gadgets["prepare_z"]
    prepare.outputs[0].code.z = ["Z_0"]
    prepare.circuit = Circuit(physical, "R 0 1 2 3\nH 1\nCX 1 3\nM 3", format="stim")
    prepare.checks = [
        ["out[0].stabilizers[0]", "circuit.readouts[0]"],
        ["out[0].stabilizers[1]", "circuit.readouts[0]"],
        ["out[0].z[0]"],
    ]
    step = codec.layers[0].gadgets["idle"]
    # Z1 = logical Z * stabilizer 0, so the swap must carry its syndrome bit.
    step.circuit = Circuit(physical, "CX 0 1\nCX 1 0\nCX 0 1", format="stim")
    step.checks = [
        ["out[0].stabilizers[0]", "in[0].stabilizers[0]"],
        ["out[0].stabilizers[1]", "in[0].stabilizers[0]", "in[0].stabilizers[1]"],
    ]
    program = qir_program(
        [
            ("prepare_z", [0], []),
            ("idle", [0], []),
            ("__quantum__qis__m__body", [0], [0]),
        ],
        [0],
    )
    batch = circuit_batch(circuit_decoder, codec=codec, program=program)
    rows = batch.trace.sample(32, None, seed=0)
    assert {row[0] for row in rows} == {Result.Zero, Result.One}
    assert decode_records(batch, rows) == [[Result.Zero]] * len(rows)


def test_circuit_deq_specializes_propagation_for_bound_action_guards(circuit_decoder):
    from dataclasses import replace
    from deq.circuit.model import LogicalPauliTarget, PropagateStatement
    from qodec.instructions import InstructionCall, Parameter
    from qdk.simulation._qodec.deq_decoding import _LocalLibraryBuilder
    from qdk.simulation._qodec.native_batch import _Decode

    program = qir_program(
        [("prepare", [0], []), ("step", [0], []), ("measure", [0], [0])], [0]
    )
    batch = circuit_batch(circuit_decoder, codec=unencoded_codec(), program=program)
    event = next(
        event
        for event in batch.trace.events
        if isinstance(event, _Decode) and event.invocation.call.mnemonic == "step"
    )
    gadget = event.invocation.gadget
    gadget.implements.parameters = [Parameter("enabled", "bit")]
    gadget.implements.action = [
        actions.Clifford(
            {"X_0": "Z_0", "Z_0": "X_0"}, condition=actions.Condition(["enabled"])
        )
    ]
    builder = _LocalLibraryBuilder()
    name = builder.add_code(gadget.inputs[0].code, 1)
    source = [f"INPUT {name} 0", "H 0", f"OUTPUT {name} 0"]
    types = []
    for enabled in (False, True, False):
        invocation = replace(
            event.invocation,
            call=InstructionCall("step", operands=[0], arguments={"enabled": enabled}),
        )
        types.append(
            builder.add_gadget(source, replace(event, invocation=invocation))[0]
        )
    assert types == [1, 2, 1]
    for definition, axis in zip(builder.gadgets, ("X", "Z"), strict=True):
        row = next(
            item
            for item in definition.body
            if isinstance(item, PropagateStatement) and item.target.pauli == "X"
        )
        assert row.terms == [LogicalPauliTarget(axis, 0, "IN", 0)]


def test_circuit_deq_translates_propagation_without_qdk_parities(
    circuit_decoder, monkeypatch
):
    from deq.circuit.model import CodeDefinition, GadgetDefinition
    from qdk.simulation._qodec import deq_decoding
    from qdk.simulation._qodec.native_batch import _Decode

    codec = unencoded_codec()
    step = codec.layers[0].gadgets["step"]
    step.implements.action = [actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})]
    step.circuit.source = "H 0"
    program = qir_program(
        [("prepare", [0], []), ("step", [0], []), ("measure", [0], [0])], [0]
    )
    batch = circuit_batch(circuit_decoder, codec=codec, program=program)
    source, _, gadgets = deq_decoding._connected_library(batch.trace, None)
    codes = {
        definition.name: definition
        for definition in source.definitions
        if isinstance(definition, CodeDefinition)
    }
    definitions = [
        definition
        for definition in source.definitions
        if isinstance(definition, GadgetDefinition)
    ]
    event = next(
        event
        for event in batch.trace.events
        if isinstance(event, _Decode) and event.invocation.call.mnemonic == "step"
    )

    def forbidden(*args, **kwargs):
        pytest.fail("deq propagation must not round-trip through QDK parities")

    monkeypatch.setattr(deq_decoding, "Parity", forbidden)
    monkeypatch.setattr(deq_decoding, "_Parity", forbidden)
    declared = deq_decoding._action_propagations(event.invocation)
    completed = deq_decoding._complete_propagations(
        definitions[gadgets[1].gtype - 1], codes, declared
    )
    for statements in (declared, completed):
        assert {
            (str(statement.target), tuple(map(str, statement.terms)))
            for statement in statements
        } == {
            ("OUT0.LX0", ("IN0.LZ0",)),
            ("OUT0.LZ0", ("IN0.LX0",)),
        }


def test_composition_preserves_measurement_and_result_order(circuit_decoder):
    program = compile_qasm(
        'include "stdgates.inc"; qubit[3] data; bit[3] result; '
        "x data[0]; x data[2]; "
        "result[2] = measure data[0]; "
        "result[0] = measure data[1]; "
        "result[1] = measure data[2];"
    )
    batch = circuit_batch(circuit_decoder, program=program)
    assert batch.run(3, None, seed=9) == [[Result.Zero, Result.One, Result.One]] * 3


@pytest.mark.parametrize(
    "field, message",
    [
        ("inputs", "boundary port count"),
        ("outputs", "boundary port count"),
        ("measurements", "measurement or readout count"),
        ("readouts", "measurement or readout count"),
    ],
)
def test_composition_rejects_changed_record_or_port_counts(
    circuit_decoder, monkeypatch, field, message
):
    from qdk.simulation._qodec import deq_composition

    compose = deq_composition.transpile_compose_jit_gadget_type

    def wrong_count(*args, **kwargs):
        artifacts = compose(*args, **kwargs)
        getattr(artifacts.jit_type.base, field).add()
        return artifacts

    monkeypatch.setattr(
        deq_composition, "transpile_compose_jit_gadget_type", wrong_count
    )
    with pytest.raises(ValueError, match=message):
        circuit_batch(circuit_decoder)


def memory_batch(circuit_decoder, noise=None):
    qir = qir_program(
        [
            ("prepare_z", [0], []),
            ("prepare_z", [1], []),
            *(("idle", [block], []) for _ in range(8) for block in range(2)),
            ("__quantum__qis__m__body", [0], [0]),
            ("__quantum__qis__m__body", [1], [1]),
        ],
        [0, 1],
    )
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    return circuit_batch(circuit_decoder, noise, codec, qir)


def test_composition_reuses_open_port_types_and_resets_completed_shots(
    circuit_decoder, monkeypatch
):
    from contextlib import asynccontextmanager
    from types import SimpleNamespace
    from qdk.simulation._qodec import deq_decoding, deq_composition

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", 32)
    batch = memory_batch(circuit_decoder)
    gadgets = primitive_gadgets(batch)
    assert 1 < len(batch.composites) < len(gadgets)
    assert len(batch.library.gadget_types) < len(batch.composites)
    assert any(chunk.connectors for chunk in batch.composites)
    assert all(kind.base.is_free_hop is False for kind in batch.library.gadget_types)
    assert sorted(
        index for chunk in batch.composites for index in chunk.measurements
    ) == list(range(batch.trace.num_measurements))
    assert sum(chunk.readout_count for chunk in batch.composites) == sum(
        len(gadget.readouts) + gadget.checks for gadget in gadgets
    )
    resets = []
    original_runtime = deq_decoding.Runtime

    @asynccontextmanager
    async def runtime(**kwargs):
        assert kwargs["coordinator"] == "window"
        assert kwargs["coordinator_config"] == {
            "buffer_radius": 1,
            "lookahead_radius": 1,
        }
        async with original_runtime(**kwargs) as runtime:
            service = runtime.jit_controller

            async def reset(**flags):
                resets.append(flags)
                await service.reset(**flags)

            yield SimpleNamespace(
                jit_controller=SimpleNamespace(
                    load_library=service.load_library,
                    batch_execute=service.batch_execute,
                    batch_decode=service.batch_decode,
                    reset=reset,
                )
            )

    monkeypatch.setattr(deq_decoding, "Runtime", runtime)
    assert batch.run(257, None, seed=9) == [[Result.Zero, Result.Zero]] * 257
    assert resets == [{"reset_library": False, "reset_decoder_service": False}]


def test_composition_corrects_single_faults_across_chunks(circuit_decoder, monkeypatch):
    from qdk.simulation._qodec import deq_composition

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", 32)
    noise = NoiseConfig()
    noise.cx.xi = 0.01
    batch = memory_batch(circuit_decoder, noise)
    assert len(batch.composites) > 2
    masks = {mask for mask, _ in physical_faults(batch.trace, noise)}
    assert len(masks) > 10
    rows = [
        [
            Result.One if mask >> index & 1 else Result.Zero
            for index in range(batch.trace.num_measurements)
        ]
        for mask in sorted(masks)
    ]
    results = decode_records(batch, rows, seed=9)
    assert results == [[Result.Zero, Result.Zero]] * len(rows)


def test_composition_keeps_individually_oversized_gadgets_intact(
    circuit_decoder, monkeypatch
):
    from qdk.simulation._qodec import deq_composition

    monkeypatch.setattr(deq_composition, "_COMPOSITE_SIZE", 1)
    batch = memory_batch(circuit_decoder)
    assert [
        (chunk.measurements, chunk.readout_count) for chunk in batch.composites
    ] == [
        (
            tuple(range(gadget.start, gadget.start + gadget.width)),
            len(gadget.readouts) + gadget.checks,
        )
        for gadget in primitive_gadgets(batch)
    ]
    assert batch.run(3, None, seed=9) == [[Result.Zero, Result.Zero]] * 3


@pytest.mark.parametrize("source", ["x data;", "x data; reset data; x data;"])
def test_circuit_deq_closes_discarded_and_live_outputs(circuit_decoder, source):
    batch = circuit_batch(
        circuit_decoder,
        program=compile_qasm(f'include "stdgates.inc"; qubit data; {source}'),
    )
    types = {kind.base.gtype: kind.base for kind in batch.library.gadget_types}
    assert {
        connector for chunk in batch.composites for connector in chunk.connectors
    } == {
        (index, port)
        for index, chunk in enumerate(batch.composites, 1)
        for port in range(len(types[chunk.gtype].outputs))
    }
    assert batch.run(3, None, seed=9) == [[]] * 3


def test_circuit_deq_does_not_add_undeclared_detection_checks(circuit_decoder):
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["__quantum__qis__m__body"].checks = []
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert all(not gadget.finished_checks for gadget in batch.library.gadget_types)


def test_circuit_deq_trusts_authored_check_sign_until_decoding(circuit_decoder):
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["__quantum__qis__m__body"].checks = [
        ["circuit.readouts[0]", "circuit.readouts[1]", 1]
    ]
    batch = circuit_batch(circuit_decoder, codec=codec)
    with pytest.raises(ExecutionUnresolved, match="explain"):
        batch.run(1, None, seed=9, on_shot_failure="raise")


def test_circuit_deq_preserves_a_pauli_before_a_logical_readout(circuit_decoder):
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.circuit = Circuit(
        codec.layers[-1].instruction_set, "X 0 1 2\nM 0 1 2", format="stim"
    )
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert batch.run(3, None, seed=9) == [[Result.One]] * 3


def test_circuit_deq_corrects_all_single_bit_errors(circuit_decoder):
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise)
    rows = [
        [Result.One if index == fault else Result.Zero for index in range(3)]
        for fault in range(3)
    ]
    results = decode_records(batch, rows, seed=9)
    assert results == [[Result.Zero]] * 3


def test_circuit_deq_preserves_raw_rejection_flags(circuit_decoder):
    from qdk.simulation._qodec.protocols import ExecutionRejected

    codec = repetition_with_parity_flag()
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise, codec)
    rows = [[Result.One, Result.Zero, Result.Zero], [Result.Zero] * 3]
    assert decode_records(batch, rows, seed=9, policy="discard") == [[Result.Zero]]
    with pytest.raises(ExecutionRejected):
        decode_records(batch, rows, seed=9)


def test_circuit_deq_skips_selected_raw_flags_only_when_discarding(
    circuit_decoder, monkeypatch
):
    from qdk.simulation._qodec import deq_decoding
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    codec = repetition_with_parity_flag()
    batch = circuit_batch(circuit_decoder, codec=codec)
    rows = [[Result.One, Result.Zero, Result.Zero]]
    with pytest.raises(ExecutionUnresolved, match="explain"):
        decode_records(batch, rows, seed=9)

    def forbidden(**kwargs):
        pytest.fail("Selected-out raw flags must not reach deq under discard")

    monkeypatch.setattr(deq_decoding, "Runtime", forbidden)
    assert decode_records(batch, rows, seed=9, policy="discard") == []


def test_circuit_deq_keeps_filtered_shots_aligned_across_batches(circuit_decoder):
    codec = repetition_with_parity_flag()
    batch = circuit_batch(circuit_decoder, codec=codec)
    rows = [
        [Result.Zero] * 3,
        [Result.One, Result.Zero, Result.Zero],
        [Result.Zero] * 3,
    ] * 257
    assert (
        decode_records(batch, rows, seed=9, policy="discard") == [[Result.Zero]] * 514
    )


@pytest.mark.parametrize("probability", [0, 0.01, 0.75, 1])
@pytest.mark.parametrize("shots", [20, 257])
def test_circuit_deq_seeded_runs_are_reproducible(circuit_decoder, probability, shots):
    noise = NoiseConfig()
    noise.mresetz.x = probability
    batch = circuit_batch(circuit_decoder, noise)
    first = batch.run(shots, noise, seed=42)
    assert first == batch.run(shots, noise, seed=42)
    assert len(first) == shots
    if probability == 0:
        assert first == [[Result.Zero]] * shots


def test_circuit_deq_requires_explicit_supported_execution(circuit_decoder):
    from qdk.simulation import run_qir
    import qdk.openqasm
    from qdk import TargetProfile

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit data; bit result = measure data;',
        target_profile=TargetProfile.Adaptive,
    )
    for options in ({"type": "cpu"}, {"on_shot_failure": "retry"}):
        with pytest.raises(NotImplementedError, match="deq"):
            run_qir(qir, qodec=codec, decoder=circuit_decoder, **options)
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit r = measure data; if (r) { x data; }'
    )
    with pytest.raises(NotImplementedError, match="deq"):
        circuit_batch(circuit_decoder, program=program)


def test_circuit_deq_rejects_general_pauli_channels(circuit_decoder):
    noise = NoiseConfig()
    noise.mresetz.x = 0.02
    noise.mresetz.z = 0.01
    with pytest.raises(NotImplementedError, match="general Pauli channels"):
        circuit_batch(circuit_decoder, noise)


@pytest.mark.parametrize("width", [1, 2])
def test_circuit_deq_depolarizing_mechanisms_reproduce_the_channel(
    circuit_decoder, width
):
    from qdk.simulation._qodec.deq_decoding import _channel

    noise = NoiseConfig()
    name = "x" if width == 1 else "cx"
    getattr(noise, name).set_depolarizing(0.12)
    mechanisms = _channel(noise, name, width)
    # Convolve the independent mechanisms over the Pauli group, ignoring phase.
    distribution = {("I",) * width: 1.0}
    for axes, probability in mechanisms:
        updated = {}
        for previous, weight in distribution.items():
            following = tuple(
                "IXYZ"["IXYZ".index(a) ^ "IXYZ".index(b)]
                for a, b in zip(previous, axes)
            )
            updated[previous] = updated.get(previous, 0.0) + weight * (1 - probability)
            updated[following] = updated.get(following, 0.0) + weight * probability
        distribution = updated
    for axes in product("IXYZ", repeat=width):
        expected = 0.88 if axes == ("I",) * width else 0.12 / (4**width - 1)
        assert distribution[axes] == pytest.approx(expected)


def test_circuit_deq_aliases_and_constants_are_compiled_once(circuit_decoder):
    from qdk.simulation._qodec.deq_decoding import _Parity, _eliminate_aliases
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    # r0 = r1; r1 = m0 XOR 1.
    readouts, checks = _eliminate_aliases([_Parity(0b110), _Parity(0b101, True)], 1, 2)
    assert readouts == [_Parity(1, True), _Parity(1, True)]
    assert checks == []
    with pytest.raises(ExecutionUnresolved, match="underdetermined"):
        _eliminate_aliases([_Parity(0b110)], 1, 2)


@pytest.mark.parametrize("record_dependent", [False, True])
def test_circuit_deq_applies_declared_frames_once(circuit_decoder, record_dependent):
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    prepare = codec.layers[0].gadgets["prepare_z"]
    if record_dependent:
        prepare.circuit = Circuit(
            codec.layers[-1].instruction_set,
            "R 0 1 2 3\nX 3\nM 3",
            format="stim",
        )
    prepare.frames = {
        "out[0].z[0]": ["circuit.readouts[0]"] if record_dependent else [1]
    }
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert batch.run(10, None, seed=42) == [[Result.One]] * 10


def test_circuit_deq_handles_random_preparation_syndromes(circuit_decoder):
    from qodec.actions import Clifford
    from qodec.gadgets import Circuit
    from qodec.instructions import BlockOperand

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = codec.layers[-1].instruction_set
    physical.instructions["H"] = qodec.Instruction(
        "H",
        inputs=[BlockOperand("qubit")],
        outputs=[BlockOperand("qubit")],
        action=[Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
    )
    prepare = codec.layers[0].gadgets["prepare_z"]
    prepare.circuit = Circuit(physical, "R 0 1 2 3\nH 1\nCX 1 3\nM 3", format="stim")
    prepare.checks = [
        ["out[0].stabilizers[0]", "circuit.readouts[0]"],
        ["out[0].stabilizers[1]", "circuit.readouts[0]"],
        ["out[0].z[0]"],
    ]
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.implements.flags = ["reject"]
    measure.readouts.append({"reject": ["circuit.readouts[0]", "circuit.readouts[1]"]})
    batch = circuit_batch(circuit_decoder, codec=codec)
    assert (
        batch.run(100, None, seed=42, on_shot_failure="raise") == [[Result.Zero]] * 100
    )


def test_circuit_deq_logical_sign_equation_matches_an_authored_frame(circuit_decoder):
    from qodec.actions import Clifford
    from qodec.gadgets import Circuit
    from qodec.instructions import BlockOperand

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = codec.layers[-1].instruction_set
    physical.instructions["H"] = qodec.Instruction(
        "H",
        inputs=[BlockOperand("qubit")],
        outputs=[BlockOperand("qubit")],
        action=[Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
    )
    prepare = codec.layers[0].gadgets["prepare_z"]
    prepare.circuit = Circuit(physical, "R 0 1 2 3\nH 3\nM 3", format="stim")
    prepare.frames = {"out[0].z[0]": ["circuit.readouts[0]"]}
    expected = circuit_batch(circuit_decoder, codec=codec).run(30, None, seed=42)
    assert {row[0] for row in expected} == {Result.Zero, Result.One}
    prepare.frames = {}
    prepare.checks = [["out[0].z[0]", "circuit.readouts[0]"]]
    assert (
        circuit_batch(circuit_decoder, codec=codec).run(30, None, seed=42) == expected
    )


@pytest.mark.parametrize("aliased", [False, True])
def test_circuit_deq_resolves_coupled_and_aliased_logical_signs(
    circuit_decoder, aliased
):
    from qdk.simulation import run_qir

    codec = qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml"))
    prepare = codec.layers[0].gadgets["prepare_zz"]
    if aliased:
        prepare.readouts = [{"reject": ["out[0].z[1]", 1]}]
        prepare.checks += [["out[0].z[0]", 1], ["readouts[0]"]]
    else:
        prepare.checks += [
            ["out[0].z[0]", "out[0].z[1]"],
            ["out[0].z[1]", 1],
        ]
    qir = qir_program([("prepare_zz", [0], [0]), ("measure_zz", [0], [1, 2])], [1, 2])
    assert (
        run_qir(qir, qodec=codec, decoder=circuit_decoder, shots=3, seed=42)
        == [[Result.One, Result.One]] * 3
    )


def test_circuit_faults_follow_measurement_and_reset_timing(circuit_decoder):
    from dataclasses import replace
    from qdk._native import QirInstructionId as Id

    batch = circuit_batch(circuit_decoder)
    trace = replace(
        batch.trace,
        instructions=(
            (Id.RESET, 0),
            (Id.MZ, 0, 0),
            (Id.MZ, 0, 1),
            (Id.RESET, 0),
            (Id.MZ, 0, 2),
        ),
    )
    noise = NoiseConfig()
    noise.mresetz.x = 0.1
    # Reset faults flip subsequent measurements; measurement faults never
    # flip the bit just recorded, and do not survive the next reset.
    assert list(physical_faults(trace, noise)) == [
        (0b011, 0.1),
        (0b010, 0.1),
        (0b100, 0.1),
    ]


@pytest.mark.parametrize("width", [1, 2])
def test_circuit_deq_handles_depolarizing_boundaries(circuit_decoder, width):
    from qdk.simulation._qodec.deq_decoding import _channel

    name = "x" if width == 1 else "cx"
    noise = NoiseConfig()
    table = getattr(noise, name)
    table.set_depolarizing((4**width - 1) / 4**width)
    assert all(probability == 0.5 for _, probability in _channel(noise, name, width))
    table.set_depolarizing(1e-20)
    assert all(probability > 0 for _, probability in _channel(noise, name, width))
    table.set_depolarizing(0.99)
    with pytest.raises(NotImplementedError, match="not approximated"):
        _channel(noise, name, width)


@pytest.mark.parametrize("field", ["loss", "x"])
def test_circuit_deq_rejects_changed_noise_before_sampling(circuit_decoder, field):
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise)
    setattr(noise.mresetz, field, 0.02)
    with pytest.raises(ValueError, match="prepare a new"):
        batch.run(1, noise, seed=42)


def test_circuit_deq_surfaces_constant_contradictions(circuit_decoder):
    from qdk.simulation._qodec.readout_equations import InconsistentParity

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["prepare_z"].checks = [[1]]
    with pytest.raises(InconsistentParity):
        circuit_batch(circuit_decoder, codec=codec)


def test_circuit_deq_cannot_explain_impossible_evidence(circuit_decoder):
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    batch = circuit_batch(circuit_decoder)
    rows = [[Result.One, Result.Zero, Result.Zero], [Result.Zero] * 3]
    assert decode_records(batch, rows, seed=1, policy="discard") == [[Result.Zero]]
    with pytest.raises(ExecutionUnresolved, match="explain"):
        decode_records(batch, rows, seed=1)


def test_circuit_deq_crosses_batch_boundaries_without_state_leaks(circuit_decoder):
    noise = NoiseConfig()
    noise.mresetz.x = 0.1
    batch = circuit_batch(circuit_decoder, noise)
    results = batch.run(513, noise, seed=42)
    assert len(results) == 513
    assert results == batch.run(513, noise, seed=42)


def test_circuit_deq_works_in_a_notebook_event_loop(circuit_decoder):
    import asyncio

    async def run():
        return circuit_batch(circuit_decoder).run(2, None, seed=42)

    assert asyncio.run(run()) == [[Result.Zero]] * 2


def test_circuit_deq_preserves_bit_order_across_bytes(circuit_decoder):
    program = compile_qasm(
        'include "stdgates.inc"; qubit[9] data; '
        "x data[0]; x data[3]; x data[8]; bit[9] result = measure data;"
    )
    batch = circuit_batch(circuit_decoder, program=program)
    expected = [Result.One if index in (0, 3, 8) else Result.Zero for index in range(9)]
    assert batch.run(3, None, seed=42) == [expected] * 3


def test_circuit_deq_preserves_reused_blocks_and_discards(circuit_decoder):
    from .test_native_batch import _repetition_code_with_x_circuit

    codec = _repetition_code_with_x_circuit("X 0 1 2\nCX 3 0 3 1\nM 3")
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; x data; '
        "bit first = measure data; reset data; x data; bit last = measure data;"
    )
    batch = circuit_batch(circuit_decoder, codec=codec, program=program)
    assert batch.run(4, None, seed=42) == [[Result.Zero, Result.One]] * 4


@pytest.mark.parametrize("probability", [0.25, 0.5, 0.75, 1])
def test_circuit_deq_preserves_single_pauli_fault_probabilities(
    circuit_decoder, probability
):
    noise = NoiseConfig()
    noise.x.x = probability
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    )
    batch = circuit_batch(circuit_decoder, noise, program=program)
    assert [
        error.base.probability
        for gadget in batch.library.gadget_types
        for error in gadget.errors
    ] == [probability] * 3


def test_circuit_deq_corrects_data_and_intermediate_syndrome_faults(circuit_decoder):
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    gadget = codec.layers[0].gadgets["__quantum__qis__x__body"]
    gadget.circuit = Circuit(
        codec.layers[-1].instruction_set,
        "X 0 1 2\nR 3 4\nCX 0 3 1 3\nCX 1 4 2 4\nM 3 4",
        format="stim",
    )
    gadget.checks = codec.layers[0].gadgets["idle"].checks
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    )
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    batch = circuit_batch(circuit_decoder, noise, codec, program)
    clean = [False, False, True, True, True]
    rows = [
        [
            Result.One if value ^ bool(fault >> index & 1) else Result.Zero
            for index, value in enumerate(clean)
        ]
        for fault, _ in physical_faults(batch.trace, noise)
    ]
    assert len(rows) == 5
    assert decode_records(batch, rows) == [[Result.One]] * len(rows)


@pytest.mark.parametrize("kind", ["reset_loss", "gate_loss", "rotation", "layers"])
def test_circuit_deq_rejects_unsupported_traces(circuit_decoder, kind):
    noise = NoiseConfig()
    source = 'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    codec = None
    if kind == "reset_loss":
        noise.mresetz.loss = 0.01
    elif kind == "gate_loss":
        noise.x.loss = 0.01
    elif kind == "rotation":
        source = source.replace("x data", "rz(0.1) data")
    else:
        from .test_execution_pipeline import nested_repetition_qodec

        source = 'include "stdgates.inc";'
        codec = nested_repetition_qodec()
    with pytest.raises(NotImplementedError, match="deq"):
        circuit_batch(circuit_decoder, noise, codec, compile_qasm(source))


@pytest.mark.parametrize("basis", ["zz", "xx"])
@pytest.mark.parametrize("flagged", [False, True])
def test_circuit_deq_c4_transport_syndromes_and_returned_flags(
    circuit_decoder, basis, flagged
):
    from qdk.simulation import run_qir

    codec = qodec.Qodec.load(str(FIXTURES / "c4.qodec.yaml"))
    if flagged:
        circuit = codec.layers[0].gadgets[f"prepare_{basis}"].circuit
        circuit.source = circuit.source.replace("M 4", "X 4\nM 4")
    flip = "x0" if basis == "zz" else "z0"
    qir = qir_program(
        [
            (f"prepare_{basis}", [0], [0]),
            (f"prepare_{basis}", [1], [1]),
            (flip, [0], []),
            ("transversal_cx", [0, 1], []),
            ("idle", [0], []),
            ("idle", [1], []),
            (f"measure_{basis}", [0], [2, 3]),
            (f"measure_{basis}", [1], [4, 5]),
        ],
        range(6),
    )
    flag = Result.One if flagged else Result.Zero
    expected = [
        flag,
        flag,
        Result.One,
        Result.Zero,
        Result.One if basis == "zz" else Result.Zero,
        Result.Zero,
    ]
    assert (
        run_qir(qir, qodec=codec, decoder=circuit_decoder, shots=20, seed=42)
        == [expected] * 20
    )


@pytest.mark.parametrize(
    "kind", ["identifier", "size", "bytes", "count", "execute", "failure"]
)
def test_circuit_deq_does_not_discard_runtime_protocol_errors(
    circuit_decoder, monkeypatch, kind
):
    from contextlib import asynccontextmanager
    from types import SimpleNamespace
    from deq.proto import coordinator_pb2 as coordinator, util_pb2 as util
    from qdk.simulation._qodec import deq_decoding

    batch = circuit_batch(circuit_decoder)
    closed = []

    async def load_library(library):
        pass

    async def batch_execute(instructions):
        return [
            99 if kind == "execute" else instruction.gadget.gid
            for instruction in instructions
        ]

    async def batch_decode(outcomes):
        if kind == "failure":
            raise RuntimeError("native decoder failure")
        if kind == "count":
            return []
        widths = {
            kind.base.gtype: len(kind.base.readouts)
            for kind in batch.library.gadget_types
        }
        return [
            coordinator.Readouts(
                gid=99 if kind == "identifier" else outcome.gid,
                readouts=util.BitVector(
                    size=99 if kind == "size" else widths[gadget.gtype],
                    data=(
                        b""
                        if kind == "bytes"
                        else bytes((widths[gadget.gtype] + 7) // 8)
                    ),
                ),
            )
            for outcome, gadget in zip(outcomes, batch.composites)
        ]

    @asynccontextmanager
    async def runtime(**kwargs):
        try:
            yield SimpleNamespace(
                jit_controller=SimpleNamespace(
                    load_library=load_library,
                    batch_execute=batch_execute,
                    batch_decode=batch_decode,
                )
            )
        finally:
            closed.append(True)

    monkeypatch.setattr(deq_decoding, "Runtime", runtime)
    with pytest.raises(RuntimeError):
        decode_records(batch, [[Result.Zero] * 3], policy="discard")
    assert closed == [True]


@pytest.mark.parametrize("failure", ["start", "shutdown"])
def test_deq_releases_worker_after_runtime_failure(
    circuit_decoder, monkeypatch, failure
):
    from contextlib import asynccontextmanager
    from threading import current_thread
    from qdk.simulation._qodec import deq_decoding

    batch = circuit_batch(circuit_decoder)
    original_runtime = deq_decoding.Runtime
    workers = []
    error = RuntimeError("injected deq failure")

    @asynccontextmanager
    async def runtime(**options):
        workers.append(current_thread())
        if failure == "start":
            raise error
        async with original_runtime(**options) as instance:
            yield instance
        raise error

    monkeypatch.setattr(deq_decoding, "Runtime", runtime)
    with pytest.raises(RuntimeError) as raised:
        batch.run(1, None, seed=7)
    assert raised.value is error
    assert workers and all(not worker.is_alive() for worker in workers)


def test_deq_factory_has_only_a_layer_parameter():
    from inspect import Parameter, signature

    parameters = signature(prepare_deq_decoder).parameters
    assert tuple(parameters) == ("layer",)
    assert parameters["layer"].kind is Parameter.POSITIONAL_OR_KEYWORD
    assert parameters["layer"].default is Parameter.empty


@pytest.mark.parametrize(
    "options",
    [
        {"circuit_level": True},
        {"circuit_level": False},
        {"error_probability": None},
        {"error_probability": 0.01},
    ],
)
def test_deq_factory_rejects_removed_options(circuit_decoder, options):
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    with pytest.raises(TypeError, match="unexpected keyword argument"):
        circuit_decoder(codec.layers[0], **options)


def test_circuit_deq_empty_program_and_zero_shots(circuit_decoder):
    program = compile_qasm('include "stdgates.inc";')
    batch = circuit_batch(circuit_decoder, program=program)
    assert batch.run(0, None, seed=42) == []
    assert batch.run(3, None, seed=42) == [[], [], []]
    assert circuit_batch(circuit_decoder).run(0, None, seed=42) == []


def test_decoder_public_names_remain_unchanged():
    from qdk.simulation import decoders

    expected = {
        "BatchDecoderFactory",
        "BatchDecoderSession",
        "BatchUnsupported",
        "BlockReference",
        "Correction",
        "Corrections",
        "Decoded",
        "DecoderFactory",
        "DecoderSession",
        "ExecutionRejected",
        "ExecutionUnresolved",
        "Invocation",
        "LogicalSlot",
        "Operation",
        "PrepareDecoder",
        "ReadoutBatch",
        "ReadoutTable",
        "Readouts",
        "prepare_syndrome_decoder",
        "prepare_frame_decoder",
        "prepare_deq_decoder",
    }
    assert set(decoders.__all__) == expected
    assert {name for name in vars(decoders) if not name.startswith("_")} == expected
