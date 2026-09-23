from . import FIXTURES
from contextlib import closing
from unittest.mock import Mock

import qdk
import qdk.openqasm
import qdk.simulation._simulation as simulation
import qodec
import pytest

from qdk.simulation._qodec.bytecode import compile
from qdk.simulation._qodec.execution_pipeline import ExecutionPipeline
from qdk.simulation._qodec.protocols import Closable, Startable
from qdk.simulation._qodec.protocols import Resources
from qdk.simulation._qodec._run import run_qir_with_qodec
from qdk.simulation._qodec.quantum_backend import (
    full_state_backend,
    stabilizer_backend,
    tableau_backend,
)


@pytest.fixture(params=["syndrome", "deq"])
def prepare_code_decoder(request):
    from qdk.simulation import decoders

    if request.param == "deq":
        pytest.importorskip("deq")
        pytest.importorskip("deq_runtime")
        return decoders.prepare_deq_decoder
    return decoders.prepare_syndrome_decoder


@pytest.mark.parametrize(
    "error_name", ["ExecutionRejected", "ExecutionUnresolved", "InconsistentParity"]
)
def test_raw_shot_failure_policy_raises_by_default(monkeypatch, error_name):
    from qdk.simulation._qodec import _run

    monkeypatch.setattr(_run, "compile", Mock())
    failure = getattr(_run, error_name)("failed shot")
    executor = Mock()
    executor.run.side_effect = failure
    with pytest.raises(type(failure)) as raised:
        _run.run_qir_raw_records(Mock(), executor, 2)
    assert raised.value is failure
    assert executor.run.call_count == 1
    executor.set_seed.assert_not_called()


@pytest.mark.parametrize("max_retries", [0, 2])
@pytest.mark.parametrize("python_version", [(3, 10), (3, 11)])
def test_raw_shot_failure_policy_limits_retries(
    monkeypatch, max_retries, python_version
):
    from types import SimpleNamespace
    from qdk.simulation._qodec import _run
    from qdk.simulation._qodec.readout_equations import InconsistentParity

    monkeypatch.setattr(_run, "compile", Mock())
    monkeypatch.setattr(_run, "sys", SimpleNamespace(version_info=python_version))
    failure = InconsistentParity("unrecoverable check")
    executor = Mock()
    executor.run.side_effect = failure
    with pytest.raises(InconsistentParity) as raised:
        _run.run_qir_raw_records(
            Mock(),
            executor,
            2,
            on_shot_failure="retry",
            max_retries=max_retries,
        )
    assert raised.value is failure
    assert executor.run.call_count == max_retries + 1
    assert any(
        f"Shot 1 failed after {max_retries + 1} attempts" in str(note)
        for note in getattr(raised.value, "__notes__", raised.value.args)
    )
    executor.set_seed.assert_not_called()


def test_raw_shot_retry_budget_resets_for_each_requested_shot(monkeypatch):
    from qdk.simulation._qodec import _run
    from qdk.simulation._qodec.protocols import ExecutionRejected

    program = object()
    compile_program = Mock(return_value=program)
    monkeypatch.setattr(_run, "compile", compile_program)
    executor = Mock()
    executor.run.side_effect = [
        ExecutionRejected(),
        "first",
        ExecutionRejected(),
        "second",
    ]
    module = Mock()
    assert _run.run_qir_raw_records(
        module,
        executor,
        2,
        on_shot_failure="retry",
        max_retries=1,
    ) == ["first", "second"]
    assert executor.run.call_count == 4
    assert all(call.args == (program,) for call in executor.run.call_args_list)
    compile_program.assert_called_once_with(module)
    executor.set_seed.assert_not_called()


@pytest.mark.parametrize(
    "options",
    [
        {"on_shot_failure": "ignore"},
        {"max_retries": -1},
        {"max_retries": 0.5},
        {"max_retries": True},
    ],
)
def test_raw_shot_policy_rejects_invalid_configuration_before_execution(
    monkeypatch, options
):
    from qdk.simulation._qodec import _run

    compile_program = Mock()
    monkeypatch.setattr(_run, "compile", compile_program)
    executor = Mock()
    with pytest.raises(ValueError):
        _run.run_qir_raw_records(Mock(), executor, 1, **options)
    compile_program.assert_not_called()
    executor.run.assert_not_called()
    executor.set_seed.assert_not_called()


@pytest.mark.parametrize(
    "policy, accepted, attempts",
    [
        ("raise", 0, 1),
        ("discard", 1, 2),
        ("retry", 2, 3),
    ],
)
def test_qir_shot_failure_policy_restarts_and_closes_lost_shots(
    policy, accepted, attempts
):
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    instances = []

    class Backend:
        def __init__(self, seed, lost):
            self.seed = seed
            self.lost = lost
            self.operations = []
            self.closed = False

        def execute(self, operation):
            self.operations.append(operation.name)
            return (None if self.lost else True,) if operation.name == "measure" else ()

        def close(self):
            self.closed = True

    def create_backend(noise, seed):
        assert all(instance.closed for instance in instances)
        backend = Backend(seed, not instances)
        instances.append(backend)
        return backend

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).slice(1, 2)
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; x target; bit result = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )

    def run():
        return run_qir_with_qodec(
            qir,
            codec,
            None,
            shots=2,
            seed=7,
            quantum_backend_factory=create_backend,
            on_shot_failure=policy,
            max_retries=1,
        )

    seeds = []
    for _ in range(2):
        if policy == "raise":
            with pytest.raises(ExecutionUnresolved, match="could not be decoded"):
                run()
        else:
            assert run() == [qdk.Result.One] * accepted
        assert len(instances) == attempts
        assert len({instance.seed for instance in instances}) == attempts
        assert all(instance.closed for instance in instances)
        assert all(
            instance.operations == ["prepare", "x", "measure"] for instance in instances
        )
        seeds.append([instance.seed for instance in instances])
        instances.clear()
    assert seeds[0] == seeds[1]


@pytest.mark.parametrize("policy", ["raise", "discard", "retry"])
@pytest.mark.parametrize("failure_kind", ["loss", "parity"])
@pytest.mark.parametrize("simulator_type", ["cpu", "clifford"])
def test_qir_shot_failure_policy_handles_encoded_decoding_failures(
    policy, failure_kind, simulator_type
):
    from qdk.simulation._qodec.protocols import ExecutionUnresolved
    from qdk.simulation._qodec.readout_equations import InconsistentParity

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    noise = simulation.NoiseConfig()
    if failure_kind == "loss":
        noise.x.loss = 1
        error_type = ExecutionUnresolved
    else:
        gadget = codec.layers[0].gadgets["measure_z"]
        gadget.checks = [*gadget.checks, ["circuit.readouts[0]"]]
        error_type = InconsistentParity
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; x target; bit result = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )

    def run():
        return simulation.run_qir(
            qir,
            qodec=codec,
            noise=noise,
            shots=2,
            seed=7,
            type=simulator_type,
            on_shot_failure=policy,
            max_retries=1,
        )

    if policy == "discard":
        assert run() == []
    else:
        with pytest.raises(error_type) as raised:
            run()
        if policy == "retry":
            assert any(
                "2 attempts" in str(note)
                for note in getattr(raised.value, "__notes__", raised.value.args)
            )


@pytest.mark.parametrize("policy", ["discard", "retry"])
def test_shot_policy_does_not_retry_preparation_failures(monkeypatch, policy):
    from qdk.simulation._qodec import _run
    from qdk.simulation._qodec.readout_equations import InconsistentParity

    failure = InconsistentParity("invalid prepared equations")
    compile_program = Mock(side_effect=failure)
    monkeypatch.setattr(_run, "compile", compile_program)
    executor = Mock()
    with pytest.raises(InconsistentParity) as raised:
        _run.run_qir_raw_records(Mock(), executor, 2, on_shot_failure=policy)
    assert raised.value is failure
    assert compile_program.call_count == 1
    executor.run.assert_not_called()


def test_resources_distinguish_bare_qubits_and_typed_blocks():
    from collections.abc import MutableMapping
    from typing import cast

    from qdk.simulation._qodec.protocols import Resources

    blocks = {"c4": 3, "c6": 1}
    resources = Resources(qubits=2, blocks=blocks)
    blocks["c4"] = 9

    assert resources.qubits == 2
    assert dict(resources.blocks) == {"c4": 3, "c6": 1}
    assert Resources() == Resources(qubits=0, blocks={})
    with pytest.raises(TypeError):
        cast(MutableMapping[str, int], resources.blocks)["c4"] = 4


@pytest.mark.parametrize("qubits, blocks", [(-1, {}), (0, {"c4": -1})])
def test_resources_reject_negative_capacities(qubits, blocks):
    from qdk.simulation._qodec.protocols import Resources

    with pytest.raises(ValueError, match="non-negative"):
        Resources(qubits=qubits, blocks=blocks)


def test_invocation_contract_preserves_regrouping_and_qodec_arguments():
    from qodec.gadgets import Circuit, Encoding
    from qodec.instructions import Block, BlockOperand, InstructionCall, Parameter

    from qdk.simulation._qodec.protocols import BlockReference, Invocation

    physical = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[-1]
        .instruction_set
    )
    pair = qodec.Code("pair", [], ["X_0", "X_1"], ["Z_0", "Z_1"])
    single = qodec.Code("single", [], ["X_0"], ["Z_0"])
    instruction = qodec.Instruction(
        "split",
        inputs=[BlockOperand("pair")],
        outputs=[BlockOperand("single"), BlockOperand("single")],
        flags=["reject"],
        parameters=[
            Parameter("theta", "number"),
            Parameter("correction", "bit"),
            Parameter("operator", "pauli"),
            Parameter("names", "string"),
        ],
    )
    gadget = qodec.Gadget(
        instruction,
        Circuit(physical, "[]", format="yaml"),
        inputs=[Encoding(pair, support=["0", "1"])],
        outputs=[Encoding(single, support=["0"]), Encoding(single, support=["1"])],
        readouts=[[]],
        frames={"out[0].z[0]": []},
    )
    source = qodec.InstructionSet(
        "regroup",
        blocks=[Block("pair", 2), Block("single", 1)],
        instructions=[instruction],
    )
    qodec.Qodec(
        [
            qodec.Layer(
                source, codes={"pair": pair, "single": single}, gadgets=[gadget]
            ),
            qodec.Layer(physical),
        ]
    ).validate()
    call = InstructionCall(
        "split",
        operands=["data", "other"],
        arguments={
            "theta": 0.25,
            "correction": True,
            "operator": "-X_0 Z_1",
            "names": ["a", "b"],
        },
        select=[{"reject": 0}],
    )
    incoming = BlockReference("data", 1, "pair")
    outgoing = (
        BlockReference("data", 2, "single"),
        BlockReference("other", 1, "single"),
    )
    invocation = Invocation(3, gadget, call, (incoming,), outgoing)

    assert invocation.inputs == (incoming,)
    assert invocation.outputs == outgoing
    assert invocation.call.arguments == call.arguments
    assert invocation.call.select == [{"reject": 0}]
    assert invocation.gadget.frames == gadget.frames
    assert outgoing[0] != incoming


def test_decoder_contract_supports_correction_replies_and_distinct_flags():
    from typing_extensions import assert_type

    from qdk.simulation._qodec.protocols import (
        BlockReference,
        Correction,
        Corrections,
        Decoded,
        DecoderSession,
        Invocation,
        Readouts,
    )
    from qdk.simulation._qodec.quantum_operations import Operation

    survivor = BlockReference("other", 2, "repetition3")
    observed = []

    class Decoder:
        def decode(
            self, invocation: Invocation, readouts: Readouts
        ) -> Corrections[Decoded]:
            reply = yield Correction((survivor,), Operation("measure", (1,)))
            observed.append(reply)
            yield Correction((survivor,), Operation("rz", (0,), 0.25))
            return Decoded(outcomes=(None,), flags=(True,))

        def close(self) -> None:
            pass

    decoder: DecoderSession = Decoder()
    gadget = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[0]
        .gadgets["measure_z"]
    )
    requests = decoder.decode(invocation_for(gadget), (None, False, False))
    with closing(requests):
        assert next(requests) == Correction((survivor,), Operation("measure", (1,)))
        assert observed == []
        assert requests.send((False,)) == Correction(
            (survivor,), Operation("rz", (0,), 0.25)
        )
        assert observed == [(False,)]
        with pytest.raises(StopIteration) as completed:
            requests.send(())
    decoded = completed.value.value
    assert decoded.outcomes == (None,)
    assert decoded.flags == (True,)
    assert decoded.readouts == (None, True)
    assert_type(requests, Corrections[Decoded])


def test_pipeline_transports_typed_resources_from_arbitrary_program_data():
    from qdk.simulation._qodec.protocols import (
        ClassicalRuntimeFactory,
        Readouts,
        Requests,
    )

    capacities = []

    class Runtime:
        def required_resources(self, program: str) -> Resources:
            return Resources(blocks={"c4": 2, "c6": 1})

        def run(self, program: str) -> Requests[str]:
            yield from ()
            return program

    class Stage:
        def required_resources(self, upper: Resources) -> Resources:
            return Resources(qubits=4 * upper.blocks["c4"] + 6 * upper.blocks["c6"])

        def handle(self, request) -> Requests[Readouts]:
            reply = yield request
            if reply is None:
                raise TypeError("Missing call reply")
            return reply

        def start(self, resources: Resources) -> None:
            capacities.append(resources)

    factory: ClassicalRuntimeFactory[str, str] = Runtime
    pipeline = ExecutionPipeline(factory(), [Stage()], full_state_backend(None, 7))
    assert pipeline.run("opaque program") == "opaque program"
    assert capacities == [Resources(qubits=14)]


