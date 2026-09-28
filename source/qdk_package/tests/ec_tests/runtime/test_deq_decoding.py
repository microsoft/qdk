"""Exercise deq through run_qir; use internal seams for model and fault assertions."""

import asyncio
from functools import partial
from itertools import product
from typing import Literal

import pytest
import pyqir
import pyqir.rt
import qodec
import qdk.openqasm
from qodec import actions

from qdk import Result, TargetProfile
from qdk.simulation import NoiseConfig, run_qir
from qdk.simulation.decoders import (
    ExecutionRejected,
    ExecutionUnresolved,
    prepare_deq_decoder,
)
from . import FIXTURES


@pytest.fixture(params=[32, 1024])
def circuit_decoder(request, monkeypatch):
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec import deq_decoding

    monkeypatch.setattr(deq_decoding, "_COMPOSITE_SIZE", request.param)
    return prepare_deq_decoder


def compile_qasm(source):
    return qdk.openqasm.compile(source, target_profile=TargetProfile.Adaptive)


@pytest.fixture
def simulate(circuit_decoder):
    def run(
        program=None,
        *,
        codec=None,
        noise=None,
        shots=3,
        seed=42,
        runtime_factory=None,
        on_shot_failure: Literal["raise", "discard"] = "raise",
    ):
        if codec is None:
            codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        if program is None:
            program = qir_program(
                [("prepare_z", [0], []), ("__quantum__qis__m__body", [0], [0])], [0]
            )
        return run_qir(
            program,
            qodec=codec,
            decoder=partial(circuit_decoder, runtime_factory=runtime_factory),
            noise=noise,
            shots=shots,
            seed=seed,
            on_shot_failure=on_shot_failure,
        )

    return run


@pytest.fixture
def sample_records(monkeypatch):
    """Replace sampling, not the public compilation or decoding path."""
    from qdk.simulation._qodec.native_batch import CircuitTrace

    def inject(rows):
        def sample(trace, shots, noise, *, seed):
            assert shots == len(rows)
            assert all(len(row) == trace.num_measurements for row in rows)
            return rows

        monkeypatch.setattr(CircuitTrace, "sample", sample)

    return inject


@pytest.fixture
def physical_samples(monkeypatch):
    from qdk.simulation._qodec.native_batch import CircuitTrace

    rows = []
    sample = CircuitTrace.sample

    def record(trace, *args, **kwargs):
        rows[:] = sample(trace, *args, **kwargs)
        return rows

    monkeypatch.setattr(CircuitTrace, "sample", record)
    return rows


@pytest.fixture
def prepared_batches(monkeypatch):
    from qdk.simulation._qodec.deq_decoding import DeqModel

    batches = []
    prepare = DeqModel.prepare_circuit

    def record(*args):
        batch = prepare(*args)
        batches.append(batch)
        return batch

    monkeypatch.setattr(DeqModel, "prepare_circuit", record)
    return batches


@pytest.fixture
def compilations(monkeypatch):
    from qdk.simulation._qodec import deq_decoding

    compiled = []
    compose = deq_decoding._compose_gadgets

    def record(source, artifacts, instances):
        compiled.append((source, artifacts, instances))
        return compose(source, artifacts, instances)

    monkeypatch.setattr(deq_decoding, "_compose_gadgets", record)
    return compiled


def qir_program(calls, outputs):
    qubit_count = 1 + max(index for _, qubits, _ in calls for index in qubits)
    result_count = 1 + max(
        (index for _, _, results in calls for index in results), default=-1
    )
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
    from qdk.simulation._qodec.deq_conversion import _channel
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
    simulate, monkeypatch
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
    assert simulate(program, shots=8, seed=9) == [[Result.Zero, Result.One]] * 8


def test_circuit_deq_records_once_without_constructing_a_replay_batch(
    simulate, monkeypatch, prepared_batches
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
    assert simulate(seed=9) == [[Result.Zero]] * 3
    (batch,) = prepared_batches
    assert len(recordings) == 1
    assert not hasattr(batch.trace, "create_session")
    assert batch.trace.events == ()
    assert batch.trace.sources == ()
    assert batch.trace.outputs == ()


def test_deq_skips_replay_probes_and_reuses_layer_preparation(simulate, monkeypatch):
    from qdk.simulation._qodec import native_batch
    from qdk.simulation._qodec.layer_runtime import LayerPlan

    plans = []
    checked = []
    original_plan = LayerPlan.__init__
    original_check = native_batch._require_static_circuit

    def prepare(plan, *args, **kwargs):
        plans.append(plan)
        original_plan(plan, *args, **kwargs)

    def check(plan, operations, invocation):
        checked.append(invocation.call.mnemonic)
        original_check(plan, operations, invocation)

    def forbidden(*args, **kwargs):
        pytest.fail("deq tracing must not construct correction-replay metadata")

    monkeypatch.setattr(LayerPlan, "__init__", prepare)
    monkeypatch.setattr(native_batch, "_require_static_circuit", check)
    monkeypatch.setattr(native_batch._DeferringDecoder, "_probe", forbidden)
    assert simulate(memory_program()) == [[Result.Zero, Result.Zero]] * 3
    assert len(plans) == 1
    assert sorted(checked) == ["__quantum__qis__m__body", "idle", "prepare_z"]


def test_circuit_deq_connects_reusable_local_gadget_types(simulate, compilations):
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; x data; bit r = measure data;'
    )
    assert simulate(program, seed=9) == [Result.Zero] * 3
    ((_, artifacts, gadgets),) = compilations
    local_library = artifacts.jit_library
    assert len(local_library.gadget_types) == 3
    assert len(gadgets) == 4
    assert gadgets[1].model is gadgets[2].model
    assert [gadget.connectors for gadget in gadgets] == [
        (),
        ((1, 0),),
        ((2, 0),),
        ((3, 0),),
    ]
    assert [gadget.model.measurement_count for gadget in gadgets] == [0, 0, 0, 3]


def test_composition_reuses_primitives_without_patching_or_recompiling(
    simulate, monkeypatch, prepared_batches
):
    from qdk.simulation._qodec import deq_decoding, deq_conversion

    compiled = []
    original_models = []
    compositions = []
    build = deq_conversion.build_jit_library_artifacts
    compose = deq_decoding.transpile_compose_jit_gadget_type
    contracts = []
    compile_contract = deq_conversion._local_contract

    def record_contract(gadget, width, defaults):
        key = (id(gadget), width)
        assert key not in contracts
        contracts.append(key)
        return compile_contract(gadget, width, defaults)

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

    monkeypatch.setattr(deq_decoding, "_COMPOSITE_SIZE", 1)
    monkeypatch.setattr(deq_conversion, "_local_contract", record_contract)
    monkeypatch.setattr(deq_conversion, "build_jit_library_artifacts", build_primitives)
    monkeypatch.setattr(
        deq_decoding, "transpile_compose_jit_gadget_type", compose_primitives
    )
    program = compile_qasm(
        'include "stdgates.inc"; qubit[2] data; '
        "x data[1]; x data[1]; x data[1]; bit[2] result = measure data;"
    )
    assert (
        simulate(program, codec=repetition_with_parity_flag(), seed=9)
        == [[Result.Zero, Result.One]] * 3
    )
    (batch,) = prepared_batches
    (artifacts,) = compiled
    assert all(
        original == kind.SerializeToString()
        for original, kind in zip(
            original_models, artifacts.jit_library.gadget_types, strict=True
        )
    )
    assert len(compositions) == len(batch.library.gadget_types)
    assert len(compositions) < len(batch.composites)
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
    simulate, physical_samples, action, body, basis, flips
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
        results = simulate(program, codec=codec, shots=4, seed=13)
        expected = [
            [
                (
                    Result.One
                    if (bit == Result.One) ^ bool((incoming & mask).bit_count() % 2)
                    else Result.Zero
                )
                for bit, mask in zip(row, flips)
            ]
            for row in physical_samples
        ]
        assert results == expected