def test_circuit_preparation_contract_is_independent_of_source_format():
    from qodec.gadgets import Circuit
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.protocols import (
        Invocation,
        PrepareCircuit,
        PreparedCircuit,
        Readouts,
        Requests,
    )

    compiled = []
    instances = []

    def prepare(circuit: Circuit) -> PreparedCircuit:
        compiled.append((circuit.format, circuit.source))

        class Runtime:
            def __init__(self) -> None:
                self.records: list[bool | None] = []
                instances.append(self)

            def required_resources(self, invocation: Invocation) -> Resources:
                return Resources(blocks={"pair": len(invocation.inputs)})

            def run(self, invocation: Invocation) -> Requests[Readouts]:
                reply = yield InstructionCall(
                    "mpp",
                    operands=["left", "middle", "right"],
                    arguments={"p": "-X_0 Y_1 Z_2"},
                    select=[{"reject": 0}],
                )
                if reply is None:
                    raise TypeError("Missing call reply")
                self.records.extend(reply)
                return tuple(self.records)

        return PreparedCircuit(("left", "middle", "right"), Runtime)

    gadget = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[0]
        .gadgets["measure_z"]
    )
    circuit = Circuit(gadget.circuit.instruction_set, "opaque source", format="custom")
    prepare_circuit: PrepareCircuit = prepare
    prepared = prepare_circuit(circuit)
    first, second = prepared.create_runtime(), prepared.create_runtime()
    calls = []

    def respond(call):
        calls.append(call)
        return (None, False)

    assert first.required_resources(invocation_for(gadget)) == Resources(
        blocks={"pair": 1}
    )
    assert drive_requests(first.run(invocation_for(gadget)), respond) == (None, False)
    assert drive_requests(second.run(invocation_for(gadget)), respond) == (None, False)
    assert compiled == [("custom", "opaque source")]
    assert prepared.labels == ("left", "middle", "right")
    assert len(instances) == 2 and instances[0] is not instances[1]
    assert calls[0].operands == ["left", "middle", "right"]
    assert calls[0].arguments == {"p": "-X_0 Y_1 Z_2"}
    assert calls[0].select == [{"reject": 0}]


@pytest.mark.parametrize("flag", [False, True])
@pytest.mark.parametrize("reject_flagged", [False, True])
def test_native_instruction_program_preserves_flags_and_rejection(flag, reject_flagged):
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import Executor
    from qdk.simulation._qodec.protocols import ExecutionRejected, Readouts, Requests

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    layer = codec.layers[0]
    original = layer.gadgets["measure_z"]
    instruction = qodec.Instruction(
        "measure_z",
        inputs=original.implements.inputs,
        action=original.implements.action,
        flags=["reject"],
    )
    declarations = layer.instruction_set.instructions
    declarations["measure_z"] = instruction
    layer.instruction_set.instructions = declarations
    gadgets = layer.gadgets
    gadgets["measure_z"] = qodec.Gadget(
        instruction,
        original.circuit,
        inputs=original.inputs,
        checks=original.checks,
        readouts=[
            original.readouts[0].equation,
            {"reject": ["circuit.readouts[0]", "circuit.readouts[1]"]},
        ],
    )
    layer.gadgets = gadgets
    codec.validate()
    closed = []

    class Runtime:
        def required_resources(self, program: bool) -> Resources:
            return Resources(blocks={layer.instruction_set.blocks[0].name: 1})

        def run(self, program: bool) -> Requests[Readouts]:
            yield InstructionCall("prepare_z", operands=[0])
            readouts = yield InstructionCall("measure_z", operands=[0])
            if readouts is None:
                raise TypeError("Missing call reply")
            if program and readouts[1]:
                raise ExecutionRejected("reject flag")
            return readouts

    class Backend:
        def execute(self, request):
            return (
                (flag and request.targets[0] == 0,) if request.name == "measure" else ()
            )

        def close(self):
            closed.append(True)

    executor = Executor(
        codec, prepare_syndrome_decoder, None, Runtime, lambda noise, seed: Backend()
    )
    if flag and reject_flagged:
        with pytest.raises(ExecutionRejected, match="reject flag"):
            executor.run(reject_flagged)
    else:
        assert executor.run(reject_flagged) == (False, flag)
    assert closed == [True]


def test_pipeline_exposes_the_components_it_executes(monkeypatch):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.logical_qubits import LogicalQubits

    runtime = AdaptiveRuntime()
    logical = LogicalQubits()
    backend = full_state_backend(None, 7)
    pipeline = ExecutionPipeline(runtime, [logical], backend)
    execute = Mock(wraps=backend.execute)
    monkeypatch.setattr(pipeline.quantum_backend, "execute", execute)
    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit[1] qs;
        x qs[0];
        bit[1] rs = measure qs;
    """)

    assert pipeline.classical_runtime is runtime
    assert pipeline.layers == (logical,)
    assert pipeline.quantum_backend is backend
    assert pipeline.run(bytecode) == [qdk.Result.One]
    assert [call.args[0].name for call in execute.call_args_list] == [
        "prepare",
        "x",
        "measure",
    ]
    with pytest.raises(RuntimeError, match="has not been started"):
        backend.measure(0)


def compile_qasm(source):
    qir = qdk.openqasm.compile(source, target_profile=qdk.TargetProfile.Adaptive)
    module, _, _, _ = simulation.preprocess_simulation_input(qir)
    return compile(module)


@pytest.mark.parametrize("fails", [False, True])
def test_factory_creates_and_closes_injected_quantum_backends(fails):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.protocols import Readouts
    from qdk.simulation._qodec.quantum_operations import Operation

    instances = []
    noise = simulation.NoiseConfig()

    class RecordingBackend:
        def __init__(self, received_noise, seed):
            self.noise = received_noise
            self.seed = seed
            self.capacity = None
            self.close_count = 0

        def start(self, resources: Resources) -> None:
            self.capacity = resources.qubits

        def execute(self, request: Operation) -> Readouts:
            if fails:
                raise RuntimeError("injected backend failure")
            return (False,) if request.name == "measure" else ()

        def close(self) -> None:
            self.close_count += 1

    def create_backend(received_noise, seed):
        backend = RecordingBackend(received_noise, seed)
        instances.append(backend)
        return backend

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).slice(1, 2)
    factory = ExecutionPipelineFactory(
        codec,
        prepare_syndrome_decoder,
        noise,
        AdaptiveRuntime,
        quantum_backend_factory=create_backend,
    )
    bytecode = compile_qasm(
        'include "stdgates.inc"; qubit target; bit readout = measure target;'
    )
    factory.set_seed(7)
    for _ in range(2):
        pipeline = factory.build_pipeline()
        assert pipeline.quantum_backend is instances[-1]
        if fails:
            with pytest.raises(RuntimeError, match="injected backend failure"):
                pipeline.run(bytecode)
        else:
            assert pipeline.run(bytecode) == [qdk.Result.Zero]

    assert instances[0] is not instances[1]
    assert instances[0].seed != instances[1].seed
    assert all(backend.noise is noise for backend in instances)
    assert all(
        backend.capacity == 1 and backend.close_count == 1 for backend in instances
    )
    factory.set_seed(7)
    replay = factory.build_pipeline()
    try:
        assert instances[-1].seed == instances[0].seed
    finally:
        replay._close()


def test_quantum_backend_uses_an_injected_engine():
    from qdk.simulation._qodec.quantum_backend import QuantumBackend

    events = []

    class RecordingEngine:
        def apply(self, operation, targets, *, angle=None):
            events.append((operation, tuple(targets), angle))

        def measure(self, target):
            events.append(("measure", target))
            return 1

        def reset(self, target):
            events.append(("reset", target))

        def close(self):
            events.append(("close",))

    def create_engine(num_qubits, seed):
        events.append(("create", num_qubits, seed))
        return RecordingEngine()

    backend = QuantumBackend(None, seed=7, engine_factory=create_engine)
    backend.start(Resources(qubits=3))
    try:
        backend.prepare(0)
        backend.apply("rz", (0,), angle=0.25)
        assert backend.measure(0) is True
        backend.discard(0)
    finally:
        backend.close()

    assert events == [
        ("create", 3, 7),
        ("reset", 0),
        ("rz", (0,), 0.25),
        ("measure", 0),
        ("reset", 0),
        ("close",),
    ]


@pytest.mark.parametrize(
    "factory_name", ["full_state_backend", "stabilizer_backend", "tableau_backend"]
)
def test_engine_factories_preserve_noise_and_noiseless_discard(factory_name):
    from qdk.simulation._qodec import quantum_backend

    noise = simulation.NoiseConfig()
    noise.mresetz.x = 1
    backend = getattr(quantum_backend, factory_name)(noise, 7)
    backend.start(Resources(qubits=1))
    try:
        backend.prepare(0)
        assert backend.measure(0) is True
        backend.discard(0)
        assert backend.measure(0) is False
    finally:
        backend.close()


@pytest.mark.parametrize(
    "operation, targets",
    [
        ("x", (2,)),
        ("y", (2,)),
        ("z", (2,)),
        ("h", (2,)),
        ("s", (2,)),
        ("s_adj", (2,)),
        ("sx", (2,)),
        ("sx_adj", (2,)),
        ("cx", (2, 0)),
        ("cy", (2, 0)),
        ("cz", (2, 0)),
        ("swap", (2, 0)),
        ("mov", (2,)),
    ],
)
def test_tableau_clifford_gates_match_full_state(operation, targets):
    from qdk.simulation._qodec.full_state_engine import FullStateEngine
    from qdk.simulation._qodec.tableau_engine import TableauEngine
    from .test_quantum_instruments import assert_same_state

    tableau = TableauEngine(3, seed=7)
    full_state = FullStateEngine(3, seed=7)
    try:
        for engine in (tableau, full_state):
            engine.apply("h", (2,))
            engine.apply("s", (2,))
            engine.apply("cx", (2, 1))
            engine.apply(operation, targets)
        assert_same_state(
            tableau.simulator.state_vector(endian="little"), full_state.state()
        )
    finally:
        tableau.close()
        full_state.close()


@pytest.mark.parametrize(
    "operation, targets, angle, error",
    [
        ("t", (0,), None, NotImplementedError),
        ("measure", (0,), None, NotImplementedError),
        ("rz", (0,), 0.3, NotImplementedError),
        ("x", (0,), 0.3, ValueError),
        ("x", (0, 1), None, ValueError),
        ("cx", (0,), None, ValueError),
        ("cx", (0, 0), None, ValueError),
        ("x", (2,), None, ValueError),
        ("x", (-1,), None, ValueError),
    ],
)
def test_tableau_engine_rejects_invalid_operations(operation, targets, angle, error):
    from qdk.simulation._qodec.tableau_engine import TableauEngine

    engine = TableauEngine(2, seed=7)
    try:
        with pytest.raises(error):
            engine.apply(operation, targets, angle=angle)
        assert engine.simulator.num_qubits == 2
        assert engine.measure(0) == engine.measure(1) == 0
    finally:
        engine.close()


@pytest.mark.parametrize("operation", ["x", "measure", "reset"])
def test_tableau_engine_enforces_capacity_and_closed_state(operation):
    from qdk.simulation._qodec.tableau_engine import TableauEngine

    engine = TableauEngine(1, seed=7)

    def apply(target):
        if operation == "x":
            engine.apply("x", (target,))
        else:
            getattr(engine, operation)(target)

    try:
        with pytest.raises(ValueError):
            apply(1)
        assert engine.simulator.num_qubits == 1
    finally:
        engine.close()
    with pytest.raises(RuntimeError, match="closed"):
        apply(0)


def test_tableau_measurements_replay_seed_and_preserve_entanglement():
    def sample(seed):
        backend = tableau_backend(None, seed)
        backend.start(Resources(qubits=2))
        records = []
        try:
            for _ in range(32):
                backend.prepare(0)
                backend.prepare(1)
                backend.apply("h", (0,))
                backend.apply("cx", (0, 1))
                first = backend.measure(0)
                assert backend.measure(1) is first
                records.append(first)
        finally:
            backend.close()
        return records

    first = sample(7)
    assert first == sample(7)
    assert first != sample(8)
    assert set(first) == {False, True}


@pytest.mark.parametrize("backend_factory", [full_state_backend, stabilizer_backend])
def test_quantum_engines_preserve_coherent_rotations_and_entanglement(backend_factory):
    from math import pi

    backend = backend_factory(None, 7)
    backend.start(Resources(qubits=2))
    try:
        backend.prepare(0)
        backend.prepare(1)
        backend.apply("h", (0,))
        backend.apply("rz", (0,), angle=pi / 3)
        backend.apply("rz", (0,), angle=-pi / 3)
        backend.apply("cx", (0, 1))
        first = backend.measure(0)
        assert first is not None
        assert backend.measure(1) is first
    finally:
        backend.close()


def test_backend_closes_if_decoder_construction_fails():
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory

    backend = Mock(execute=Mock(return_value=()), close=Mock())

    def create_decoder(seed):
        raise RuntimeError("injected decoder construction failure")

    factory = ExecutionPipelineFactory(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        lambda layer: create_decoder,
        None,
        AdaptiveRuntime,
        quantum_backend_factory=lambda noise, seed: backend,
    )
    with pytest.raises(RuntimeError, match="injected decoder construction failure"):
        factory.build_pipeline()

    backend.close.assert_called_once()


def test_runner_defaults_to_stabilizer_backend():
    from inspect import signature

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; x target; bit readout = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))

    assert (
        run_qir_with_qodec(
            qir, codec, None, decoder=prepare_syndrome_decoder, shots=2, seed=7
        )
        == [qdk.Result.One] * 2
    )
    assert (
        signature(run_qir_with_qodec).parameters["quantum_backend_factory"].default
        is stabilizer_backend
    )


def test_runner_accepts_a_stateless_backend_factory():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.protocols import Readouts
    from qdk.simulation._qodec.quantum_operations import Operation

    class StatelessBackend:
        def execute(self, request: Operation) -> Readouts:
            return (True,) if request.name == "measure" else ()

    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; bit readout = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).slice(1, 2)

    assert (
        run_qir_with_qodec(
            qir,
            codec,
            None,
            decoder=prepare_syndrome_decoder,
            shots=2,
            seed=7,
            quantum_backend_factory=lambda noise, seed: StatelessBackend(),
        )
        == [qdk.Result.One] * 2
    )


@pytest.mark.parametrize("keyword_factories", [False, True])
def test_executor_preserves_custom_program_and_result_types(keyword_factories):
    from dataclasses import dataclass
    from typing_extensions import assert_type

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import (
        Executor,
        ExecutionPipelineFactory,
    )
    from qdk.simulation._qodec.protocols import ClassicalRuntimeFactory, Requests
    from qdk.simulation._qodec.quantum_operations import Operation

    @dataclass(frozen=True)
    class ClassicalProgram:
        answer: str

    class ClassicalRuntime:
        def required_resources(self, program: ClassicalProgram) -> Resources:
            return Resources(qubits=1)

        def run(self, program: ClassicalProgram) -> Requests[str]:
            yield Operation("prepare", (0,))
            return program.answer

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).slice(1, 2)
    runtime_factory: ClassicalRuntimeFactory[ClassicalProgram, str] = ClassicalRuntime
    if keyword_factories:
        executor = Executor(
            codec,
            prepare_syndrome_decoder,
            None,
            classical_runtime_factory=runtime_factory,
            quantum_backend_factory=full_state_backend,
        )
        factory = ExecutionPipelineFactory(
            codec,
            prepare_syndrome_decoder,
            None,
            classical_runtime_factory=runtime_factory,
            quantum_backend_factory=full_state_backend,
        )
    else:
        executor = Executor(
            codec, prepare_syndrome_decoder, None, runtime_factory, full_state_backend
        )
        factory = ExecutionPipelineFactory(
            codec, prepare_syndrome_decoder, None, runtime_factory, full_state_backend
        )

    result = assert_type(executor.run(ClassicalProgram("finished")), str)

    assert result == "finished"
    pipeline = factory.build_pipeline()
    assert assert_type(pipeline.run(ClassicalProgram("finished")), str) == "finished"


def test_backend_boundary_rejects_unlowered_instructions():
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime

    pipeline = ExecutionPipeline(AdaptiveRuntime(), [], full_state_backend(None, 7))

    def requests():
        yield InstructionCall("M", operands=[0])

    try:
        with pytest.raises(TypeError, match="requires a lowered Operation"):
            pipeline._drive(requests(), 0)
    finally:
        pipeline._close()


def test_syndrome_decoder_preparation_returns_a_session_factory(prepare_code_decoder):
    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    create_decoder = prepare_code_decoder(layer)
    first = create_decoder(7)
    second = create_decoder(8)
    try:
        assert first is not second
        gadget = layer.gadgets["measure_z"]
        assert decode_gadget(first, gadget, (True, False, False)).readouts == (False,)
        assert decode_gadget(second, gadget, (False, True, True)).readouts == (True,)
    finally:
        first.close()
        second.close()


@pytest.mark.parametrize("failure", ["gid", "size", "data", "syndrome"])
def test_deq_decoder_rejects_invalid_corrections(monkeypatch, failure):
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from deq.proto import coordinator_pb2, util_pb2
    from qdk.simulation.decoders import prepare_deq_decoder
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]

    async def decode(library, outcomes):
        return coordinator_pb2.Readouts(
            gid=2 if failure == "gid" else 1,
            readouts=util_pb2.BitVector(
                size=8 if failure == "size" else 9,
                data=b"" if failure == "data" else b"\x00\x00",
            ),
        )

    with closing(prepare_deq_decoder(layer)(7)) as session:
        monkeypatch.setattr(session, "_decode", decode)
        with pytest.raises(ExecutionUnresolved, match="deq"):
            decode_gadget(session, layer.gadgets["measure_z"], (True, False, False))


@pytest.mark.parametrize("failure", ["start", "shutdown"])
def test_deq_decoder_releases_worker_after_failure(monkeypatch, failure):
    import threading

    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec import deq_decoding
    from qdk.simulation.decoders import prepare_deq_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    factory = prepare_deq_decoder(layer)
    workers = []
    original = RuntimeError("injected deq failure")

    def failed_runtime(**options):
        workers.append(threading.current_thread())
        raise original

    async def failed_shutdown():
        workers.append(threading.current_thread())
        raise original

    if failure == "start":
        monkeypatch.setattr(deq_decoding, "Runtime", failed_runtime)
        with pytest.raises(RuntimeError) as raised:
            factory(7)
    else:
        session = factory(7)
        assert isinstance(session, deq_decoding.DeqSession)
        assert session._runtime is not None
        monkeypatch.setattr(session._runtime, "shutdown", failed_shutdown)
        with pytest.raises(RuntimeError) as raised:
            session.close()
        session.close()
        assert session._loop.is_closed()
    assert raised.value is original
    assert workers and all(not worker.is_alive() for worker in workers)


@pytest.mark.parametrize("probability", [-1, 0, 0.5, 1, float("nan"), float("inf")])
def test_deq_decoder_rejects_invalid_fault_probability(probability):
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation.decoders import prepare_deq_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    with pytest.raises(ValueError, match="error_probability"):
        prepare_deq_decoder(layer, error_probability=probability)


def test_deq_decoder_corrects_all_single_qubit_steane_paulis():
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec.decoding import CodeDecoder
    from qdk.simulation._qodec.deq_decoding import DeqSession
    from qdk.simulation.decoders import prepare_deq_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "steane/qodec.yaml")).layers[0]
    code = CodeDecoder(layer.codes["steane"])
    with closing(prepare_deq_decoder(layer)(7)) as session:
        assert isinstance(session, DeqSession)
        for fault in code.faults:
            syndrome = tuple(
                not fault.commutes_with(stabilizer) for stabilizer in code.stabilizers
            )
            residual = fault * session.correct(code, syndrome)
            assert all(
                residual.commutes_with(operator)
                for operators in code.operators.values()
                for operator in operators
            )


def test_decoder_preparation_is_only_required_for_encoded_layers():
    from typing import cast

    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.protocols import PrepareDecoder

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    with pytest.raises(TypeError, match="decoder must be a callable"):
        ExecutionPipelineFactory(
            codec,
            cast(PrepareDecoder, None),
            None,
            AdaptiveRuntime,
            quantum_backend_factory=full_state_backend,
        )

    prepare_decoder = Mock(
        side_effect=AssertionError("Physical-only execution must not prepare a decoder")
    )
    pipeline = ExecutionPipelineFactory(
        codec.slice(1, 2),
        prepare_decoder,
        None,
        AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    ).build_pipeline()
    try:
        assert (
            pipeline.resource_counts(Resources(qubits=2)) == (Resources(qubits=2),) * 3
        )
        prepare_decoder.assert_not_called()
    finally:
        pipeline._close()


@pytest.mark.parametrize("fail", [False, True])
def test_executor_creates_and_closes_non_adaptive_runtimes_per_shot(fail):
    from types import SimpleNamespace

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import Executor
    from qdk.simulation._qodec.quantum_operations import Operation

    class ClassicalRuntime:
        def __init__(self):
            self.readouts = []
            self.close_count = 0

        def required_resources(self, program):
            return Resources(qubits=program.num_qubits)

        def run(self, program):
            yield Operation("prepare", (0,))
            yield Operation("x", (0,))
            self.readouts.extend((yield Operation("measure", (0,))))
            if fail:
                raise RuntimeError("injected runtime failure")
            return self.readouts

        def close(self):
            self.close_count += 1

    instances = []

    def create_runtime():
        runtime = ClassicalRuntime()
        instances.append(runtime)
        return runtime

    executor = Executor(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        prepare_syndrome_decoder,
        None,
        create_runtime,
        quantum_backend_factory=full_state_backend,
    )
    program = SimpleNamespace(num_qubits=1)
    for _ in range(2):
        if fail:
            with pytest.raises(RuntimeError, match="injected runtime failure"):
                executor.run(program)
        else:
            assert executor.run(program) == [True]

    assert len(instances) == 2
    assert instances[0] is not instances[1]
    assert all(runtime.readouts == [True] for runtime in instances)
    assert all(runtime.close_count == 1 for runtime in instances)


@pytest.mark.parametrize("initializes", [False, True])
@pytest.mark.parametrize("cleans_up", [False, True])
def test_pipeline_honors_independent_lifecycle_contracts(
    monkeypatch, initializes, cleans_up
):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime

    events = []

    class ForwardingStage:
        def required_resources(self, upper):
            return upper

        def handle(self, request):
            events.append(("request", request.name))
            return (yield request)

    stage = ForwardingStage()
    if initializes:
        monkeypatch.setattr(
            stage, "start", lambda count: events.append(("start", count)), raising=False
        )
    if cleans_up:
        monkeypatch.setattr(
            stage, "close", lambda: events.append(("close", None)), raising=False
        )
    pipeline = ExecutionPipeline(
        AdaptiveRuntime(), [stage], full_state_backend(None, 7)
    )
    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit target;
        x target;
        bit readout = measure target;
    """)

    assert pipeline.run(bytecode) == [qdk.Result.One]
    expected: list[tuple[str, str | Resources | None]] = [
        ("request", "prepare"),
        ("request", "x"),
        ("request", "measure"),
    ]
    if initializes:
        expected.insert(0, ("start", Resources(qubits=1)))
    if cleans_up:
        expected.append(("close", None))
    assert events == expected


def test_pipeline_accepts_a_backend_without_lifecycle_methods():
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime

    requests = []

    class ScriptedBackend:
        def execute(self, request):
            requests.append(request.name)
            return (True,) if request.name == "measure" else ()

    pipeline = ExecutionPipeline(AdaptiveRuntime(), [], ScriptedBackend())
    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit target;
        bit readout = measure target;
    """)

    assert pipeline.run(bytecode) == [qdk.Result.One]
    assert requests == ["prepare", "measure"]
    assert pipeline.closed


@pytest.mark.parametrize(
    "num_qubits, expected", [(1, (1, 1, 5, 5)), (5, (5, 5, 17, 17))]
)
def test_resource_counts_are_inspectable_without_starting_stages(
    monkeypatch, num_qubits, expected
):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.layer_runtime import LayerRuntime
    from qdk.simulation._qodec.quantum_backend import QuantumBackend

    pipeline = ExecutionPipelineFactory(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        prepare_syndrome_decoder,
        None,
        classical_runtime_factory=AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    ).build_pipeline()
    for stage in (*pipeline.layers, pipeline.quantum_backend):
        if isinstance(stage, Startable):
            monkeypatch.setattr(
                stage,
                "start",
                Mock(side_effect=AssertionError("Sizing must not start a stage")),
            )
    try:
        assert pipeline.resource_counts(Resources(qubits=num_qubits)) == tuple(
            Resources(qubits=count) for count in expected
        )
        encoded = pipeline.layers[1]
        backend = pipeline.quantum_backend
        assert isinstance(encoded, LayerRuntime)
        assert isinstance(backend, QuantumBackend)
        assert encoded.layout.blocks == {}
        assert encoded.layout.free == []
        assert not pipeline.closed
        with pytest.raises(RuntimeError, match="has not been started"):
            _ = backend.engine
    finally:
        pipeline._close()


def test_sizing_failure_does_not_start_any_stage(monkeypatch):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.layer_runtime import LayerRuntime

    pipeline = ExecutionPipelineFactory(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        prepare_syndrome_decoder,
        None,
        classical_runtime_factory=AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    ).build_pipeline()
    starts = []
    for stage in (*pipeline.layers, pipeline.quantum_backend):
        if isinstance(stage, Startable):
            start = Mock(wraps=stage.start)
            monkeypatch.setattr(stage, "start", start)
            starts.append(start)
    monkeypatch.setattr(
        pipeline.layers[-1],
        "required_resources",
        Mock(side_effect=ValueError("Unsupported resource requirements")),
    )

    with pytest.raises(ValueError, match="Unsupported resource requirements"):
        pipeline.run(Mock(num_qubits=5))

    assert all(not start.called for start in starts)
    assert pipeline.closed
    encoded = pipeline.layers[1]
    assert isinstance(encoded, LayerRuntime)
    assert isinstance(encoded.decoder, SyndromeSession)
    assert encoded.decoder.closed


def invocation_for(gadget):
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.protocols import BlockReference, Invocation

    instruction = gadget.implements
    call = InstructionCall(
        instruction.mnemonic,
        operands=list(range(max(len(instruction.inputs), len(instruction.outputs)))),
    )
    return Invocation(
        0,
        gadget,
        call,
        tuple(
            BlockReference(index, 1, operand.block)
            for index, operand in enumerate(instruction.inputs)
        ),
        tuple(
            BlockReference(index, 1, operand.block)
            for index, operand in enumerate(instruction.outputs)
        ),
    )


def decode_gadget(decoder, gadget, readouts, corrections=None):
    captured = [] if corrections is None else corrections

    def complete(correction):
        captured.append(correction)
        return ()

    return drive_requests(decoder.decode(invocation_for(gadget), readouts), complete)


def drive_requests(requests, respond):
    with closing(requests):
        reply = None
        while True:
            try:
                request = requests.send(reply)
            except StopIteration as completed:
                return completed.value
            reply = respond(request)


@pytest.mark.parametrize("readout", [False, True])
def test_adaptive_runtime_accepts_scripted_readouts_without_a_backend(readout):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime

    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit[2] qs;
        bit[2] rs;
        rs[0] = measure qs[0];
        if (rs[0]) { x qs[1]; }
        rs[1] = measure qs[1];
    """)
    runtime = AdaptiveRuntime()
    requests = runtime.run(bytecode)
    emitted = []
    reply = None
    while True:
        try:
            request = requests.send(reply)
        except StopIteration as completed:
            records = completed.value
            break
        emitted.append((request.name, request.targets))
        reply = (readout,) if request.name == "measure" else ()

    expected = qdk.Result.One if readout else qdk.Result.Zero
    assert records == [expected, expected]
    assert (("x", (1,)) in emitted) == readout
    assert not hasattr(runtime, "quantum_runtime")