@pytest.mark.parametrize(
    "action, body, basis, inputs",
    [
        ([], "", "z", ["in[0].z[0]", "in[0].z[1]"]),
        ([], "", "x", ["in[0].x[0]", "in[0].x[1]"]),
        (
            [actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
            "H 0",
            "z",
            ["in[0].x[0]", "in[0].z[1]"],
        ),
        (
            [actions.Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
            "CX 0 1",
            "z",
            ["in[0].z[1]"],
        ),
        pytest.param(
            [actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
            "",
            "z",
            ["in[0].x[0]", "in[0].z[1]"],
            id="declared-action-not-physical-body",
        ),
    ],
)
@pytest.mark.parametrize("flip", [0, 1])
def test_partial_output_checks_complete_frames_from_declared_action(
    simulate, physical_samples, action, body, basis, inputs, flip
):
    from qdk.ec import GadgetProfile
    from qdk.ec._analysis.propagation.pauli import Pauli

    codec = unencoded_codec(2)
    gadgets = codec.layers[0].gadgets
    gadgets["step"].implements.action = action
    gadgets["step"].circuit.source = body
    gadgets["step"].checks = [
        [f"out[0].{basis}[0]", f"out[0].{basis}[1]", *inputs, flip]
    ]
    if basis == "x":
        gadgets["measure"].implements.action = [actions.Observe(["X_0", "X_1"])]
        gadgets["measure"].circuit.source = "H 0 1\nM 0 1"
    program = qir_program(
        [("prepare", [0], []), ("step", [0], []), ("measure", [0], [0, 1])],
        [0, 1],
    )
    # ChannelAction independently supplies the intended generator images.
    channel = GadgetProfile(gadgets["step"]).objective
    assert channel is not None
    images = [
        channel._mapping[Pauli({index: axis})].pauli
        for index in range(2)
        for axis in ("X", "Z")
    ]
    flips = [
        sum(
            1 << bit
            for bit, image in enumerate(images)
            if not image.commutes_with(Pauli({index: basis.upper()}))
        )
        for index in range(2)
    ]
    for incoming in (1, 2, 4, 8, 3, 12, 15):
        gadgets["prepare"].frames = {
            f"out[0].{axis}[{index}]": [1]
            for index in range(2)
            for offset, axis in enumerate(("z", "x"))
            if incoming >> (2 * index + offset) & 1
        }
        results = simulate(program, codec=codec, shots=3, seed=7)
        assert results == [
            [
                (
                    Result.One
                    if (bit == Result.One)
                    ^ bool((incoming & mask).bit_count() % 2)
                    ^ bool(index == 1 and flip)
                    else Result.Zero
                )
                for index, (bit, mask) in enumerate(zip(row, flips))
            ]
            for row in physical_samples
        ]


@pytest.mark.parametrize("controlled_x", [False, True])
def test_partial_output_checks_span_separate_code_ports(simulate, controlled_x):
    from qodec.gadgets import Encoding
    from qodec.instructions import BlockOperand

    codec = unencoded_codec()
    gadgets = codec.layers[0].gadgets
    step = gadgets["step"]
    step.implements.inputs = [BlockOperand("data"), BlockOperand("data")]
    step.implements.outputs = [BlockOperand("data"), BlockOperand("data")]
    code = codec.layers[0].codes["data"]
    step.inputs = [Encoding(code, support=[str(index)]) for index in range(2)]
    step.outputs = [Encoding(code, support=[str(index)]) for index in range(2)]
    inputs = ["in[1].z[0]"]
    if controlled_x:
        step.implements.action = [
            actions.Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})
        ]
        step.circuit.source = "CX 0 1"
    else:
        inputs.append("in[0].z[0]")
    step.checks = [["out[0].z[0]", "out[1].z[0]", *inputs]]
    gadgets["prepare"].frames = {"out[0].z[0]": [1]}
    program = qir_program(
        [
            ("prepare", [0], []),
            ("prepare", [1], []),
            ("step", [0, 1], []),
            ("measure", [0], [0]),
            ("measure", [1], [1]),
        ],
        [0, 1],
    )
    expected = [Result.One, Result.Zero if controlled_x else Result.One]
    assert simulate(program, codec=codec) == [expected] * 3


def test_partial_output_checks_keep_teleportation_byproducts(
    simulate, physical_samples
):
    from qodec.gadgets import Encoding

    codec = unencoded_codec(2)
    gadgets = codec.layers[0].gadgets
    gadgets["prepare"].frames = {"out[0].z[0]": [1]}
    step = gadgets["step"]
    step.circuit.source = "R 2 3\nH 2\nCX 2 3\nCX 0 2\nH 0\nM 0 2"
    step.outputs = [Encoding(step.outputs[0].code, support=["3", "1"])]
    step.checks = [
        [
            "out[0].z[0]",
            "out[0].z[1]",
            "in[0].z[0]",
            "in[0].z[1]",
            "circuit.readouts[1]",
        ]
    ]
    program = qir_program(
        [("prepare", [0], []), ("step", [0], []), ("measure", [0], [0, 1])],
        [0, 1],
    )
    assert (
        simulate(program, codec=codec, shots=32, seed=13)
        == [[Result.One, Result.Zero]] * 32
    )
    assert {tuple(row[:2]) for row in physical_samples} == set(
        product((Result.Zero, Result.One), repeat=2)
    )


@pytest.mark.parametrize("enabled", [False, True])
@pytest.mark.parametrize("kind", ["Clifford", "Stabilize", "Rotate"])
def test_deq_rejects_conditional_non_pauli_actions(simulate, enabled, kind):
    from qodec.instructions import Parameter

    codec = unencoded_codec(2)
    gadgets = codec.layers[0].gadgets
    gadgets["prepare"].frames = {"out[0].z[0]": [1]}
    step = gadgets["step"]
    step.implements.parameters = [Parameter("enabled", "bit")]
    condition = actions.Condition(["enabled"])
    step.implements.action = [
        {
            "Clifford": actions.Clifford(
                {"X_0": "Z_0", "Z_0": "X_0"}, condition=condition
            ),
            "Stabilize": actions.Stabilize(["Z_0"], condition=condition),
            "Rotate": actions.Rotate("Z_0", 0.25, condition=condition),
        }[kind]
    ]
    program = f"""
        %Qubit = type opaque
        %Result = type opaque
        define void @main() #0 {{
          call void @prepare(%Qubit* null)
          call void @step(%Qubit* null, i1 {str(enabled).lower()})
          call void @measure(%Qubit* null, %Result* null, %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* null)
          call void @__quantum__rt__result_record_output(%Result* null, i8* null)
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
          ret void
        }}
        declare void @prepare(%Qubit*)
        declare void @step(%Qubit*, i1)
        declare void @measure(%Qubit*, %Result*, %Result*)
        declare void @__quantum__rt__array_record_output(i64, i8*)
        declare void @__quantum__rt__result_record_output(%Result*, i8*)
        attributes #0 = {{ "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }}
    """
    with pytest.raises(NotImplementedError, match=f"conditional {kind}"):
        simulate(program, codec=codec)


def test_partial_stabilizer_checks_preserve_incoming_syndromes(
    simulate, physical_samples
):
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = codec.layers[-1].instruction_set
    physical.instructions["H"] = (
        unencoded_codec().layers[-1].instruction_set.instructions["H"]
    )
    gadgets = codec.layers[0].gadgets
    prepare = gadgets["prepare_z"]
    prepare.circuit.source = "R 0 1 2 3\nH 1\nCX 1 3\nM 3"
    prepare.checks = [
        ["out[0].stabilizers[0]", "circuit.readouts[0]"],
        ["out[0].stabilizers[1]", "circuit.readouts[0]"],
        ["out[0].z[0]"],
    ]
    gadgets["idle"].circuit.source = ""
    gadgets["idle"].checks = [
        [
            "out[0].stabilizers[0]",
            "out[0].stabilizers[1]",
            "in[0].stabilizers[0]",
            "in[0].stabilizers[1]",
        ]
    ]
    swap = gadgets["__quantum__qis__x__body"]
    swap.implements.action = []
    swap.circuit.source = "CX 0 1\nCX 1 0\nCX 0 1"
    swap.checks = [
        ["out[0].stabilizers[0]", "in[0].stabilizers[0]"],
        ["out[0].stabilizers[1]", "in[0].stabilizers[0]", "in[0].stabilizers[1]"],
    ]
    program = qir_program(
        [
            ("prepare_z", [0], []),
            ("idle", [0], []),
            ("__quantum__qis__x__body", [0], []),
            ("__quantum__qis__m__body", [0], [0]),
        ],
        [0],
    )
    assert simulate(program, codec=codec, shots=32, seed=0) == [[Result.Zero]] * 32
    assert {row[0] for row in physical_samples} == {Result.Zero, Result.One}


@pytest.mark.parametrize("explicit_frames", [False, True])
def test_circuit_deq_preserves_teleportation_measurement_frames(
    simulate, physical_samples, explicit_frames
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
    assert simulate(program, codec=codec, shots=32, seed=13) == [[Result.Zero]] * 32
    assert {tuple(row[:2]) for row in physical_samples} == set(
        product((Result.Zero, Result.One), repeat=2)
    )


def test_circuit_deq_preserves_stabilizer_contributions_to_logical_frames(
    simulate,
    physical_samples,
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
    assert simulate(program, codec=codec, shots=32, seed=0) == [[Result.Zero]] * 32
    assert {row[0] for row in physical_samples} == {Result.Zero, Result.One}


@pytest.mark.parametrize("invert", [False, True])
def test_circuit_deq_preserves_transport_for_conditional_paulis(
    circuit_decoder, invert
):
    from deq.circuit.model import LogicalPauliTarget, PropagateStatement
    from qodec.instructions import Parameter
    from qdk.simulation._qodec.deq_conversion import _LibraryBuilder

    codec = unencoded_codec()
    gadget = codec.layers[0].gadgets["step"]
    gadget.implements.parameters = [Parameter("enabled", "bit")]
    gadget.implements.action = [
        actions.Pauli("X_0", condition=actions.Condition(["enabled"], invert=invert))
    ]
    gadget.circuit.source = "X 0"
    builder = _LibraryBuilder(codec.layers[0], None)
    types = [
        builder.add_gadget("step", {"enabled": enabled}).gtype
        for enabled in (False, True, False)
    ]
    assert types == [1, 2, 1]
    for definition in builder.gadgets:
        row = next(
            item
            for item in definition.body
            if isinstance(item, PropagateStatement) and item.target.pauli == "X"
        )
        assert row.terms == [LogicalPauliTarget("X", 0, "IN", 0)]
        assert not row.flip


@pytest.mark.parametrize("invert", [False, True])
def test_deq_accepts_measurement_conditioned_pauli_actions(simulate, invert):
    from qodec.gadgets import Circuit
    from qodec.instructions import BlockOperand

    codec = unencoded_codec()
    physical = codec.layers[-1].instruction_set
    operand = BlockOperand("qubit")
    physical.instructions["measure_stay"] = qodec.Instruction(
        "measure_stay",
        inputs=[operand],
        outputs=[operand],
        action=[actions.Observe(["Z_0"])],
    )
    gadgets = codec.layers[0].gadgets
    gadgets["prepare"].circuit.source = "R 0\nH 0"
    step = gadgets["step"]
    step.implements.action = [
        actions.Observe(["Z_0"]),
        actions.Pauli(
            "X_0", condition=actions.Condition(["outcomes[0]"], invert=invert)
        ),
    ]
    step.circuit = Circuit(physical, "- measure_stay: [0]", format="yaml")
    step.readouts = [["circuit.readouts[0]", "in[0].z[0]"]]
    step.checks = [
        ["out[0].x[0]", "in[0].x[0]"],
        ["out[0].z[0]", "in[0].z[0]"],
    ]
    step.frames = {"out[0].z[0]": ["circuit.readouts[0]", int(invert)]}
    program = qir_program(
        [("prepare", [0], []), ("step", [0], [0]), ("measure", [0], [1])], [0, 1]
    )
    results = simulate(program, codec=codec, shots=32, seed=13)
    assert {row[0] for row in results} == {Result.Zero, Result.One}
    assert [row[1] for row in results] == [Result.One if invert else Result.Zero] * 32


@pytest.mark.parametrize("operator", ["X_0", "Y_0", "-Z_0"])
def test_deq_channel_action_binds_pauli_parameters(circuit_decoder, operator):
    from deq.circuit.model import LogicalPauliTarget, PropagateStatement
    from qodec.instructions import Parameter
    from qdk.simulation._qodec.deq_conversion import _LibraryBuilder

    codec = unencoded_codec()
    step = codec.layers[0].gadgets["step"]
    step.implements.parameters = [Parameter("operator", "pauli")]
    step.implements.action = [actions.Pauli("operator")]
    builder = _LibraryBuilder(codec.layers[0], None)
    builder.add_gadget("step", {"operator": operator})
    statements = [
        item for item in builder.gadgets[0].body if isinstance(item, PropagateStatement)
    ]
    assert len(statements) == 2
    for statement in statements:
        assert statement.terms == [
            LogicalPauliTarget(statement.target.pauli, 0, "IN", 0)
        ]
        assert not statement.flip


def test_public_deq_binds_actions_with_fixed_checks_and_readouts(circuit_decoder):
    from qodec.instructions import Parameter

    codec = unencoded_codec()
    codec.layers[0].gadgets["prepare"].frames = {"out[0].x[0]": [1]}
    step = codec.layers[0].gadgets["step"]
    step.implements.parameters = [Parameter("enabled", "bit")]
    step.implements.action = [
        actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"}),
        actions.Pauli("X_0", condition=actions.Condition(["enabled"])),
    ]
    step.circuit.source = "R 1\nM 1"
    step.implements.flags = ["reject"]
    step.checks = [["circuit.readouts[0]"]]
    step.readouts = [{"reject": ["circuit.readouts[0]"]}]

    module = pyqir.SimpleModule("bound_actions", 3, 3)
    qubit_type = module.qubits[0].type
    prepare = module.add_external_function(
        "prepare", pyqir.FunctionType(pyqir.Type.void(module.context), [qubit_type])
    )
    apply_step = module.add_external_function(
        "step",
        pyqir.FunctionType(
            pyqir.Type.void(module.context),
            [qubit_type, pyqir.IntType(module.context, 1)],
        ),
    )
    measure = module.add_external_function(
        "measure",
        pyqir.FunctionType(
            pyqir.Type.void(module.context), [qubit_type, module.results[0].type]
        ),
    )
    for index, enabled in enumerate((False, True, False)):
        qubit = module.qubits[index]
        module.builder.call(prepare, [qubit])
        module.builder.call(
            apply_step, [qubit, pyqir.const(pyqir.IntType(module.context, 1), enabled)]
        )
        module.builder.call(measure, [qubit, module.results[index]])
    label = pyqir.Constant.null(pyqir.PointerType(pyqir.IntType(module.context, 8)))
    pyqir.rt.array_record_output(
        module.builder, pyqir.const(pyqir.IntType(module.context, 64), 3), label
    )
    for result in module.results:
        pyqir.rt.result_record_output(module.builder, result, label)

    # The declared action, not the fixed physical body, transports the incoming frame.
    assert (
        run_qir(
            module.ir(),
            qodec=codec,
            decoder=circuit_decoder,
            shots=3,
            seed=42,
            on_shot_failure="raise",
        )
        == [[Result.One, Result.One, Result.One]] * 3
    )


@pytest.mark.parametrize(
    "arguments, error, message",
    [
        ({}, ValueError, "Missing parameter 'enabled'"),
        ({"enabled": True, "extra": 0}, ValueError, "Unknown parameter 'extra'"),
        ({"enabled": "yes"}, TypeError, "Parameter 'enabled' expects bit"),
    ],
)
def test_gadget_conversion_rejects_invalid_action_arguments(
    circuit_decoder, arguments, error, message
):
    from qodec.instructions import Parameter
    from qdk.simulation._qodec.deq_conversion import _LibraryBuilder

    codec = unencoded_codec()
    codec.layers[0].gadgets["step"].implements.parameters = [
        Parameter("enabled", "bit")
    ]
    with pytest.raises(error, match=message):
        _LibraryBuilder(codec.layers[0], None).add_gadget("step", arguments)


def test_gadget_conversion_rejects_unbound_circuit_parameters(circuit_decoder):
    from qdk.simulation._qodec.deq_conversion import _LibraryBuilder

    codec = unencoded_codec()
    codec.layers[0].gadgets["step"].parameter_bindings = {
        "missing": "circuit.source.enabled"
    }
    with pytest.raises(ValueError, match="parameter binding 'missing' has no argument"):
        _LibraryBuilder(codec.layers[0], None).add_gadget("step", {})


def test_circuit_deq_prepares_propagation_without_deq_inference(
    circuit_decoder, monkeypatch
):
    from deq.circuit.model import PropagateStatement
    from deq.transpiler import check_plugins, jit_noise_builder
    from qdk.simulation._qodec import deq_conversion

    codec = unencoded_codec()
    step = codec.layers[0].gadgets["step"]
    step.implements.action = [actions.Clifford({"X_0": "Z_0", "Z_0": "X_0"})]
    step.circuit.source = "H 0"
    compiler = deq_conversion._LibraryBuilder(codec.layers[0], None)

    def forbidden(*args, **kwargs):
        pytest.fail("QDK must supply propagation without asking deq to infer it")

    monkeypatch.setattr(check_plugins, "resolve_gadget_checks", forbidden)
    monkeypatch.setattr(jit_noise_builder, "compute_correction_propagation", forbidden)
    compiler.add_gadget("step", {})
    statements = [
        item
        for item in compiler.gadgets[0].body
        if isinstance(item, PropagateStatement)
    ]
    assert {
        (str(statement.target), tuple(map(str, statement.terms)))
        for statement in statements
    } == {
        ("OUT0.LX0", ("IN0.LZ0",)),
        ("OUT0.LZ0", ("IN0.LX0",)),
    }


@pytest.mark.parametrize("axis", ["X", "Z"])
@pytest.mark.parametrize("incoming", ["x", "z"])
def test_declared_reset_channel_discards_incoming_frames(simulate, axis, incoming):
    codec = unencoded_codec()
    gadgets = codec.layers[0].gadgets
    gadgets["prepare"].frames = {f"out[0].{incoming}[0]": [1]}
    gadgets["step"].implements.action = [actions.Stabilize([f"{axis}_0"])]
    # Deliberately different physical body: transport must use the declared reset.
    gadgets["step"].circuit.source = ""
    program = qir_program(
        [("prepare", [0], []), ("step", [0], []), ("measure", [0], [0])], [0]
    )
    assert simulate(program, codec=codec) == [[Result.Zero]] * 3


@pytest.mark.parametrize("incoming", [False, True])
def test_declared_measurement_channel_preserves_surviving_logical_frames(
    simulate, incoming
):
    from qodec.gadgets import Encoding
    from qodec.instructions import BlockOperand

    codec = unencoded_codec()
    layer = codec.layers[0]
    step = layer.gadgets["step"]
    step.implements.inputs = [BlockOperand("data"), BlockOperand("data")]
    step.implements.action = [actions.Observe(["Z_1"])]
    step.inputs = [
        Encoding(layer.codes["data"], support=[str(index)]) for index in range(2)
    ]
    step.circuit.source = "M 1"
    step.readouts = [["circuit.readouts[0]", "in[1].z[0]"]]
    layer.gadgets["prepare"].frames = {"out[0].z[0]": [int(incoming)]}
    program = qir_program(
        [
            ("prepare", [0], []),
            ("prepare", [1], []),
            ("step", [0, 1], [0]),
            ("measure", [0], [1]),
        ],
        [0, 1],
    )
    assert (
        simulate(program, codec=codec)
        == [[Result.One if incoming else Result.Zero] * 2] * 3
    )


def test_declared_preparation_channel_adds_an_unframed_output(simulate):
    from qodec.gadgets import Encoding
    from qodec.instructions import BlockOperand

    codec = unencoded_codec()
    layer = codec.layers[0]
    layer.gadgets["prepare"].frames = {"out[0].z[0]": [1]}
    step = layer.gadgets["step"]
    step.implements.outputs = [BlockOperand("data"), BlockOperand("data")]
    step.implements.action = [actions.Stabilize(["Z_1"])]
    step.outputs = [
        Encoding(layer.codes["data"], support=[str(index)]) for index in range(2)
    ]
    step.circuit.source = "R 1"
    program = qir_program(
        [
            ("prepare", [0], []),
            ("step", [0, 1], []),
            ("measure", [0], [0]),
            ("measure", [1], [1]),
        ],
        [0, 1],
    )
    assert simulate(program, codec=codec) == [[Result.One, Result.Zero]] * 3


def test_composition_preserves_measurement_and_result_order(simulate):
    program = compile_qasm(
        'include "stdgates.inc"; qubit[3] data; bit[3] result; '
        "x data[0]; x data[2]; "
        "result[2] = measure data[0]; "
        "result[0] = measure data[1]; "
        "result[1] = measure data[2];"
    )
    assert simulate(program, seed=9) == [[Result.Zero, Result.One, Result.One]] * 3


def test_circuit_deq_preserves_repeated_and_reordered_output_records(simulate):
    qir = qir_program(
        [
            ("prepare_z", [0], []),
            ("prepare_z", [1], []),
            ("__quantum__qis__x__body", [1], []),
            ("__quantum__qis__m__body", [0], [0]),
            ("__quantum__qis__m__body", [1], [1]),
        ],
        [1, 0, 1],
    )
    assert simulate(qir, seed=9) == [[Result.One, Result.Zero, Result.One]] * 3


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
    simulate, monkeypatch, field, message
):
    from qdk.simulation._qodec import deq_decoding

    compose = deq_decoding.transpile_compose_jit_gadget_type

    def wrong_count(*args, **kwargs):
        artifacts = compose(*args, **kwargs)
        getattr(artifacts.jit_type.base, field).add()
        return artifacts

    monkeypatch.setattr(deq_decoding, "transpile_compose_jit_gadget_type", wrong_count)
    with pytest.raises(ValueError, match=message):
        simulate()


def memory_program():
    return qir_program(
        [
            ("prepare_z", [0], []),
            ("prepare_z", [1], []),
            *(("idle", [block], []) for _ in range(8) for block in range(2)),
            ("__quantum__qis__m__body", [0], [0]),
            ("__quantum__qis__m__body", [1], [1]),
        ],
        [0, 1],
    )


def test_composition_reuses_open_port_types_and_resets_completed_shots(
    simulate, monkeypatch, prepared_batches, compilations
):
    from contextlib import asynccontextmanager
    from types import SimpleNamespace
    from qdk.simulation._qodec import deq_decoding

    monkeypatch.setattr(deq_decoding, "_COMPOSITE_SIZE", 32)
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
    assert (
        simulate(memory_program(), shots=257, seed=9)
        == [[Result.Zero, Result.Zero]] * 257
    )
    (batch,) = prepared_batches
    ((_, _, gadgets),) = compilations
    assert 1 < len(batch.composites) < len(gadgets)
    assert len(batch.library.gadget_types) < len(batch.composites)
    assert any(chunk.connectors for chunk in batch.composites)
    assert all(kind.base.is_free_hop is False for kind in batch.library.gadget_types)
    assert sorted(
        index for chunk in batch.composites for index in chunk.measurements
    ) == list(range(batch.trace.num_measurements))
    assert sum(chunk.readout_count for chunk in batch.composites) == sum(
        len(gadget.destinations) for gadget in gadgets
    )
    assert resets == [{"reset_library": False, "reset_decoder_service": False}]


def test_composition_corrects_single_faults_across_chunks(
    simulate, monkeypatch, prepared_batches, sample_records
):
    from qdk.simulation._qodec import deq_decoding

    monkeypatch.setattr(deq_decoding, "_COMPOSITE_SIZE", 32)
    noise = NoiseConfig()
    noise.cx.xi = 0.01
    simulate(memory_program(), noise=noise, shots=1)
    (batch,) = prepared_batches
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
    sample_records(rows)
    results = simulate(memory_program(), noise=noise, shots=len(rows), seed=9)
    assert results == [[Result.Zero, Result.Zero]] * len(rows)


def test_composition_keeps_individually_oversized_gadgets_intact(
    simulate, monkeypatch, prepared_batches, compilations
):
    from qdk.simulation._qodec import deq_decoding

    monkeypatch.setattr(deq_decoding, "_COMPOSITE_SIZE", 1)
    assert simulate(memory_program(), seed=9) == [[Result.Zero, Result.Zero]] * 3
    (batch,) = prepared_batches
    ((_, _, gadgets),) = compilations
    assert [
        (chunk.measurements, chunk.readout_count) for chunk in batch.composites
    ] == [
        (
            tuple(gadget.measurements),
            len(gadget.destinations),
        )
        for gadget in gadgets
    ]


@pytest.mark.parametrize("reprepare", [False, True])
def test_circuit_deq_closes_discarded_and_live_outputs(
    simulate, prepared_batches, reprepare
):
    calls = [("prepare_z", [0], []), ("__quantum__qis__x__body", [0], [])]
    if reprepare:
        calls *= 2
    assert simulate(qir_program(calls, []), seed=9) == [[], [], []]
    (batch,) = prepared_batches
    types = {kind.base.gtype: kind.base for kind in batch.library.gadget_types}
    assert {
        connector for chunk in batch.composites for connector in chunk.connectors
    } == {
        (index, port)
        for index, chunk in enumerate(batch.composites, 1)
        for port in range(len(types[chunk.gtype].outputs))
    }


def test_circuit_deq_does_not_add_undeclared_detection_checks(
    simulate, prepared_batches
):
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["__quantum__qis__m__body"].checks = []
    assert simulate(codec=codec) == [[Result.Zero]] * 3
    (batch,) = prepared_batches
    assert all(not gadget.finished_checks for gadget in batch.library.gadget_types)


def test_deq_keeps_detection_checks_without_exporting_check_readouts(
    simulate, compilations
):
    assert simulate(codec=repetition_with_parity_flag()) == [[Result.Zero]] * 3
    ((_, artifacts, _),) = compilations
    types = artifacts.jit_library.gadget_types
    assert sum(len(kind.finished_checks) for kind in types) == 2
    assert sum(len(kind.base.readouts) for kind in types) == 1


@pytest.mark.parametrize("policy", ["raise", "discard"])
def test_circuit_deq_does_not_reject_readouts_for_an_unsatisfied_authored_check(
    simulate, policy
):
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    codec.layers[0].gadgets["__quantum__qis__m__body"].checks = [
        ["circuit.readouts[0]", "circuit.readouts[1]", 1]
    ]
    assert simulate(codec=codec, shots=1, seed=9, on_shot_failure=policy) == [
        [Result.Zero]
    ]


def test_circuit_deq_preserves_a_pauli_before_a_logical_readout(simulate):
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.circuit = Circuit(
        codec.layers[-1].instruction_set, "X 0 1 2\nM 0 1 2", format="stim"
    )
    assert simulate(codec=codec, seed=9) == [[Result.One]] * 3


def test_circuit_deq_corrects_all_single_bit_errors(simulate, sample_records):
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    rows = [
        [Result.One if index == fault else Result.Zero for index in range(3)]
        for fault in range(3)
    ]
    sample_records(rows)
    results = simulate(noise=noise, shots=len(rows), seed=9)
    assert results == [[Result.Zero]] * 3


def test_circuit_deq_preserves_raw_rejection_flags(simulate, sample_records):
    codec = repetition_with_parity_flag()
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    rows = [[Result.One, Result.Zero, Result.Zero], [Result.Zero] * 3]
    sample_records(rows)
    assert simulate(
        codec=codec, noise=noise, shots=2, seed=9, on_shot_failure="discard"
    ) == [[Result.Zero]]
    with pytest.raises(ExecutionRejected):
        simulate(codec=codec, noise=noise, shots=2, seed=9)


def test_circuit_deq_skips_selected_raw_flags_only_when_discarding(
    simulate, monkeypatch, sample_records
):
    from qdk.simulation._qodec import deq_decoding

    codec = repetition_with_parity_flag()
    rows = [[Result.One, Result.Zero, Result.Zero]]
    sample_records(rows)
    with pytest.raises(ExecutionRejected):
        simulate(codec=codec, shots=1, seed=9)

    def forbidden(**kwargs):
        pytest.fail("Selected-out raw flags must not reach deq under discard")

    monkeypatch.setattr(deq_decoding, "Runtime", forbidden)
    assert simulate(codec=codec, shots=1, seed=9, on_shot_failure="discard") == []


def test_circuit_deq_keeps_filtered_shots_aligned_across_batches(
    simulate, sample_records
):
    codec = repetition_with_parity_flag()
    rows = [
        [Result.Zero] * 3,
        [Result.One, Result.Zero, Result.Zero],
        [Result.Zero] * 3,
    ] * 257
    sample_records(rows)
    assert (
        simulate(codec=codec, shots=len(rows), seed=9, on_shot_failure="discard")
        == [[Result.Zero]] * 514
    )


@pytest.mark.parametrize("probability", [0, 0.01, 0.75, 1])
@pytest.mark.parametrize("shots", [20, 257])
def test_circuit_deq_seeded_runs_are_reproducible(simulate, probability, shots):
    noise = NoiseConfig()
    noise.mresetz.x = probability
    first = simulate(noise=noise, shots=shots)
    assert first == simulate(noise=noise, shots=shots)
    assert len(first) == shots
    if probability == 0:
        assert first == [[Result.Zero]] * shots


def test_circuit_deq_requires_explicit_supported_execution(circuit_decoder, simulate):
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
        simulate(program)


def test_circuit_deq_rejects_general_pauli_channels(simulate):
    noise = NoiseConfig()
    noise.mresetz.x = 0.02
    noise.mresetz.z = 0.01
    with pytest.raises(NotImplementedError, match="general Pauli channels"):
        simulate(noise=noise)


@pytest.mark.parametrize("width", [1, 2])
def test_circuit_deq_depolarizing_mechanisms_reproduce_the_channel(
    circuit_decoder, width
):
    from qdk.simulation._qodec.deq_conversion import _channel

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


def test_logical_readout_aliases_preserve_constants_and_reject_cycles(simulate):
    codec = unencoded_codec(2)
    measure = codec.layers[0].gadgets["measure"]
    measure.readouts = [["readouts[1]"], ["circuit.readouts[0]", 1]]
    program = qir_program([("prepare", [0], []), ("measure", [0], [0, 1])], [0, 1])
    assert simulate(program, codec=codec) == [[Result.One, Result.One]] * 3
    measure.readouts = [["readouts[1]"], ["readouts[0]"]]
    with pytest.raises(ExecutionUnresolved, match="underdetermined"):
        simulate(program, codec=codec)


@pytest.mark.parametrize("record_dependent", [False, True])
def test_circuit_deq_applies_declared_frames_once(simulate, record_dependent):
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
    assert simulate(codec=codec, shots=10) == [[Result.One]] * 10


def test_local_flags_reject_nonzero_incoming_syndromes_even_without_noise(
    simulate,
    physical_samples,
):
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
    results = simulate(codec=codec, shots=100, on_shot_failure="discard")
    parities = [row[1] != row[2] for row in physical_samples]
    assert set(parities) == {False, True}
    assert results == [[Result.Zero] for parity in parities if not parity]

    with pytest.raises(ExecutionRejected):
        simulate(codec=codec, shots=100)


def test_circuit_deq_logical_sign_equation_matches_an_authored_frame(simulate):
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
    expected = simulate(codec=codec, shots=30)
    assert {row[0] for row in expected} == {Result.Zero, Result.One}
    prepare.frames = {}
    prepare.checks = [["out[0].z[0]", "circuit.readouts[0]"]]
    assert simulate(codec=codec, shots=30) == expected


@pytest.mark.parametrize("aliased", [False, True])
def test_circuit_deq_resolves_coupled_and_aliased_logical_signs(
    circuit_decoder, aliased
):
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
    if aliased:
        with pytest.raises(
            ValueError, match="flag values cannot reference encoding signs"
        ):
            run_qir(qir, qodec=codec, decoder=circuit_decoder, shots=3, seed=42)
        return
    assert (
        run_qir(qir, qodec=codec, decoder=circuit_decoder, shots=3, seed=42)
        == [[Result.One, Result.One]] * 3
    )


def test_deq_rejects_undeclared_quantum_calls_during_compilation(circuit_decoder):
    program = qir_program(
        [
            ("prepare", [0], []),
            ("__quantum__qis__y__body", [0], []),
            ("measure", [0], [0]),
        ],
        [0],
    )
    codec = unencoded_codec()
    with pytest.raises(
        ValueError, match="__quantum__qis__y__body.*top instruction set"
    ):
        run_qir(program, qodec=codec, decoder=circuit_decoder, shots=3, seed=42)


def test_deq_never_propagates_frames_in_python(simulate, monkeypatch):
    from qdk.simulation._qodec import native_batch

    def forbidden(*args, **kwargs):
        pytest.fail("deq must own cross-gadget frame propagation")

    monkeypatch.setattr(native_batch, "_FrameMasks", forbidden)
    monkeypatch.setattr(native_batch, "_static_flips", forbidden)
    codec = repetition_with_parity_flag()
    codec.layers[0].gadgets["prepare_z"].frames = {"out[0].z[0]": [1]}
    assert simulate(codec=codec) == [[Result.One]] * 3


def test_gadget_conversion_does_not_need_a_program_trace(circuit_decoder, monkeypatch):
    from qdk.simulation._qodec import native_batch
    from qdk.simulation._qodec.deq_conversion import _LibraryBuilder

    def forbidden(*args, **kwargs):
        pytest.fail("Gadget conversion must not trace a program")

    monkeypatch.setattr(native_batch, "_trace", forbidden)
    monkeypatch.setattr(native_batch, "CircuitTrace", forbidden)
    compiler = _LibraryBuilder(repetition_with_parity_flag().layers[0], None)
    first = compiler.add_gadget("__quantum__qis__m__body", {})
    assert compiler.add_gadget("__quantum__qis__m__body", {}) is first
    assert first.measurement_count == 3
    assert first.readout_count == 1
    assert [(flag.indices, flag.constant) for flag in first.flags] == [([0, 1], False)]
    _, artifacts = compiler.build()
    assert len(artifacts.jit_library.gadget_types) == 1


@pytest.mark.parametrize(
    "equations",
    [
        [["in[0].z[0]"]],
        [["in[0].z[0]", "in[0].z[0]"]],
        [["out[0].z[0]"]],
        [["readouts[0]"]],
        [["readouts[1]"]],
        [["readouts[2]"], ["readouts[1]"]],
        [["circuit.readouts[3]"]],
    ],
)
def test_deq_rejects_nonlocal_or_invalid_flag_dependencies(simulate, equations):
    codec = repetition_with_parity_flag()
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.readouts = [["in[0].z[0]"], *equations]
    measure.implements.flags = [f"reject_{index}" for index in range(len(equations))]
    with pytest.raises(
        (ValueError, ExecutionUnresolved), match="encoding signs|aliases|unavailable"
    ):
        simulate(codec=codec)


def test_deq_local_flags_resolve_forward_aliases_and_constants(
    simulate, sample_records
):
    codec = repetition_with_parity_flag()
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.implements.flags = ["first", "second", "constant"]
    measure.readouts = [
        ["circuit.readouts[0]"],
        ["readouts[2]", "circuit.readouts[2]", 1],
        ["readouts[0]", "circuit.readouts[1]"],
        [1],
    ]
    program = qir_program(
        [("prepare_z", [0], []), ("__quantum__qis__m__body", [0], [0, 1, 2, 3])],
        [0, 1, 2, 3],
    )
    rows = list(product((Result.Zero, Result.One), repeat=3))
    sample_records(rows)
    assert simulate(program, codec=codec, shots=len(rows)) == [
        [
            row[0],
            (
                Result.One
                if sum(bit == Result.One for bit in row) % 2 == 0
                else Result.Zero
            ),
            Result.One if row[0] != row[1] else Result.Zero,
            Result.One,
        ]
        for row in rows
    ]


def test_deq_does_not_use_checks_to_erase_raw_flag_evidence(simulate, sample_records):
    codec = repetition_with_parity_flag()
    measure = codec.layers[0].gadgets["__quantum__qis__m__body"]
    measure.checks.append(["readouts[1]"])
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    rows = [[Result.One, Result.Zero, Result.Zero], [Result.Zero] * 3]
    sample_records(rows)
    program = qir_program(
        [("prepare_z", [0], []), ("__quantum__qis__m__body", [0], [0, 1])], [0, 1]
    )
    results = simulate(program, codec=codec, noise=noise, shots=2)
    assert [row[1] for row in results] == [Result.One, Result.Zero]
    assert simulate(codec=codec, noise=noise, shots=2, on_shot_failure="discard") == [
        [Result.Zero]
    ]


def test_circuit_faults_follow_measurement_and_reset_timing(circuit_decoder):
    from qdk._native import QirInstructionId as Id
    from qdk.simulation._qodec.native_batch import CircuitTrace

    trace = CircuitTrace(
        instructions=(
            (Id.RESET, 0),
            (Id.MZ, 0, 0),
            (Id.MZ, 0, 1),
            (Id.RESET, 0),
            (Id.MZ, 0, 2),
        ),
        num_qubits=1,
        num_measurements=3,
        events=(),
        sources=(),
        outputs=(),
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
    from qdk.simulation._qodec.deq_conversion import _channel

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
def test_circuit_deq_rejects_changed_noise_before_sampling(
    simulate, prepared_batches, field
):
    noise = NoiseConfig()
    noise.mresetz.x = 0.01
    simulate(noise=noise)
    (batch,) = prepared_batches
    setattr(noise.mresetz, field, 0.02)
    with pytest.raises(ValueError, match="prepare a new"):
        batch.run(1, noise, seed=42)


@pytest.mark.parametrize(
    "check",
    [
        [1],
        ["circuit.readouts[0]", "circuit.readouts[0]", 1],
        ["readouts[1]", "circuit.readouts[0]", "circuit.readouts[1]", 1],
    ],
)
def test_deq_preserves_constant_detection_constraints(simulate, compilations, check):
    from deq.circuit.model import CheckStatement, GadgetDefinition, ReadoutStatement

    codec = repetition_with_parity_flag()
    codec.layers[0].gadgets["__quantum__qis__m__body"].checks = [check]
    program = qir_program(
        [("prepare_z", [0], []), ("__quantum__qis__m__body", [0], [0])], [0]
    )
    assert simulate(program, codec=codec) == [[Result.Zero]] * 3
    ((source, artifacts, _),) = compilations
    checks = [
        statement
        for definition in source.definitions
        if isinstance(definition, GadgetDefinition)
        for statement in definition.body
        if isinstance(statement, CheckStatement)
    ]
    assert sum(statement.flip and not statement.targets for statement in checks) == 1
    assert (
        sum(len(kind.finished_checks) for kind in artifacts.jit_library.gadget_types)
        == 1
    )
    assert (
        sum(
            isinstance(statement, ReadoutStatement)
            for definition in source.definitions
            if isinstance(definition, GadgetDefinition)
            for statement in definition.body
        )
        == 1
    )


@pytest.mark.parametrize("policy", ["raise", "discard"])
def test_circuit_deq_accepts_readouts_without_a_matching_fault_model(
    simulate, sample_records, policy
):
    rows = [[Result.One, Result.Zero, Result.Zero], [Result.Zero] * 3]
    sample_records(rows)
    assert simulate(shots=2, seed=1, on_shot_failure=policy) == [
        [Result.One],
        [Result.Zero],
    ]


def test_circuit_deq_crosses_batch_boundaries_without_state_leaks(simulate):
    noise = NoiseConfig()
    noise.mresetz.x = 0.1
    results = simulate(noise=noise, shots=513)
    assert len(results) == 513
    assert results == simulate(noise=noise, shots=513)


def test_circuit_deq_works_in_a_notebook_event_loop(simulate):
    async def run():
        return simulate(shots=2)

    assert asyncio.run(run()) == [[Result.Zero]] * 2


def test_circuit_deq_preserves_bit_order_across_bytes(simulate):
    program = compile_qasm(
        'include "stdgates.inc"; qubit[9] data; '
        "x data[0]; x data[3]; x data[8]; bit[9] result = measure data;"
    )
    expected = [Result.One if index in (0, 3, 8) else Result.Zero for index in range(9)]
    assert simulate(program) == [expected] * 3


def test_circuit_deq_preserves_reused_blocks_and_discards(simulate):
    from .test_native_batch import _repetition_code_with_x_circuit

    codec = _repetition_code_with_x_circuit("X 0 1 2\nCX 3 0 3 1\nM 3")
    program = qir_program(
        [
            ("prepare_z", [0], []),
            ("__quantum__qis__x__body", [0], []),
            ("__quantum__qis__x__body", [0], []),
            ("__quantum__qis__m__body", [0], [0]),
            ("prepare_z", [0], []),
            ("__quantum__qis__x__body", [0], []),
            ("__quantum__qis__m__body", [0], [1]),
        ],
        [0, 1],
    )
    assert simulate(program, codec=codec, shots=4) == [[Result.Zero, Result.One]] * 4


@pytest.mark.parametrize("probability", [0.25, 0.5, 0.75, 1])
def test_circuit_deq_preserves_single_pauli_fault_probabilities(
    simulate, prepared_batches, probability
):
    noise = NoiseConfig()
    noise.x.x = probability
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit r = measure data;'
    )
    simulate(program, noise=noise)
    (batch,) = prepared_batches
    assert [
        error.base.probability
        for gadget in batch.library.gadget_types
        for error in gadget.errors
    ] == [probability] * 3


def test_circuit_deq_corrects_data_and_intermediate_syndrome_faults(
    simulate, prepared_batches, sample_records
):
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
    simulate(program, noise=noise, codec=codec, shots=1)
    (batch,) = prepared_batches
    clean = [False, False, True, True, True]
    rows = [
        [
            Result.One if value ^ bool(fault >> index & 1) else Result.Zero
            for index, value in enumerate(clean)
        ]
        for fault, _ in physical_faults(batch.trace, noise)
    ]
    assert len(rows) == 5
    sample_records(rows)
    assert simulate(program, codec=codec, noise=noise, shots=len(rows)) == [
        Result.One
    ] * len(rows)


@pytest.mark.parametrize("kind", ["reset_loss", "gate_loss", "rotation", "layers"])
def test_circuit_deq_rejects_unsupported_traces(simulate, kind):
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
        simulate(compile_qasm(source), noise=noise, codec=codec)


@pytest.mark.parametrize("basis", ["zz", "xx"])
@pytest.mark.parametrize("flagged", [False, True])
def test_circuit_deq_c4_transport_syndromes_and_returned_flags(
    circuit_decoder, basis, flagged
):
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
@pytest.mark.parametrize("policy", ["discard", "raise"])
def test_circuit_deq_propagates_runtime_protocol_errors_before_flag_selection(
    simulate, sample_records, monkeypatch, kind, policy
):
    from contextlib import asynccontextmanager
    from types import SimpleNamespace
    from deq.proto import coordinator_pb2 as coordinator, util_pb2 as util
    from qdk.simulation._qodec import deq_decoding

    closed = []
    widths = {}
    instances = []

    async def load_library(library):
        widths.update(
            (kind.base.gtype, len(kind.base.readouts)) for kind in library.gadget_types
        )

    async def batch_execute(instructions):
        instances[:] = [instruction.gadget for instruction in instructions]
        return [
            99 if kind == "execute" else instruction.gadget.gid
            for instruction in instructions
        ]

    async def batch_decode(outcomes):
        if kind == "failure":
            raise RuntimeError("native decoder failure")
        if kind == "count":
            return []
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
            for outcome, gadget in zip(outcomes, instances)
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
    first = Result.One if policy == "raise" else Result.Zero
    sample_records([[first, Result.Zero, Result.Zero]])
    with pytest.raises(RuntimeError):
        simulate(codec=repetition_with_parity_flag(), shots=1, on_shot_failure=policy)
    assert closed == [True]


@pytest.mark.parametrize("failure", ["start", "shutdown"])
def test_deq_releases_worker_after_runtime_failure(simulate, monkeypatch, failure):
    from contextlib import asynccontextmanager
    from threading import current_thread
    from qdk.simulation._qodec import deq_decoding

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
        simulate(shots=1, seed=7)
    assert raised.value is error
    assert workers and all(not worker.is_alive() for worker in workers)


def test_deq_factory_accepts_keyword_only_runtime_factory():
    from inspect import Parameter, signature

    parameters = signature(prepare_deq_decoder).parameters
    assert tuple(parameters) == ("layer", "runtime_factory")
    assert parameters["layer"].kind is Parameter.POSITIONAL_OR_KEYWORD
    assert parameters["layer"].default is Parameter.empty
    assert parameters["runtime_factory"].kind is Parameter.KEYWORD_ONLY
    assert parameters["runtime_factory"].default is None


def test_runtime_factory_is_lazy_seeded_and_owned_by_the_worker(simulate):
    from threading import current_thread, main_thread
    from deq.runtime import Runtime

    created, closed, workers, seeds = [], [], [], []

    class TrackedRuntime(Runtime):
        async def shutdown(self):
            closed.append(self)
            await super().shutdown()

    def make_runtime(seed):
        assert asyncio.get_running_loop().is_running()
        assert current_thread() is not main_thread()
        workers.append(current_thread())
        seeds.append(seed)
        runtime = TrackedRuntime(
            decoder="black-box-relay-bp",
            decoder_config={"parallel": 1, "seed": seed},
            coordinator="monolithic",
            controller="jit",
        )
        created.append(runtime)
        return runtime

    codec = qodec.Qodec.load(FIXTURES / "repetition3.qodec.yaml")
    prepare_deq_decoder(codec.layers[0], runtime_factory=make_runtime)
    assert not created
    for shots, seed in ((257, 7), (3, 8)):
        assert (
            simulate(codec=codec, shots=shots, seed=seed, runtime_factory=make_runtime)
            == [[Result.Zero]] * shots
        )
    assert seeds == [7, 8]
    assert len(created) == 2 and created[0] is not created[1]
    assert closed == created
    assert all(not worker.is_alive() for worker in workers)


@pytest.mark.parametrize("failure", ["factory", "decode", "shutdown"])
def test_runtime_factory_errors_propagate_and_release_resources(
    simulate, monkeypatch, failure
):
    from threading import current_thread
    from deq.runtime import Runtime

    error = RuntimeError(f"{failure} failed")
    closed, workers = [], []

    class TrackedRuntime(Runtime):
        async def shutdown(self):
            closed.append(self)
            await super().shutdown()
            if failure == "shutdown":
                raise error

    async def fail_decode(outcomes):
        raise error

    def make_runtime(seed):
        workers.append(current_thread())
        if failure == "factory":
            raise error
        runtime = TrackedRuntime(
            decoder="black-box-relay-bp",
            decoder_config={"parallel": 1, "seed": seed},
            coordinator="monolithic",
            controller="jit",
        )
        if failure == "decode":
            monkeypatch.setattr(runtime.jit_controller, "batch_decode", fail_decode)
        return runtime

    with pytest.raises(RuntimeError) as raised:
        simulate(shots=1, runtime_factory=make_runtime)
    assert raised.value is error
    assert len(closed) == (0 if failure == "factory" else 1)
    assert all(not worker.is_alive() for worker in workers)


def test_runtime_factory_without_jit_is_rejected_and_closed(simulate):
    from deq.runtime import Runtime

    closed = []

    class TrackedRuntime(Runtime):
        async def shutdown(self):
            closed.append(self)
            await super().shutdown()

    def make_runtime(seed):
        return TrackedRuntime(
            decoder="black-box-relay-bp",
            decoder_config={"parallel": 1, "seed": seed},
            coordinator="monolithic",
        )

    with pytest.raises(AttributeError, match='controller="jit"'):
        simulate(shots=1, runtime_factory=make_runtime)
    assert len(closed) == 1


def test_runtime_factory_must_be_callable(circuit_decoder):
    codec = qodec.Qodec.load(FIXTURES / "repetition3.qodec.yaml")
    with pytest.raises(TypeError, match="runtime_factory must be callable"):
        circuit_decoder(codec.layers[0], runtime_factory=object())


def test_runtime_factory_invalid_result_is_not_replaced_by_a_default(simulate):
    with pytest.raises(TypeError, match="asynchronous context manager"):
        simulate(shots=1, runtime_factory=lambda seed: None)


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


def test_circuit_deq_empty_program_and_zero_shots(simulate):
    program = compile_qasm('include "stdgates.inc";')
    assert simulate(program, shots=0) == []
    assert simulate(program) == [(), (), ()]
    assert simulate(shots=0) == []


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