def test_adaptive_runtime_resumes_classical_loop_state():
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime

    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit target;
        int count = 0;
        while (count < 2) {
            bit readout = measure target;
            if (readout) { count += 1; }
        }
    """)
    runtime = AdaptiveRuntime()
    readouts = iter([False, True, False, True])
    observed = []

    def respond(request):
        if request.name != "measure":
            return ()
        value = next(readouts)
        observed.append(value)
        return (value,)

    records = drive_requests(runtime.run(bytecode), respond)

    assert records == [2]
    assert observed == [False, True, False, True]
    assert list(runtime.results.values()) == [qdk.Result.One]


def test_adaptive_runtime_branches_on_a_measurement():
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.logical_qubits import LogicalQubits

    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit[2] qs;
        bit[2] rs;
        x qs[0];
        rs[0] = measure qs[0];
        if (rs[0]) { x qs[1]; }
        rs[1] = measure qs[1];
    """)
    pipeline = ExecutionPipeline(
        AdaptiveRuntime(), [LogicalQubits()], full_state_backend(None, 7)
    )

    assert pipeline.run(bytecode) == [qdk.Result.One, qdk.Result.One]


def test_binding_does_not_decompose_operations():
    from qdk.simulation._qodec.instruction_set import InstructionSet

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    instructions = InstructionSet(codec.layers[0].instruction_set)

    with pytest.raises(NotImplementedError, match="does not implement 't'"):
        instructions.bind("t", 1)
    assert instructions.bind("rz", 1, 0.25) == ("rotate_z", {"theta": 0.25})


def rotation_instruction_set(*fixed_names):
    from math import pi

    from qdk.simulation._qodec.instruction_set import InstructionSet
    from qodec.actions import Rotate

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[0]
        .instruction_set
    )
    declarations = isa.instructions
    rotation = declarations["rotate_z"]
    for name in fixed_names:
        declarations[name] = qodec.Instruction(
            name,
            inputs=rotation.inputs,
            outputs=rotation.outputs,
            action=[Rotate("Z_0", pi / 4)],
        )
    isa.instructions = declarations
    return InstructionSet(isa)


def test_binding_prefers_a_fixed_angle_over_a_parameter():
    from math import pi

    instructions = rotation_instruction_set("quarter_phase")

    assert instructions.bind("rz", 1, pi / 4) == ("quarter_phase", {})
    assert instructions.bind("rz", 1, 0.25) == ("rotate_z", {"theta": 0.25})


def test_binding_rejects_equally_specific_matches():
    from math import pi

    instructions = rotation_instruction_set("quarter_phase", "another_quarter_phase")

    with pytest.raises(ValueError, match="Ambiguous"):
        instructions.bind("rz", 1, pi / 4)


def test_resolver_uses_a_direct_binding_before_decomposition():
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qodec.instructions import InstructionCall

    decompose = Mock(
        side_effect=AssertionError("Direct matches must not be decomposed")
    )
    resolve = prepare_resolver(rotation_instruction_set(), decompose)

    assert resolve("rz", ("data",), 0.25) == (
        InstructionCall("rotate_z", operands=["data"], arguments={"theta": 0.25}),
    )
    decompose.assert_not_called()


def test_resolver_keeps_a_direct_named_gate_implementation():
    from qdk.simulation._qodec.instruction_set import InstructionSet
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qodec.actions import Clifford
    from qodec.instructions import InstructionCall

    isa = (
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
        .layers[0]
        .instruction_set
    )
    declarations = isa.instructions
    rotation = declarations["rotate_z"]
    declarations["quarter_turn"] = qodec.Instruction(
        "quarter_turn",
        inputs=rotation.inputs,
        outputs=rotation.outputs,
        action=[Clifford({"X_0": "Y_0"})],
    )
    isa.instructions = declarations
    decompose = Mock(side_effect=AssertionError("A direct S gadget must be preserved"))
    resolve = prepare_resolver(InstructionSet(isa), decompose)

    assert resolve("s", (4,), None) == (InstructionCall("quarter_turn", operands=[4]),)
    decompose.assert_not_called()


@pytest.mark.parametrize("error_type", [TypeError, NotImplementedError])
def test_resolver_does_not_treat_other_binding_errors_as_missing(
    monkeypatch, error_type
):
    from qdk.simulation._qodec.operation_resolution import prepare_resolver

    instructions = rotation_instruction_set()
    monkeypatch.setattr(
        instructions, "bind_slots", Mock(side_effect=error_type("invalid binding"))
    )
    decompose = Mock()
    resolve = prepare_resolver(instructions, decompose)

    with pytest.raises(error_type, match="invalid binding"):
        resolve("t", (0,), None)
    decompose.assert_not_called()


@pytest.mark.parametrize(
    "expression, width, characters, phase",
    [
        ("X_0 Z_2", 5, "XIZII", 1),
        ("-Y_0", 3, "YII", -1),
        ("I_4", 0, "IIIII", 1),
        ("", 2, "II", 1),
        ("iX_0", 1, "X", 1j),
        ("X_2", 1, "IIX", 1),
    ],
)
def test_pauli_uses_paulimer_with_explicit_width_and_phase(
    expression, width, characters, phase
):
    from paulimer import DensePauli

    from qdk.simulation._qodec.instruction_set import pauli

    operator = pauli(expression, width)

    assert isinstance(operator, DensePauli)
    assert operator.characters == characters
    assert operator.phase == phase
    assert operator.size == len(characters)


def test_paulimer_pauli_algebra_preserves_phases_and_commutation():
    from qdk.simulation._qodec.instruction_set import pauli

    product = pauli("X_0", 2) * pauli("Y_0", 2)

    assert product == pauli("iZ_0", 2)
    assert not pauli("X_0", 2).commutes_with(pauli("Z_0", 2))
    assert pauli("X_0", 2).commutes_with(pauli("Z_1", 2))
    assert pauli("Z_0 Z_1", 3).support == [0, 1]


def test_clifford_images_use_paulimer_and_preserve_generator_order():
    from paulimer import CliffordUnitary, DensePauli

    from qdk.simulation._qodec.clifford_semantics import (
        clifford_tableau,
        named_clifford,
    )

    clifford = clifford_tableau({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"}, 2)

    assert isinstance(clifford, CliffordUnitary)
    assert clifford == named_clifford("cx", 2)
    assert clifford.image_x(0) == DensePauli("XX")
    assert clifford.image_z(0) == DensePauli("ZI")
    assert clifford.image_x(1) == DensePauli("IX")
    assert clifford.image_z(1) == DensePauli("ZZ")


def test_paulimer_clifford_composition_preserves_order_and_support():
    from paulimer import DensePauli

    from qdk.simulation._qodec.clifford_semantics import (
        clifford_tableau,
        compose_cliffords,
    )
    from qdk.simulation._qodec.quantum_operations import Operation

    phase_after_hadamard = compose_cliffords(
        [Operation("h", (0,)), Operation("s", (0,))], 1
    )
    assert phase_after_hadamard == clifford_tableau({"X_0": "Z_0", "Z_0": "Y_0"}, 1)

    circuit = compose_cliffords([Operation("h", (2,)), Operation("cx", (2, 0))], 3)
    assert circuit is not None
    assert circuit.image_x(2) == DensePauli("IIZ")
    assert circuit.image_z(2) == DensePauli("XIX")
    assert circuit.image_z(0) == DensePauli("ZIZ")


@pytest.mark.parametrize(
    "generators",
    [
        {"X_0": "Z_0"},
        {"X_0": "iX_0"},
        {"X_0": "X_1"},
    ],
)
def test_invalid_clifford_images_raise_python_errors(generators):
    from qdk.simulation._qodec.clifford_semantics import clifford_tableau

    with pytest.raises(ValueError):
        clifford_tableau(generators, 1)


def clifford_isa(action):
    from qodec.instructions import Block, BlockOperand

    return qodec.InstructionSet(
        "custom-clifford",
        blocks=[Block("qubit", 1)],
        instructions=[
            qodec.Instruction(
                "custom_gate",
                inputs=[BlockOperand("qubit")],
                outputs=[BlockOperand("qubit")],
                action=action,
            )
        ],
    )


@pytest.mark.parametrize("representation", ["three_phases", "pauli_and_phase"])
def test_clifford_binding_matches_composed_action_semantics(representation):
    from qdk.simulation._qodec.instruction_set import InstructionSet
    from qodec.actions import Clifford, Pauli

    phase = Clifford({"X_0": "Y_0"})
    action = (
        [phase, phase, phase]
        if representation == "three_phases"
        else [Pauli("Z_0"), phase]
    )
    instructions = InstructionSet(clifford_isa(action))

    assert instructions.bind("s_adj", 1) == ("custom_gate", {})


def test_instruction_set_preparation_does_not_synthesize_cliffords(monkeypatch):
    from qdk.simulation._qodec import stim_synthesis

    from qdk.simulation._qodec.instruction_set import InstructionSet, UnboundOperation
    from qodec.actions import Clifford

    synthesize = Mock(
        side_effect=AssertionError("Binding must not synthesize a circuit")
    )
    monkeypatch.setattr(stim_synthesis, "synthesize_clifford", synthesize)
    instructions = InstructionSet(
        clifford_isa([Clifford({"X_0": "Z_0", "Z_0": "Y_0"})])
    )

    with pytest.raises(UnboundOperation):
        instructions.bind("h", 1)
    synthesize.assert_not_called()


@pytest.mark.parametrize(
    "name, generators",
    [
        ("x", {"Z_0": "-Z_0"}),
        ("s_adj", {"X_0": "-Y_0"}),
    ],
)
def test_physical_clifford_lowering_preserves_primitive_noise_sites(name, generators):
    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qodec.actions import Clifford
    from qdk.simulation._qodec.quantum_operations import Operation

    runtime = InstructionRuntime(InstructionSet(clifford_isa([Clifford(generators)])))
    emitted = []

    def respond(request):
        emitted.append(request)
        return ()

    assert drive_requests(runtime.execute("custom_gate", (4,), {}), respond) == ()
    assert emitted == [Operation(name, (4,))]


def test_physical_lowering_preserves_declared_clifford_steps():
    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qodec.actions import Clifford
    from qdk.simulation._qodec.quantum_operations import Operation

    phase = Clifford({"X_0": "Y_0"})
    instructions = InstructionSet(clifford_isa([phase, phase, phase]))
    runtime = InstructionRuntime(instructions)
    emitted = []

    def respond(request):
        emitted.append(request)
        return ()

    assert instructions.bind("s_adj", 1) == ("custom_gate", {})
    drive_requests(runtime.execute("custom_gate", (4,), {}), respond)
    assert emitted == [Operation("s", (4,))] * 3


def test_physical_adapter_synthesizes_a_nonprimitive_clifford():
    pytest.importorskip(
        "stim", reason="This test exercises the optional Stim synthesis adapter"
    )

    from qdk.simulation._qodec.clifford_semantics import compose_cliffords
    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qodec.actions import Clifford

    instructions = InstructionSet(
        clifford_isa([Clifford({"X_0": "Z_0", "Z_0": "Y_0"})])
    )
    runtime = InstructionRuntime(instructions)
    operations = []

    def respond(request):
        operations.append(request)
        return ()

    drive_requests(runtime.execute("custom_gate", (0,), {}), respond)
    assert compose_cliffords(operations, 1) == instructions.cliffords["custom_gate"]


def test_synthesized_clifford_operations_preserve_the_action():
    pytest.importorskip(
        "stim", reason="This test exercises the optional Stim synthesis adapter"
    )

    from qdk.simulation._qodec.clifford_semantics import (
        clifford_tableau,
        compose_cliffords,
    )
    from qdk.simulation._qodec.instruction_set import clifford_operations

    operations = clifford_operations({"X_0": "Z_0", "Z_0": "Y_0"}, 1)

    expected = clifford_tableau({"X_0": "Z_0", "Z_0": "Y_0"}, 1)
    assert compose_cliffords(operations, 1) == expected


@pytest.mark.parametrize(
    "name, stim_name, width",
    [
        ("x", "X", 1),
        ("y", "Y", 1),
        ("z", "Z", 1),
        ("h", "H", 1),
        ("s", "S", 1),
        ("s_adj", "S_DAG", 1),
        ("sx", "SQRT_X", 1),
        ("sx_adj", "SQRT_X_DAG", 1),
        ("cx", "CX", 2),
        ("cy", "CY", 2),
        ("cz", "CZ", 2),
        ("swap", "SWAP", 2),
    ],
)
def test_paulimer_primitive_images_agree_with_stim(name, stim_name, width):
    stim = pytest.importorskip(
        "stim", reason="Optional cross-library phase and target-order check"
    )

    from qdk.simulation._qodec.clifford_semantics import lower_clifford, named_clifford
    from qdk.simulation._qodec.quantum_operations import Operation

    clifford = named_clifford(name, width)
    assert clifford is not None
    reference = stim.Tableau.from_named_gate(stim_name)
    for index in range(width):
        for actual, expected in (
            (clifford.image_x(index), reference.x_output(index)),
            (clifford.image_z(index), reference.z_output(index)),
        ):
            assert actual.characters == "".join(
                "IXYZ"[expected[target]] for target in range(width)
            )
            assert actual.phase == expected.sign
    assert lower_clifford(clifford) == (Operation(name, tuple(range(width))),)


def test_only_nonprimitive_synthesis_requires_stim(monkeypatch):
    import sys

    from qdk.simulation._qodec.clifford_semantics import (
        clifford_tableau,
        lower_clifford,
        named_clifford,
    )
    from qdk.simulation._qodec.quantum_operations import Operation

    monkeypatch.setitem(sys.modules, "stim", None)
    primitive = named_clifford("cy", 2)
    assert primitive is not None
    assert lower_clifford(primitive) == (Operation("cy", (0, 1)),)
    nonprimitive = clifford_tableau({"X_0": "Z_0", "Z_0": "Y_0"}, 1)
    with pytest.raises(NotImplementedError, match="optional Stim package"):
        lower_clifford(nonprimitive)


@pytest.mark.parametrize("backend_name", ["full_state_backend", "stabilizer_backend"])
def test_encoded_adaptive_execution_without_legacy_runtime_or_stim(backend_name):
    from pathlib import Path
    import subprocess
    import sys
    from textwrap import dedent

    code = dedent("""
        import sys
        sys.modules["stim"] = None
        legacy_modules = (
            "qdk._adaptive_runtime", "qdk.simulation._engine_noise",
            "qdk.simulation._full_state", "qdk.simulation._stabilizer",
            "_sink",
        )
        for module in legacy_modules:
            sys.modules[module] = None

        import qdk
        import qdk.openqasm
        import qodec
        from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
        from qdk.simulation._qodec._run import run_qir_with_qodec
        from qdk.simulation._qodec.protocols import BlockReference, Invocation, Resources
        from qodec.instructions import InstructionCall
        from qdk.simulation._qodec import quantum_backend

        backend_factory = getattr(quantum_backend, sys.argv[1])
        optional_backend = quantum_backend.tableau_backend(None, 7)
        try:
            optional_backend.start(Resources(qubits=1))
        except NotImplementedError as error:
            assert "optional Stim package" in str(error)
        else:
            raise AssertionError("Tableau startup must report the missing Stim dependency")
        finally:
            optional_backend.close()

        codec = qodec.Qodec.load(sys.argv[2])
        circuits = {
            "prepare_z": "[{R: [0]}, {R: [1]}, {R: [2]}]",
            "measure_z": "[{M: [0]}, {M: [1]}, {M: [2]}]",
            "idle": "[{R: [3]}, {R: [4]}, {CX: [0, 3]}, {CX: [1, 3]}, {CX: [1, 4]}, {CX: [2, 4]}, {M: [3]}, {M: [4]}]",
        }
        for name, source in circuits.items():
            circuit = codec.layers[0].gadgets[name].circuit
            circuit.format = "yaml"
            circuit.source = source

        qir = qdk.openqasm.compile(
            'include "stdgates.inc"; qubit[2] qs; bit[2] rs; '
            'x qs[0]; rs[0] = measure qs[0]; '
            'if (rs[0]) { x qs[1]; } rs[1] = measure qs[1];',
            target_profile=qdk.TargetProfile.Adaptive,
        )
        assert run_qir_with_qodec(qir, codec, None, decoder=prepare_syndrome_decoder, shots=3, seed=7, quantum_backend_factory=backend_factory) == [[qdk.Result.One, qdk.Result.One]] * 3
        decoder = prepare_syndrome_decoder(codec.layers[0])(7)
        try:
            gadget = codec.layers[0].gadgets["measure_z"]
            block = BlockReference(0, 1, gadget.implements.inputs[0].block)
            invocation = Invocation(0, gadget, InstructionCall("measure_z", operands=[0]), (block,), ())
            try:
                next(decoder.decode(invocation, (True, False, False)))
            except StopIteration as completed:
                assert completed.value.readouts == (False,)
            else:
                raise AssertionError("A destructive measurement must not request an output correction")
        finally:
            decoder.close()
        assert sys.modules["stim"] is None
        assert all(sys.modules[module] is None for module in legacy_modules)
    """)
    result = subprocess.run(
        [
            sys.executable,
            "-c",
            code,
            backend_name,
            str(FIXTURES / "repetition3.qodec.yaml"),
        ],
        cwd=Path(__file__).parent,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("dedicated", [False, True])
def test_resolver_binds_phase_gates_by_semantics(dedicated):
    from math import pi

    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qodec.instructions import InstructionCall
    from qdk.simulation._qodec.quantum_operations import decompose_rotations

    instructions = rotation_instruction_set(*(("quarter_phase",) if dedicated else ()))
    resolve = prepare_resolver(instructions, decompose_rotations)
    expected = (
        InstructionCall("quarter_phase", operands=[9])
        if dedicated
        else InstructionCall("rotate_z", operands=[9], arguments={"theta": pi / 4})
    )

    assert resolve("t", (9,), None) == (expected,)


@pytest.mark.parametrize("operation", ["rz", "t"])
def test_resolver_propagates_ambiguous_bindings(operation):
    from math import pi

    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qdk.simulation._qodec.quantum_operations import decompose_rotations

    instructions = rotation_instruction_set("quarter_phase", "another_quarter_phase")
    decompose = Mock(wraps=decompose_rotations)
    resolve = prepare_resolver(instructions, decompose)

    with pytest.raises(ValueError, match="Ambiguous"):
        resolve(operation, (0,), pi / 4 if operation == "rz" else None)

    assert decompose.call_count == (0 if operation == "rz" else 1)


@pytest.mark.parametrize("targets, angle", [((0, 1), None), ((0,), 0.25)])
def test_rotation_decomposition_rejects_invalid_gate_arguments(targets, angle):
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qdk.simulation._qodec.quantum_operations import decompose_rotations

    resolve = prepare_resolver(rotation_instruction_set(), decompose_rotations)

    with pytest.raises(ValueError, match="expects one target and no angle"):
        resolve("t", targets, angle)


@pytest.mark.parametrize(
    "name, axis, turns",
    [
        ("t", "rz", 0.25),
        ("t_adj", "rz", -0.25),
        ("s", "rz", 0.5),
        ("s_adj", "rz", -0.5),
        ("sx", "rx", 0.5),
        ("sx_adj", "rx", -0.5),
    ],
)
def test_rotation_decomposition_is_independent_of_an_isa(name, axis, turns):
    from math import pi

    from qdk.simulation._qodec.quantum_operations import Operation, decompose_rotations

    assert decompose_rotations(Operation(name, (4,))) == (
        Operation(axis, (4,), turns * pi),
    )
    assert decompose_rotations(Operation("h", (4,))) is None


def test_physical_instruction_set_uses_declared_actions():
    from math import pi

    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    instructions = InstructionSet(codec.layers[-1].instruction_set)
    runtime = InstructionRuntime(instructions)
    backend = full_state_backend(None, 7)
    physical_qubits = runtime.required_resources(Resources(qubits=2))
    backend.start(physical_qubits)
    try:
        assert instructions.bind("cx", 2) == ("CX", {})
        assert instructions.bind("rz", 1, pi) == ("rotate_z", {"theta": pi})
        drive_requests(runtime.execute("R", (0,), {}), backend.execute)
        drive_requests(runtime.execute("R", (1,), {}), backend.execute)
        backend.apply("h", (0,))
        drive_requests(
            runtime.execute("rotate_z", (0,), {"theta": pi}), backend.execute
        )
        backend.apply("h", (0,))
        drive_requests(runtime.execute("CX", (0, 1), {}), backend.execute)
        assert drive_requests(runtime.execute("M", (0,), {}), backend.execute) == (
            True,
        )
        assert drive_requests(runtime.execute("M", (1,), {}), backend.execute) == (
            True,
        )
    finally:
        backend.close()


def test_physical_instruction_adapter_emits_data_without_a_backend():
    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.quantum_operations import Operation

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    runtime = InstructionRuntime(InstructionSet(codec.layers[-1].instruction_set))
    requests = runtime.execute("rotate_z", (4,), {"theta": 0.25})

    assert next(requests) == Operation("rz", (4,), 0.25)
    with pytest.raises(StopIteration) as completed:
        requests.send(())
    assert completed.value.value == ()
    assert not hasattr(runtime, "quantum_runtime")
    assert runtime.layout is None
    runtime.start(Resources(qubits=2))
    assert runtime.layout is not None
    assert runtime.layout.free == [0, 1]
    runtime.close()
    assert runtime.layout is None


@pytest.mark.parametrize("logical", [False, True])
@pytest.mark.parametrize("fault", [None, 0, 1, 2])
def test_syndrome_decoder_corrects_single_data_faults(
    logical, fault, prepare_code_decoder
):
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    layer = codec.layers[0]
    readouts = [logical] * 3
    if fault is not None:
        readouts[fault] = not readouts[fault]

    corrections = []
    with closing(prepare_code_decoder(layer)(7)) as decoder:
        decoded = decode_gadget(
            decoder, layer.gadgets["measure_z"], tuple(readouts), corrections
        )

    assert decoded.readouts == (logical,)
    assert corrections == []


@pytest.mark.parametrize("declared", [False, True])
def test_executor_does_not_complete_missing_pauli_gadgets(declared):
    from qodec.actions import Pauli
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.quantum_operations import Operation

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    layer = codec.layers[0]
    declarations = layer.instruction_set.instructions
    declarations.pop("x", None)
    if declared:
        operands = declarations["idle"].inputs
        declarations["x"] = qodec.Instruction(
            "x", inputs=operands, outputs=operands, action=[Pauli("X_0")]
        )
    layer.instruction_set.instructions = declarations
    gadgets = layer.gadgets
    gadgets.pop("x", None)
    layer.gadgets = gadgets
    authored = codec.dumps()
    runtime = LayerRuntime(LayerPlan(layer), prepare_syndrome_decoder(layer)(7))
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    respond = Mock(return_value=())
    try:
        drive_requests(
            runtime.handle(InstructionCall("prepare_z", operands=[0])), respond
        )
        before = (
            dict(runtime.layout.blocks),
            list(runtime.layout.free),
            runtime._next_invocation,
        )
        respond.reset_mock()
        with pytest.raises(NotImplementedError, match="implement"):
            drive_requests(runtime.handle(Operation("x", (0,))), respond)
        respond.assert_not_called()
        assert (
            runtime.layout.blocks,
            runtime.layout.free,
            runtime._next_invocation,
        ) == before
        assert codec.dumps() == authored
    finally:
        runtime.close()


def repetition_runtime(prepare_decoder=None):
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.instruction_set import InstructionRuntime, InstructionSet
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = InstructionRuntime(InstructionSet(codec.layers[-1].instruction_set))
    backend = full_state_backend(None, 7)
    decoder = (prepare_decoder or prepare_syndrome_decoder)(codec.layers[0])(7)
    return LayerRuntime(LayerPlan(codec.layers[0]), decoder), physical, backend


@pytest.mark.parametrize(
    "arguments, message",
    [
        ({"unknown": 0.5}, "Unknown parameter"),
        ({}, "Missing parameter"),
        ({"theta": True}, "expects number"),
        ({"theta": "unbound"}, "expects number"),
    ],
)
def test_native_call_arguments_fail_before_side_effects(arguments, message):
    from qodec.instructions import InstructionCall

    runtime, _, backend = repetition_runtime()
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    respond = Mock(return_value=())
    try:
        drive_requests(
            runtime.handle(InstructionCall("prepare_z", operands=[0])), respond
        )
        before = (
            dict(runtime.layout.blocks),
            list(runtime.layout.free),
            runtime._next_invocation,
        )
        respond.reset_mock()
        with pytest.raises((TypeError, ValueError), match=message):
            drive_requests(
                runtime.handle(
                    InstructionCall("rotate_z", operands=[0], arguments=arguments)
                ),
                respond,
            )
        respond.assert_not_called()
        assert (
            runtime.layout.blocks,
            runtime.layout.free,
            runtime._next_invocation,
        ) == before
    finally:
        runtime.close()
        backend.close()


@pytest.mark.parametrize("unrelated", ["x_preparation", "conditional_action"])
def test_supplied_gadgets_ignore_unrelated_instruction_actions(unrelated):
    from qodec.actions import Condition, Pauli, Stabilize
    from qodec.instructions import InstructionCall, Parameter

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    layer = codec.layers[0]
    declarations = layer.instruction_set.instructions
    operands = declarations["prepare_z"].outputs
    declarations["unrelated"] = (
        qodec.Instruction("unrelated", outputs=operands, action=[Stabilize(["X_0"])])
        if unrelated == "x_preparation"
        else qodec.Instruction(
            "unrelated",
            inputs=operands,
            outputs=operands,
            parameters=[Parameter("correction", "bit")],
            action=[Pauli("X_0", condition=Condition(["correction"]))],
        )
    )
    layer.instruction_set.instructions = declarations
    codec.validate()
    runtime = LayerRuntime(LayerPlan(layer), prepare_syndrome_decoder(layer)(7))
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    emitted = []

    def respond(request):
        emitted.append(request)
        return (
            (False,)
            if isinstance(request, InstructionCall) and request.mnemonic == "M"
            else ()
        )

    try:
        assert (
            drive_requests(
                runtime.handle(InstructionCall("prepare_z", operands=["data"])), respond
            )
            == ()
        )
        assert drive_requests(
            runtime.handle(InstructionCall("measure_z", operands=["data"])), respond
        ) == (False,)
        assert [
            request.mnemonic
            for request in emitted
            if isinstance(request, InstructionCall)
        ] == ["R"] * 3 + ["M"] * 3
        assert runtime.layout.blocks == {}
    finally:
        runtime.close()


@pytest.mark.parametrize("fails", [False, True])
def test_layer_executes_prepared_bodies_without_parsing_source(fails):
    from qodec.gadgets import Circuit
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.protocols import (
        Corrections,
        Decoded,
        Invocation,
        PreparedCircuit,
        Readouts,
        Requests,
    )
    from qdk.simulation._qodec.quantum_operations import Operation

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    gadget = layer.gadgets["prepare_z"]
    gadget.circuit.format = "scripted"
    gadget.circuit.source = "opaque source"
    layer.gadgets = {"prepare_z": gadget}
    prepared_sources = []
    instances = []
    invocations = []
    observations = []
    cleanup = []
    emitted = []

    class BodyRuntime:
        def __init__(self) -> None:
            instances.append(self)

        def required_resources(self, invocation: Invocation) -> Resources:
            return Resources(qubits=4)

        def run(self, invocation: Invocation) -> Requests[Readouts]:
            invocations.append(invocation)
            try:
                yield InstructionCall("R", operands=["scratch"])
                readouts = yield InstructionCall("M", operands=["scratch"])
                if readouts is None or readouts[0] is None:
                    raise TypeError("Missing resolved measurement reply")
                yield Operation("x", (0,))
                return (not readouts[0],)
            finally:
                cleanup.append("generator")

        def close(self) -> None:
            cleanup.append("runtime")

    def prepare(circuit: Circuit) -> PreparedCircuit:
        prepared_sources.append(circuit)
        return PreparedCircuit(("0", "1", "2", "scratch"), BodyRuntime)

    class Decoder:
        def decode(
            self, invocation: Invocation, readouts: Readouts
        ) -> Corrections[Decoded]:
            observations.append(readouts)
            yield from ()
            return Decoded(())

        def close(self) -> None:
            pass

    def respond(request):
        emitted.append(request)
        if isinstance(request, InstructionCall) and request.mnemonic == "M":
            if fails:
                raise RuntimeError("body child failed")
            return (False,)
        return ()

    plan = LayerPlan(layer, prepare_circuit=prepare)
    runtime = LayerRuntime(plan, Decoder())
    assert runtime.required_resources(Resources(qubits=1)) == Resources(qubits=4)
    runtime.start(Resources(qubits=4))
    try:
        if fails:
            with pytest.raises(RuntimeError, match="body child failed"):
                drive_requests(
                    runtime.handle(InstructionCall("prepare_z", operands=["data"])),
                    respond,
                )
            assert observations == []
        else:
            for _ in range(2):
                assert (
                    drive_requests(
                        runtime.handle(InstructionCall("prepare_z", operands=["data"])),
                        respond,
                    )
                    == ()
                )
            assert observations == [(True,), (True,)]
            assert len(instances) == 2 and instances[0] is not instances[1]
            assert [invocation.id for invocation in invocations] == [0, 1]
            assert all(invocation.gadget == gadget for invocation in invocations)
            assert Operation("x", (0,)) in emitted
        assert prepared_sources == [gadget.circuit]
        assert emitted[:2] == [
            InstructionCall("R", operands=[3]),
            InstructionCall("M", operands=[3]),
        ]
        assert cleanup == ["generator", "runtime"] * len(instances)
    finally:
        runtime.close()


def test_layer_can_lower_and_decode_without_a_downstream_runtime():
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    decoder = prepare_syndrome_decoder(layer)(7)
    assert isinstance(decoder, SyndromeSession)
    runtime = LayerRuntime(LayerPlan(layer), decoder)
    emitted = []

    def respond(request):
        emitted.append(request)
        if isinstance(request, InstructionCall) and request.mnemonic == "M":
            return (request.operands[0] == 0,)
        return ()

    lower_qubits = runtime.required_resources(Resources(qubits=1))
    assert lower_qubits == Resources(qubits=5)
    assert runtime.start(lower_qubits) is None
    assert runtime.layout.free == list(range(lower_qubits.qubits))
    try:
        drive_requests(runtime.prepare(0), respond)
        assert runtime.layout.blocks[0].support == (0, 1, 2)
        assert drive_requests(runtime.measure(0), respond) is False
        assert runtime.layout.blocks == {}
        assert [
            request.mnemonic
            for request in emitted
            if isinstance(request, InstructionCall)
        ] == ["R"] * 3 + ["M"] * 3
        assert not hasattr(runtime, "lower")
    finally:
        runtime.close()
    assert decoder.closed


def test_layer_accepts_a_protocol_only_decoder_without_a_classical_runtime():
    from typing_extensions import assert_type

    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.protocols import (
        Corrections,
        Decoded,
        DecoderSession,
        ExecutionLayer,
        Invocation,
        Readouts,
        Request,
    )
    from qodec.instructions import InstructionCall
    from qdk.simulation._qodec.quantum_operations import Operation

    class RecordingDecoder:
        def __init__(self) -> None:
            self.observations: list[Readouts] = []
            self.close_count = 0

        def decode(
            self, invocation: Invocation, readouts: Readouts
        ) -> Corrections[Decoded]:
            self.observations.append(readouts)
            yield from ()
            return Decoded((True,) * invocation.gadget.implements.observe_count)

        def close(self) -> None:
            self.close_count += 1

    def respond(request: Request) -> Readouts:
        if isinstance(request, InstructionCall) and request.mnemonic == "M":
            return (False,)
        return ()

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    decoder = RecordingDecoder()
    runtime = LayerRuntime(LayerPlan(layer), decoder)
    execution: ExecutionLayer = runtime
    assert_type(runtime.decoder, DecoderSession)
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    try:
        drive_requests(execution.handle(Operation("prepare", (0,))), respond)
        assert drive_requests(
            execution.handle(Operation("measure", (0,))), respond
        ) == (True,)
        assert decoder.observations == [(), (False, False, False)]
    finally:
        runtime.close()
    assert decoder.close_count == 1


def test_decoder_hooks_track_lifetimes_and_correct_other_live_blocks():
    from qodec.instructions import InstructionCall

    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.protocols import (
        Correction,
        Corrections,
        Decoded,
        Invocation,
        Readouts,
    )
    from qdk.simulation._qodec.quantum_operations import Operation

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    inner = prepare_syndrome_decoder(layer)(7)
    invocations = []
    discarded = []
    events = []
    emitted = []
    live = {}

    class Decoder:
        def before(self, invocation: Invocation) -> Corrections[None]:
            if invocation.call.mnemonic == "idle":
                yield Correction((live[0], live[1]), Operation("cx", (0, 3)))
                events.append("before confirmed")

        def decode(
            self, invocation: Invocation, readouts: Readouts
        ) -> Corrections[Decoded]:
            invocations.append(invocation)
            live.update((block.label, block) for block in invocation.outputs)
            decoded = yield from inner.decode(invocation, readouts)
            if invocation.call.mnemonic == "idle":
                yield Correction((live[1],), Operation("x", (1,)))
                events.append("after confirmed")
            return decoded

        def discarded(self, blocks) -> None:
            discarded.extend(blocks)

        def close(self) -> None:
            inner.close()

    def respond(request):
        emitted.append(request)
        events.append(
            request.name if isinstance(request, Operation) else request.mnemonic
        )
        return (
            (False,)
            if isinstance(request, InstructionCall) and request.mnemonic == "M"
            else ()
        )

    runtime = LayerRuntime(LayerPlan(layer), Decoder())
    runtime.start(runtime.required_resources(Resources(qubits=2)))
    try:
        drive_requests(runtime.prepare(0), respond)
        drive_requests(runtime.prepare(1), respond)
        events.clear()
        emitted.clear()
        drive_requests(runtime.execute("idle", (0,), {}), respond)

        assert emitted[0] == Operation("cx", (0, 3))
        assert events[:3] == ["cx", "before confirmed", "R"]
        assert Operation("x", (4,)) in emitted
        assert events.index("x") < events.index("after confirmed")
        assert invocations[2].inputs == invocations[0].outputs
        assert invocations[2].outputs == invocations[0].outputs

        original = invocations[0].outputs[0]
        drive_requests(
            runtime.handle(InstructionCall("prepare_z", operands=[0])), respond
        )
        replacement = invocations[-1].outputs[0]
        assert replacement.label == original.label
        assert replacement.generation == original.generation + 1
        assert discarded == [original]
        assert [invocation.id for invocation in invocations] == list(range(4))
        assert runtime.layout.blocks[0].support == (0, 1, 2)
    finally:
        runtime.close()


@pytest.mark.parametrize(
    "failure, error_type, message",
    [
        ("backend", RuntimeError, "correction failed"),
        ("missing_reply", TypeError, "readout tuple"),
        ("stale", ValueError, "no longer live"),
        ("out_of_range", ValueError, "out-of-range code qubit"),
    ],
)
def test_failed_corrections_are_closed_without_acknowledgement(
    failure, error_type, message
):
    from dataclasses import replace

    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime
    from qdk.simulation._qodec.protocols import (
        Correction,
        Corrections,
        Decoded,
        Invocation,
        Readouts,
    )
    from qdk.simulation._qodec.quantum_operations import Operation

    acknowledged = []
    closed = []

    class Decoder:
        def decode(
            self, invocation: Invocation, readouts: Readouts
        ) -> Corrections[Decoded]:
            reference = invocation.outputs[0]
            if failure == "stale":
                reference = replace(reference, generation=reference.generation - 1)
            target = 3 if failure == "out_of_range" else 0
            try:
                reply = yield Correction((reference,), Operation("x", (target,)))
                acknowledged.append(reply)
                return Decoded(())
            finally:
                closed.append(True)

        def close(self) -> None:
            pass

    def respond(request):
        if isinstance(request, Operation) and request.name == "x":
            if failure == "backend":
                raise RuntimeError("correction failed")
            return None
        return ()

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    runtime = LayerRuntime(LayerPlan(layer), Decoder())
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    try:
        with pytest.raises(error_type, match=message):
            drive_requests(runtime.prepare(0), respond)
        assert acknowledged == []
        assert closed == [True]
    finally:
        runtime.close()


def test_unknown_selection_flags_are_rejected_before_execution():
    from qodec.instructions import InstructionCall

    runtime, _, backend = repetition_runtime()
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    respond = Mock(return_value=())
    try:
        with pytest.raises(ValueError, match="Unknown selection flag"):
            drive_requests(
                runtime.handle(
                    InstructionCall("prepare_z", operands=[0], select=[{"reject": 0}])
                ),
                respond,
            )
        respond.assert_not_called()
        assert runtime.layout.blocks == {}
    finally:
        runtime.close()
        backend.close()


def test_layer_rejects_a_missing_instruction_reply():
    runtime, _, backend = repetition_runtime()
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    try:
        with closing(runtime.prepare(0)) as requests:
            next(requests)
            with pytest.raises(TypeError, match="readout tuple"):
                requests.send(None)
    finally:
        runtime.close()
        backend.close()


@pytest.mark.parametrize("readout", [False, True, None])
def test_call_list_requires_resolved_readout_arguments(readout):
    from qdk.simulation._qodec.circuit_runtime import _argument
    from qdk.simulation._qodec.protocols import ExecutionUnresolved

    if readout is None:
        with pytest.raises(ExecutionUnresolved, match="unresolved readout"):
            _argument("circuit.readouts[0]", {}, (readout,))
    else:
        assert _argument("circuit.readouts[0]", {}, (readout,)) is readout


def test_layer_uses_its_prepared_resolver_for_phase_gates():
    from math import pi

    from qodec.instructions import InstructionCall

    runtime, _, backend = repetition_runtime()
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    emitted = []

    def respond(request):
        emitted.append(request)
        return ()

    try:
        drive_requests(runtime.prepare("data"), respond)
        emitted.clear()

        drive_requests(runtime.apply("t", ("data",)), respond)

        assert emitted == [
            InstructionCall("rotate_z", operands=[0], arguments={"theta": pi / 4})
        ]
        assert runtime.layout.blocks["data"].support == (0, 1, 2)
    finally:
        runtime.close()
        backend.close()


@pytest.mark.parametrize("supported", [False, True])
def test_layer_resolves_the_entire_decomposition_before_execution(supported):
    from qdk.simulation._qodec.instruction_set import InstructionSet, UnboundOperation
    from qdk.simulation._qodec.operation_resolution import prepare_resolver
    from qodec.instructions import InstructionCall
    from qdk.simulation._qodec.quantum_operations import Operation

    runtime, _, backend = repetition_runtime()

    def decompose(operation):
        return (
            Operation("rz", operation.targets, 0.1),
            (
                Operation("rz", operation.targets, 0.2)
                if supported
                else Operation("h", operation.targets)
            ),
        )

    runtime.plan.resolve = prepare_resolver(
        InstructionSet(runtime.plan.instruction_set), decompose
    )
    runtime.start(runtime.required_resources(Resources(qubits=1)))
    emitted = []

    def respond(request):
        emitted.append(request)
        return ()

    try:
        drive_requests(runtime.prepare(0), respond)
        emitted.clear()
        if supported:
            drive_requests(runtime.apply("phase_pair", (0,)), respond)
            assert emitted == [
                InstructionCall("rotate_z", operands=[0], arguments={"theta": 0.1}),
                InstructionCall("rotate_z", operands=[0], arguments={"theta": 0.2}),
            ]
        else:
            with pytest.raises(UnboundOperation, match="does not implement 'h'"):
                drive_requests(runtime.apply("phase_pair", (0,)), respond)
            assert emitted == []
        assert runtime.layout.blocks[0].support == (0, 1, 2)
    finally:
        runtime.close()
        backend.close()


def test_declared_pauli_gadgets_reuse_prepared_calls_without_mutation(monkeypatch):
    from qdk.simulation._qodec import instruction_set
    from qodec.instructions import InstructionCall
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.layer_runtime import LayerPlan, LayerRuntime

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    plan = LayerPlan(layer)
    decoder_factory = prepare_syndrome_decoder(layer)
    plan.resolve
    monkeypatch.setattr(
        instruction_set,
        "pauli",
        Mock(side_effect=AssertionError("Pauli actions must be prepared")),
    )
    expected = {
        "x": (
            InstructionCall("X", operands=[0]),
            InstructionCall("X", operands=[1]),
            InstructionCall("X", operands=[2]),
        ),
        "y": (
            InstructionCall("Y", operands=[0]),
            InstructionCall("X", operands=[1]),
            InstructionCall("X", operands=[2]),
        ),
        "z": (InstructionCall("Z", operands=[0]),),
    }
    emitted = []

    def respond(request):
        emitted.append(request)
        return ()

    for seed in (7, 8):
        runtime = LayerRuntime(plan, decoder_factory(seed))
        runtime.start(runtime.required_resources(Resources(qubits=1)))
        try:
            drive_requests(runtime.prepare(0), respond)
            for operation in ("y", "z", "x", "y", "z"):
                emitted.clear()
                drive_requests(runtime.apply(operation, (0,)), respond)
                assert tuple(emitted) == expected[operation]
        finally:
            runtime.close()


def test_layer_lowers_logical_paulis_and_lifts_measurements():
    runtime, physical, backend = repetition_runtime()
    lower_qubits = runtime.required_resources(Resources(qubits=2))
    physical_qubits = physical.required_resources(lower_qubits)
    runtime.start(lower_qubits)
    backend.start(physical_qubits)

    def execute_lower(request):
        return drive_requests(physical.handle(request), backend.execute)

    try:
        drive_requests(runtime.prepare(0), execute_lower)
        drive_requests(runtime.prepare(1), execute_lower)
        drive_requests(runtime.apply("x", (0,)), execute_lower)
        drive_requests(runtime.apply("rz", (1,), angle=0.25), execute_lower)

        assert drive_requests(runtime.measure(0), execute_lower) is True
        assert drive_requests(runtime.measure(1), execute_lower) is False
        assert runtime.layout.blocks == {}
    finally:
        runtime.close()
        backend.close()


def test_idle_materializes_correction_before_continuing(prepare_code_decoder):
    runtime, physical, backend = repetition_runtime(prepare_code_decoder)
    lower_qubits = runtime.required_resources(Resources(qubits=1))
    physical_qubits = physical.required_resources(lower_qubits)
    runtime.start(lower_qubits)
    backend.start(physical_qubits)

    def execute_lower(request):
        return drive_requests(physical.handle(request), backend.execute)

    try:
        drive_requests(runtime.prepare(0), execute_lower)
        data = runtime.layout.blocks[0].support
        backend.apply("x", (data[1],))

        drive_requests(runtime.execute("idle", (0,), {}), execute_lower)

        assert tuple(backend.measure(target) for target in data) == (False,) * 3
    finally:
        runtime.close()
        backend.close()


@pytest.mark.parametrize("backend_factory", [full_state_backend, stabilizer_backend])
def test_run_qir_formats_encoded_adaptive_results_for_each_shot(backend_factory):
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[2] qs;
        bit[2] rs;
        x qs[0];
        rs[0] = measure qs[0];
        if (rs[0]) { x qs[1]; }
        rs[1] = measure qs[1];
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )

    results = run_qir_with_qodec(
        qir,
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        None,
        decoder=prepare_syndrome_decoder,
        shots=3,
        seed=7,
        quantum_backend_factory=backend_factory,
    )

    assert results == [[qdk.Result.One, qdk.Result.One]] * 3


def test_discard_does_not_apply_reset_noise():
    noise = simulation.NoiseConfig()
    noise.mresetz.x = 1
    backend = full_state_backend(noise, 7)
    backend.start(Resources(qubits=1))
    try:
        backend.prepare(0)
        backend.discard(0)

        assert backend.measure(0) is False
    finally:
        backend.close()


@pytest.mark.parametrize(
    "backend_factory", [full_state_backend, stabilizer_backend, tableau_backend]
)
def test_seeded_noise_replays_with_independent_shots(backend_factory):
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[1] qs;
        x qs[0];
        bit[1] rs = measure qs;
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    noise = simulation.NoiseConfig()
    noise.x.x = 0.25

    first = run_qir_with_qodec(
        qir,
        codec,
        noise,
        decoder=prepare_syndrome_decoder,
        shots=40,
        seed=7,
        quantum_backend_factory=backend_factory,
    )
    replay = run_qir_with_qodec(
        qir,
        codec,
        noise,
        decoder=prepare_syndrome_decoder,
        shots=40,
        seed=7,
        quantum_backend_factory=backend_factory,
    )
    other = run_qir_with_qodec(
        qir,
        codec,
        noise,
        decoder=prepare_syndrome_decoder,
        shots=40,
        seed=8,
        quantum_backend_factory=backend_factory,
    )

    assert first == replay
    assert first != other
    assert [qdk.Result.Zero] in first
    assert [qdk.Result.One] in first


def nested_repetition_qodec():
    from qodec.gadgets import Circuit, Encoding

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    logical, physical = codec.layers
    lower_isa = physical.instruction_set
    middle_isa = qodec.InstructionSet(
        "encoded-qubit", blocks=lower_isa.blocks, instructions=lower_isa.instructions
    )
    gadgets = {}
    for name, source in (
        ("R", "prepare_z"),
        ("M", "measure_z"),
        ("rotate_z", "rotate_z"),
        ("X", "x"),
        ("Y", "y"),
        ("Z", "z"),
    ):
        gadget = logical.gadgets[source]
        gadgets[name] = qodec.Gadget(
            middle_isa.instructions[name],
            Circuit(lower_isa, gadget.circuit.source, format=gadget.circuit.format),
            inputs=gadget.inputs,
            outputs=gadget.outputs,
            checks=gadget.checks,
            readouts=gadget.readouts,
            parameter_bindings=dict(gadget.parameter_bindings),
        )
    code = logical.codes["repetition3"]
    encodings = [
        Encoding(code, support=["0", "1", "2"]),
        Encoding(code, support=["3", "4", "5"]),
    ]
    gadgets["CX"] = qodec.Gadget(
        middle_isa.instructions["CX"],
        Circuit(lower_isa, "CX 0 3 1 4 2 5", format="stim"),
        inputs=encodings,
        outputs=encodings,
    )
    middle = qodec.Layer(middle_isa, gadgets=gadgets, codes={"qubit": code})
    for gadget in logical.gadgets.values():
        gadget.circuit.instruction_set = middle_isa
    return qodec.Qodec([logical, middle, physical])


def test_factory_prepares_physical_operations_once_with_separate_translators(
    monkeypatch,
):
    from types import MappingProxyType

    from qdk.simulation._qodec import instruction_set
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.instruction_set import InstructionRuntime

    lower = Mock(wraps=instruction_set.action_operations)
    monkeypatch.setattr(instruction_set, "action_operations", lower)
    factory = ExecutionPipelineFactory(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        prepare_syndrome_decoder,
        None,
        classical_runtime_factory=AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    )
    assert lower.call_count == len(factory.physical.declarations)
    lower.side_effect = AssertionError("Shot construction must not lower instructions")
    pipelines = [factory.build_pipeline(), factory.build_pipeline()]
    try:
        first = pipelines[0].layers[-1]
        second = pipelines[1].layers[-1]
        assert isinstance(first, InstructionRuntime)
        assert isinstance(second, InstructionRuntime)
        assert first is not second
        assert first.operations is second.operations
        assert isinstance(first.operations, MappingProxyType)

        probe = Mock(wraps=first.handle)
        monkeypatch.setattr(first, "handle", probe)
        bytecode = compile_qasm("""
            include "stdgates.inc";
            qubit target;
            x target;
            bit readout = measure target;
        """)
        assert pipelines[0].run(bytecode) == [qdk.Result.One]
        first_calls = probe.call_count
        assert first_calls > 0
        assert pipelines[1].run(bytecode) == [qdk.Result.One]
        assert probe.call_count == first_calls
    finally:
        for pipeline in pipelines:
            if not pipeline.closed:
                pipeline._close()


def test_factory_replays_seeds_with_layers_in_execution_order():
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.layer_runtime import LayerRuntime
    from qdk.simulation._qodec.quantum_backend import QuantumBackend

    codec = nested_repetition_qodec()
    prepared_layers = []
    started = []

    def prepare_decoder(layer):
        layer_index = len(prepared_layers)
        prepared_layers.append(layer)
        create_decoder = prepare_syndrome_decoder(layer)

        def create_session(seed):
            started.append((layer_index, seed))
            return create_decoder(seed)

        return create_session

    factory = ExecutionPipelineFactory(
        codec,
        prepare_decoder,
        None,
        classical_runtime_factory=AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    )

    runs = []
    for seed in (7, 7, 8):
        factory.set_seed(seed)
        shots = []
        for _ in range(2):
            started.clear()
            pipeline = factory.build_pipeline()
            try:
                assert [index for index, seed in started] == [0, 1]
                plans = []
                for layer in pipeline.layers[1:-1]:
                    assert isinstance(layer, LayerRuntime)
                    plans.append(layer.plan)
                assert tuple(plans) == tuple(
                    plan for plan, create_decoder in factory.prepared
                )
                backend = pipeline.quantum_backend
                assert isinstance(backend, QuantumBackend)
                shots.append((backend.seed, tuple(started)))
            finally:
                pipeline._close()
        runs.append(shots)

    assert prepared_layers == codec.layers[:-1]
    assert runs[0] == runs[1]
    assert runs[0] != runs[2]
    assert runs[0][0] != runs[0][1]


def test_nested_resource_counts_expand_at_each_encoding_layer():
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory

    pipeline = ExecutionPipelineFactory(
        nested_repetition_qodec(),
        prepare_syndrome_decoder,
        None,
        classical_runtime_factory=AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    ).build_pipeline()
    try:
        assert pipeline.resource_counts(Resources(qubits=5)) == tuple(
            Resources(qubits=count) for count in (5, 5, 17, 51, 51)
        )
    finally:
        pipeline._close()


@pytest.mark.parametrize("backend_factory", [full_state_backend, stabilizer_backend])
def test_nested_layers_run_without_flattening_the_qodec(backend_factory):
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[1] qs;
        x qs[0];
        bit[1] rs = measure qs;
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )

    results = run_qir_with_qodec(
        qir,
        nested_repetition_qodec(),
        None,
        decoder=prepare_syndrome_decoder,
        shots=2,
        seed=7,
        quantum_backend_factory=backend_factory,
    )

    assert results == [[qdk.Result.One]] * 2


def test_decoding_reduces_logical_errors_under_the_same_noise():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.protocols import Decoded

    class RawReadoutSession:
        def __init__(self):
            self.closed = False

        def decode(self, invocation, readouts):
            yield from ()
            return Decoded(
                tuple(readouts[: invocation.gadget.implements.observe_count])
            )

        def close(self):
            self.closed = True

    def prepare_raw_decoder(layer):
        return lambda seed: RawReadoutSession()

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[1] qs;
        x qs[0];
        bit[1] rs = measure qs;
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    noise = simulation.NoiseConfig()
    noise.x.x = 0.1

    corrected = run_qir_with_qodec(
        qir,
        codec,
        noise,
        decoder=prepare_syndrome_decoder,
        shots=100,
        seed=7,
        quantum_backend_factory=full_state_backend,
    )
    uncorrected = run_qir_with_qodec(
        qir,
        codec,
        noise,
        decoder=prepare_raw_decoder,
        shots=100,
        seed=7,
        quantum_backend_factory=full_state_backend,
    )

    assert corrected.count([qdk.Result.Zero]) < uncorrected.count([qdk.Result.Zero])


def test_decoder_reuses_boundary_operators_without_parsing(monkeypatch):
    from qdk.simulation._qodec import decoding
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    decoder_factory = prepare_syndrome_decoder(layer)
    monkeypatch.setattr(
        decoding,
        "pauli",
        Mock(side_effect=AssertionError("Boundary Paulis must be prepared")),
    )
    for seed in (7, 8):
        session = decoder_factory(seed)
        try:
            for logical in (False, True):
                for fault in (0, 1, 2):
                    readouts = [logical] * 3
                    readouts[fault] = not readouts[fault]
                    assert decode_gadget(
                        session, layer.gadgets["measure_z"], tuple(readouts)
                    ).readouts == (logical,)
        finally:
            session.close()


def test_returned_correction_cannot_mutate_shared_decoder_state():
    from paulimer import DensePauli

    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    decoder_factory = prepare_syndrome_decoder(layer)
    first = decoder_factory(7)
    second = decoder_factory(8)
    try:
        assert isinstance(first, SyndromeSession)
        initial = []
        decode_gadget(first, layer.gadgets["idle"], (True, False), initial)
        code = first.model.gadgets["idle"][0][("out", 0)]
        correction = code.correct((True, False))
        expected = correction.copy()
        correction *= DensePauli.z(2, 3)

        for session in (first, second):
            repeated = []
            decode_gadget(session, layer.gadgets["idle"], (True, False), repeated)
            assert repeated == initial
            assert code.correct((True, False)) == expected
            assert decode_gadget(
                session, layer.gadgets["measure_z"], (True, False, False)
            ).readouts == (False,)
    finally:
        first.close()
        second.close()


def test_decoder_models_are_prepared_once_and_sessions_close():
    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    prepared_layers = []
    sessions = []

    def prepare_decoder(layer):
        prepared_layers.append(layer)
        create_decoder = prepare_syndrome_decoder(layer)

        def create_session(seed):
            session = create_decoder(seed)
            assert isinstance(session, SyndromeSession)
            sessions.append((seed, session))
            return session

        return create_session

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[1] qs;
        bit[1] rs = measure qs;
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )

    assert (
        run_qir_with_qodec(
            qir,
            codec,
            None,
            decoder=prepare_decoder,
            shots=3,
            seed=7,
            quantum_backend_factory=full_state_backend,
        )
        == [[qdk.Result.Zero]] * 3
    )
    assert prepared_layers == [codec.layers[0]]
    assert len({seed for seed, session in sessions}) == 3
    assert len({id(session) for seed, session in sessions}) == 3
    assert all(session.closed for seed, session in sessions)


def test_decoder_closes_when_a_logical_gate_is_unsupported():
    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    session = prepare_syndrome_decoder(codec.layers[0])(7)
    assert isinstance(session, SyndromeSession)
    prepare_decoder = Mock(return_value=lambda seed: session)
    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[1] qs;
        h qs[0];
        bit[1] rs = measure qs;
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )

    with pytest.raises(NotImplementedError, match="does not implement 'h'"):
        run_qir_with_qodec(
            qir,
            codec,
            None,
            decoder=prepare_decoder,
            seed=7,
            quantum_backend_factory=full_state_backend,
        )

    assert session.closed


def test_adaptive_branch_uses_decoded_not_raw_measurement(monkeypatch):
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.quantum_backend import QuantumBackend

    prepare = QuantumBackend.prepare
    injected = []

    def prepare_with_fault(backend, target):
        prepare(backend, target)
        if target == 0:
            backend.apply("x", (target,))
            injected.append(target)

    monkeypatch.setattr(QuantumBackend, "prepare", prepare_with_fault)
    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[2] qs;
        bit[2] rs;
        rs[0] = measure qs[0];
        if (rs[0]) { x qs[1]; }
        rs[1] = measure qs[1];
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )

    results = run_qir_with_qodec(
        qir,
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        None,
        decoder=prepare_syndrome_decoder,
        seed=7,
        quantum_backend_factory=full_state_backend,
    )

    assert injected == [0]
    assert results == [[qdk.Result.Zero, qdk.Result.Zero]]


def test_erased_readouts_remain_explicitly_unavailable():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    session = prepare_syndrome_decoder(layer)(7)

    try:
        assert decode_gadget(
            session, layer.gadgets["measure_z"], (None, False, False)
        ).readouts == (None,)
    finally:
        session.close()


def test_empty_qodec_is_rejected():
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory

    with pytest.raises(ValueError, match="physical instruction set"):
        ExecutionPipelineFactory(
            qodec.Qodec([]),
            prepare_syndrome_decoder,
            None,
            classical_runtime_factory=AdaptiveRuntime,
            quantum_backend_factory=full_state_backend,
        )


def test_nondestructive_measurement_does_not_trigger_repreparation():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
    from qdk.simulation._qodec.layer_runtime import LayerRuntime
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    logical, physical = codec.layers
    declarations = physical.instruction_set.instructions
    measurement = declarations["M"]
    declarations["M"] = qodec.Instruction(
        "M",
        inputs=measurement.inputs,
        outputs=measurement.inputs,
        action=measurement.action,
    )
    physical.instruction_set.instructions = declarations
    gadget = logical.gadgets["measure_z"]
    instruction = qodec.Instruction(
        "measure_z",
        inputs=gadget.implements.inputs,
        outputs=gadget.implements.inputs,
        action=gadget.implements.action,
    )
    declarations = logical.instruction_set.instructions
    declarations["measure_z"] = instruction
    logical.instruction_set.instructions = declarations
    gadget.implements = instruction
    gadget.outputs = gadget.inputs
    codec.validate()
    pipeline = ExecutionPipelineFactory(
        codec, prepare_syndrome_decoder, None, AdaptiveRuntime, full_state_backend
    ).build_pipeline()
    layer = pipeline.layers[1]
    assert isinstance(layer, LayerRuntime)
    executions = []
    original = layer.execute

    def record(mnemonic, targets, arguments, **kwargs):
        executions.append(mnemonic)
        return (yield from original(mnemonic, targets, arguments, **kwargs))

    layer.execute = record
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit first = measure data; x data; bit second = measure data;'
    )
    assert pipeline.run(program) == [qdk.Result.Zero, qdk.Result.One]
    assert executions == ["prepare_z", "measure_z", "x", "measure_z"]


def test_repeated_measurements_execute_fresh_gadgets():
    from qdk.simulation._qodec.decoding import prepare_syndrome_decoder

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[1] qs;
        bit[2] rs;
        rs[0] = measure qs[0];
        rs[1] = measure qs[0];
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )
    noise = simulation.NoiseConfig()
    noise.mresetz.x = 1

    results = run_qir_with_qodec(
        qir,
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        noise,
        decoder=prepare_syndrome_decoder,
        seed=7,
        quantum_backend_factory=full_state_backend,
    )

    assert results == [[qdk.Result.One, qdk.Result.Zero]]


def test_partial_syndromes_do_not_invent_unobserved_boundary_signs(
    prepare_code_decoder,
):
    from qdk.simulation._qodec.decoding import SyndromeSession

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    gadget = layer.gadgets["idle"]
    gadget.checks = gadget.checks[:-1]
    session = prepare_code_decoder(layer)(7)
    assert isinstance(session, SyndromeSession)
    try:
        assert decode_gadget(session, gadget, (True, False)).readouts == ()
        boundary = session.boundaries[invocation_for(gadget).outputs[0]]
        assert boundary[("stabilizers", 0)] is False
        assert ("stabilizers", 1) not in boundary
    finally:
        session.close()


def test_factory_exposes_independent_stages_for_request_and_readout_probes(monkeypatch):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.instruction_set import InstructionRuntime
    from qdk.simulation._qodec.layer_runtime import LayerRuntime
    from qdk.simulation._qodec.logical_qubits import LogicalQubits

    factory = ExecutionPipelineFactory(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        prepare_syndrome_decoder,
        None,
        classical_runtime_factory=AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    )
    pipeline = factory.build_pipeline()
    logical, encoded, physical = pipeline.layers
    assert isinstance(logical, LogicalQubits)
    assert isinstance(encoded, LayerRuntime)
    assert isinstance(physical, InstructionRuntime)
    assert all(not hasattr(stage, "lower") for stage in pipeline.layers)
    assert not hasattr(pipeline.classical_runtime, "quantum_runtime")
    assert all(not hasattr(stage, "quantum_runtime") for stage in pipeline.layers)
    events = []
    handle = encoded.handle

    def probe(request):
        events.append(("request", request.name))
        readouts = yield from handle(request)
        events.append(("readouts", readouts))
        return readouts

    monkeypatch.setattr(encoded, "handle", probe)
    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit[1] qs;
        bit[1] rs = measure qs;
    """)

    assert pipeline.run(bytecode) == [qdk.Result.Zero]
    assert events == [
        ("request", "prepare"),
        ("readouts", ()),
        ("request", "measure"),
        ("readouts", (False,)),
    ]
    assert isinstance(encoded.decoder, SyndromeSession)
    assert encoded.decoder.closed


@pytest.mark.parametrize("failure", ["start", "execute", "close"])
def test_pipeline_closes_every_resource_on_failure(monkeypatch, failure):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.layer_runtime import LayerRuntime

    pipeline = ExecutionPipelineFactory(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        prepare_syndrome_decoder,
        None,
        classical_runtime_factory=AdaptiveRuntime,
        quantum_backend_factory=full_state_backend,
    ).build_pipeline()
    closers = []
    for component in (*pipeline.layers, pipeline.quantum_backend):
        if isinstance(component, Closable):
            close = Mock(wraps=component.close)
            monkeypatch.setattr(component, "close", close)
            closers.append(close)
    failure_point = (
        pipeline.layers[0] if failure == "close" else pipeline.quantum_backend
    )
    injected_failure = Mock(side_effect=RuntimeError("injected failure"))
    monkeypatch.setattr(failure_point, failure, injected_failure)
    if failure == "close":
        closers[0] = injected_failure
    bytecode = compile_qasm("""
        include "stdgates.inc";
        qubit[1] qs;
        bit[1] rs = measure qs;
    """)

    with pytest.raises(RuntimeError, match="injected failure"):
        pipeline.run(bytecode)

    assert all(close.call_count == 1 for close in closers)
    encoded = pipeline.layers[1]
    runtime = pipeline.classical_runtime
    assert isinstance(encoded, LayerRuntime)
    assert isinstance(runtime, AdaptiveRuntime)
    assert isinstance(encoded.decoder, SyndromeSession)
    assert encoded.decoder.closed
    assert encoded.layout.blocks == {}
    assert runtime._pending == []


def test_classical_runtime_cleanup_failure_does_not_skip_other_resources(monkeypatch):
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.decoding import SyndromeSession, prepare_syndrome_decoder
    from qdk.simulation._qodec.executor import ExecutionPipelineFactory
    from qdk.simulation._qodec.layer_runtime import LayerRuntime

    class FailingCleanupRuntime(AdaptiveRuntime):
        def __init__(self):
            super().__init__()
            self.close_count = 0

        def close(self):
            self.close_count += 1
            raise RuntimeError("injected runtime cleanup failure")

    runtime = FailingCleanupRuntime()
    pipeline = ExecutionPipelineFactory(
        qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")),
        prepare_syndrome_decoder,
        None,
        classical_runtime_factory=lambda: runtime,
        quantum_backend_factory=full_state_backend,
    ).build_pipeline()
    closers = []
    for component in (*pipeline.layers, pipeline.quantum_backend):
        if isinstance(component, Closable):
            close = Mock(wraps=component.close)
            monkeypatch.setattr(component, "close", close)
            closers.append(close)

    with pytest.raises(RuntimeError, match="injected runtime cleanup failure"):
        pipeline._close()

    assert runtime.close_count == 1
    assert all(close.call_count == 1 for close in closers)
    encoded = pipeline.layers[1]
    assert isinstance(encoded, LayerRuntime)
    assert isinstance(encoded.decoder, SyndromeSession)
    assert encoded.decoder.closed
    assert pipeline.closed
