import pytest
import qodec
import qdk.openqasm
from qdk import Result
from qdk.simulation import NoiseConfig

from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
from qdk.simulation._qodec.decoding import prepare_syndrome_decoder
from qdk.simulation._qodec.executor import ExecutionPipelineFactory
from qdk.simulation._qodec.quantum_backend import stabilizer_backend
from . import FIXTURES
from .test_execution_pipeline import compile_qasm
from ec_tests.testing.optional import requires_stim


def make_factory(noise=None, decoder=prepare_syndrome_decoder, codec=None):
    if codec is None:
        codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    return ExecutionPipelineFactory(
        codec, decoder, noise, AdaptiveRuntime, stabilizer_backend
    )


@requires_stim
def test_native_batch_prepares_static_noisy_qodec_shots():
    from qdk.simulation._qodec.native_batch import prepare_batch

    program = compile_qasm("""
        include "stdgates.inc";
        qubit[5] data;
        for int target in [0:4] { x data[target]; }
        bit[5] readout = measure data;
    """)
    noise = NoiseConfig()
    noise.x.x = 0.01
    batch = prepare_batch(program, make_factory(noise))
    assert batch is not None
    first = batch.run(1000, noise, seed=7)
    assert first == batch.run(1000, noise, seed=7)
    assert len(first) == 1000
    assert all(len(shot) == 5 for shot in first)
    assert sum(any(bit == Result.Zero for bit in shot) for shot in first) < 20


@pytest.mark.parametrize("qubits", [1, 4])
def test_native_batch_uses_deq_and_its_per_shot_seeds(monkeypatch, qubits):
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec.decoding import CodeDecoder, prepare_deq_decoder
    from qdk.simulation._qodec.deq_decoding import DeqSession
    from qdk.simulation._qodec.native_batch import prepare_batch

    def forbidden_syndrome_solver(*args):
        pytest.fail("Selecting deq must not use the syndrome solver")

    seeds = []
    original_start = DeqSession._start

    async def start(session, seed):
        seeds.append(seed)
        await original_start(session, seed)

    monkeypatch.setattr(CodeDecoder, "correct", forbidden_syndrome_solver)
    monkeypatch.setattr(DeqSession, "_start", start)
    gates = " ".join(f"x data[{index}];" for index in range(qubits))
    program = compile_qasm(f"""
        include "stdgates.inc"; qubit[{qubits}] data;
        {gates}
        bit[{qubits}] readout = measure data;
    """)
    noise = NoiseConfig()
    noise.x.x = 0.2
    batch = prepare_batch(program, make_factory(noise, decoder=prepare_deq_decoder))
    assert batch is not None
    assert seeds == []
    first = batch.run(32, noise, seed=17)
    assert first == batch.run(32, noise, seed=17)
    assert len(first) == 32
    assert all(len(shot) == qubits for shot in first)
    assert len(set(seeds)) > 1


def test_deq_batch_reuses_transport_for_independent_seeded_records(monkeypatch):
    from contextlib import closing

    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec import deq_decoding
    from qdk.simulation._qodec.decoding import prepare_deq_decoder
    from qdk.simulation._qodec.protocols import BatchDecoderFactory
    from .test_execution_pipeline import invocation_for

    original_worker = deq_decoding.ThreadPoolExecutor
    workers = []

    def worker(*args, **kwargs):
        result = original_worker(*args, **kwargs)
        workers.append(result)
        return result

    monkeypatch.setattr(deq_decoding, "ThreadPoolExecutor", worker)
    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    factory = prepare_deq_decoder(layer)
    assert isinstance(factory, BatchDecoderFactory)
    with closing(factory.prepare_batch()) as session:
        prepared = session.prepare_readouts(
            invocation_for(layer.gadgets["__quantum__qis__m__body"]), 3
        )
    assert prepared is not None
    assert workers == []
    rows = [
        tuple(bool(pattern & (1 << index)) for index in range(3))
        for pattern in range(8)
    ]
    assert prepared.decode_batch(rows, list(range(8))) == [
        (sum(row) >= 2,) for row in rows
    ]
    assert len(workers) == 1
    assert all(not thread.is_alive() for thread in workers[0]._threads)


@requires_stim
def test_native_batch_declines_measurement_dependent_control():
    from qdk.simulation._qodec.native_batch import prepare_batch

    program = compile_qasm("""
        include "stdgates.inc";
        qubit data;
        bit first = measure data;
        if (first) { x data; }
        bit second = measure data;
    """)
    assert prepare_batch(program, make_factory()) is None


@requires_stim
def test_public_runner_uses_native_batch_for_static_qodec(monkeypatch):
    from qdk.simulation._qodec._run import run_qir_with_qodec
    from qdk.simulation._qodec.executor import Executor

    def unexpected_shot(*args):
        pytest.fail("Eligible shots must not enter the per-shot Python interpreter")

    monkeypatch.setattr(Executor, "run", unexpected_shot)
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit[2] data; x data[1]; bit[2] readout = measure data;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    assert (
        run_qir_with_qodec(qir, codec, None, shots=10, seed=7)
        == [[Result.Zero, Result.One]] * 10
    )


@requires_stim
@pytest.mark.parametrize(
    "control,target", [(False, False), (False, True), (True, False), (True, True)]
)
@pytest.mark.parametrize(
    "fault",
    [
        "II",
        "IX",
        "IY",
        "IZ",
        "XI",
        "XX",
        "XY",
        "XZ",
        "YI",
        "YX",
        "YY",
        "YZ",
        "ZI",
        "ZX",
        "ZY",
        "ZZ",
    ],
)
def test_native_batch_matches_interpreter_for_logical_cnot_and_correlated_noise(
    control, target, fault
):
    from qodec.actions import Clifford
    from qodec.gadgets import Circuit, Encoding
    from qodec.instructions import BlockOperand
    from qdk.simulation._qodec.native_batch import prepare_batch

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    layer, physical = codec.layers
    operand = BlockOperand("repetition3")
    declaration = qodec.Instruction(
        "cnot",
        inputs=[operand, operand],
        outputs=[operand, operand],
        action=[Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
    )
    declarations = layer.instruction_set.instructions
    declarations["cnot"] = declaration
    layer.instruction_set.instructions = declarations
    code = layer.codes["repetition3"]
    encodings = [
        Encoding(code, support=[str(index) for index in range(start, start + 3)])
        for start in (0, 3)
    ]
    gadgets = layer.gadgets
    gadgets["cnot"] = qodec.Gadget(
        declaration,
        Circuit(physical.instruction_set, "CX 0 3 1 4 2 5", format="stim"),
        inputs=encodings,
        outputs=encodings,
    )
    layer.gadgets = gadgets
    noise = NoiseConfig()
    if fault != "II":
        noise.cx.set_pauli_noise(fault, 1.0)
    gates = ("x data[0];" if control else "") + ("x data[1];" if target else "")
    program = compile_qasm(f"""
        include "stdgates.inc"; qubit[2] data;
        {gates} cx data[0], data[1];
        bit[2] readout = measure data;
    """)
    factory = make_factory(noise, codec=codec)
    batch = prepare_batch(program, factory)
    assert batch is not None
    factory.set_seed(7)
    expected = [factory.build_pipeline().run(program) for _ in range(2)]
    assert batch.run(2, noise, seed=7) == expected


@requires_stim
def test_public_runner_batches_qsharp_programs_that_reset_after_measuring(
    monkeypatch,
):
    from qdk import TargetProfile, qsharp
    from qdk.simulation._qodec._run import run_qir_with_qodec
    from qdk.simulation._qodec.executor import Executor

    def unexpected_shot(*args):
        pytest.fail("Eligible shots must not enter the per-shot Python interpreter")

    monkeypatch.setattr(Executor, "run", unexpected_shot)
    qsharp.init(target_profile=TargetProfile.Adaptive)
    qir = qsharp.compile("{ use q = Qubit(); X(q); MResetZ(q) }")
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    assert run_qir_with_qodec(qir, codec, None, shots=5, seed=7) == [Result.One] * 5


@requires_stim
@pytest.mark.parametrize(
    "gates",
    [
        "h data; h data;",
        "t data;",
        "bit first = measure data;",
    ],
)
def test_native_batch_declines_unsupported_logical_programs(gates):
    from qdk.simulation._qodec.native_batch import prepare_batch

    program = compile_qasm(f"""
        include "stdgates.inc"; qubit data;
        {gates} bit last = measure data;
    """)
    if gates.startswith("h"):
        with pytest.raises(NotImplementedError):
            prepare_batch(program, make_factory())
    else:
        assert prepare_batch(program, make_factory()) is None


@requires_stim
def test_native_batch_accepts_measured_qubits_after_a_reset():
    from qdk.simulation._qodec.native_batch import prepare_batch

    program = compile_qasm("""
        include "stdgates.inc"; qubit data;
        x data; bit first = measure data; reset data;
        x data; bit last = measure data;
    """)
    factory = make_factory()
    batch = prepare_batch(program, factory)
    assert batch is not None
    assert batch.run(3, None, seed=7) == [[Result.One, Result.One]] * 3
    assert factory.build_pipeline().run(program) == [Result.One, Result.One]


@requires_stim
@pytest.mark.parametrize("noise_kind", ["reset_loss", "loss"])
def test_native_batch_declines_unsupported_noise(noise_kind):
    from qdk.simulation._qodec.native_batch import prepare_batch

    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;'
    )
    noise = NoiseConfig()
    if noise_kind == "reset_loss":
        noise.mresetz.loss = 0.01
    else:
        noise.x.set_pauli_noise("L", 0.01)
    assert prepare_batch(program, make_factory(noise)) is None


def _reset_noise(fault, probability=1.0):
    noise = NoiseConfig()
    setattr(noise.mresetz, fault, probability)
    return noise


def _repetition_code_with_x_circuit(source, *, non_destructive_measurement=False):
    from qodec.actions import Observe
    from qodec.gadgets import Circuit
    from qodec.instructions import BlockOperand

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    physical = codec.layers[-1].instruction_set
    if non_destructive_measurement:
        operand = BlockOperand("qubit")
        instructions = physical.instructions
        instructions["M"] = qodec.Instruction(
            "M", inputs=[operand], outputs=[operand], action=[Observe(["Z_0"])]
        )
        physical.instructions = instructions
    gadget = codec.layers[0].gadgets["__quantum__qis__x__body"]
    gadget.circuit = Circuit(physical, source, format="stim")
    return codec


def _assert_batch_matches_interpreter(program, factory, noise, batch_type=None):
    from qdk.simulation._qodec.native_batch import prepare_batch

    batch = prepare_batch(program, factory)
    assert batch is not None
    if batch_type is not None:
        assert type(batch).__name__ == batch_type
    factory.set_seed(7)
    expected = [factory.build_pipeline().run(program) for _ in range(3)]
    assert batch.run(3, noise, seed=7) == expected


@requires_stim
@pytest.mark.parametrize("fault", ["x", "y", "z"])
@pytest.mark.parametrize(
    "gates",
    [
        "x data; bit readout = measure data;",
        "x data; bit first = measure data; reset data; x data; bit last = measure data;",
    ],
)
@pytest.mark.parametrize("batch_type", ["NativeBatch", "ReplayBatch"])
def test_native_batch_matches_interpreter_under_reset_noise(fault, gates, batch_type):
    program = compile_qasm(f'include "stdgates.inc"; qubit data; {gates}')
    codec = (
        _syndrome_measuring_repetition_code() if batch_type == "ReplayBatch" else None
    )
    noise = _reset_noise(fault)
    _assert_batch_matches_interpreter(
        program, make_factory(noise, codec=codec), noise, batch_type
    )


@requires_stim
@pytest.mark.parametrize("fault", ["x", "y", "z"])
def test_native_batch_resets_discarded_qubits_without_reset_noise(fault):
    # The X gadget reuses its discarded ancilla without preparing it, so a
    # noisy reset on discard would flip two data qubits.
    codec = _repetition_code_with_x_circuit("X 0 1 2\nCX 3 0 3 1\nM 3")
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; x data; bit readout = measure data;'
    )
    noise = _reset_noise(fault)
    _assert_batch_matches_interpreter(program, make_factory(noise, codec=codec), noise)


@requires_stim
def test_native_batch_bounds_fresh_qubits_for_reused_discarded_qubits(monkeypatch):
    from qdk.simulation._qodec import native_batch

    monkeypatch.setattr(native_batch, "_MAX_FRESH_QUBITS", 0)
    codec = _repetition_code_with_x_circuit("X 0 1 2\nCX 3 0 3 1\nM 3")
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; x data; bit readout = measure data;'
    )
    noise = _reset_noise("x", 0.01)
    assert native_batch.prepare_batch(program, make_factory(noise, codec=codec)) is None


@requires_stim
@pytest.mark.parametrize("table", ["mresetz", "mz"])
def test_native_batch_samples_interpreter_noise_after_measurements(table):
    # The interpreter samples mresetz after a measurement and never reads mz.
    codec = _repetition_code_with_x_circuit(
        "X 0 1 2\nM 3\nCX 3 0 3 1", non_destructive_measurement=True
    )
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;'
    )
    noise = NoiseConfig()
    getattr(noise, table).x = 1.0
    _assert_batch_matches_interpreter(program, make_factory(noise, codec=codec), noise)


def test_native_noise_maps_measurement_noise_without_mutating_the_config():
    from qdk.simulation._qodec.native_batch import _native_noise

    noise = NoiseConfig()
    noise.mresetz.x = 0.1
    noise.mz.z = 0.2
    noise.cx.set_pauli_noise("XZ", 0.3)
    native = _native_noise(noise)
    assert native is not None and native is not noise
    assert (native.mz.x, native.mz.z) == (0.1, 0.0)
    assert (native.mresetz.x, native.cx.xz) == (0.1, 0.3)
    assert (noise.mz.x, noise.mz.z) == (0.0, 0.2)
    assert _native_noise(None) is None


@requires_stim
def test_native_batch_observes_expected_reset_noise_distribution():
    import math

    from qdk.simulation._qodec.native_batch import prepare_batch

    probability = 0.2
    shots = 10_000
    noise = _reset_noise("x", probability)
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit readout = measure data;'
    )
    batch = prepare_batch(program, make_factory(noise))
    assert batch is not None
    results = batch.run(shots, noise, seed=17)
    failures = sum(shot == [Result.One] for shot in results)
    # Each code qubit's preparation flips with the given probability.
    expected_rate = 3 * probability**2 - 2 * probability**3
    deviation = math.sqrt(shots * expected_rate * (1 - expected_rate))
    assert abs(failures - shots * expected_rate) < 5 * deviation


@requires_stim
def test_public_runner_batches_reset_noise(monkeypatch):
    from qdk.simulation._qodec._run import run_qir_with_qodec
    from qdk.simulation._qodec.executor import Executor

    def unexpected_shot(*args):
        pytest.fail("Eligible shots must not enter the per-shot Python interpreter")

    monkeypatch.setattr(Executor, "run", unexpected_shot)
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    results = run_qir_with_qodec(qir, codec, _reset_noise("x", 0.01), shots=10, seed=7)
    assert len(results) == 10


@requires_stim
def test_native_batch_preserves_custom_decoder_and_seed_state():
    from qdk.simulation._qodec.native_batch import prepare_batch

    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit readout = measure data;'
    )
    factory = make_factory()
    factory.set_seed(7)
    before = factory.rng.getstate()
    assert prepare_batch(program, factory) is not None
    assert factory.rng.getstate() == before
    custom = make_factory(decoder=lambda layer: prepare_syndrome_decoder(layer))
    assert prepare_batch(program, custom) is not None


@requires_stim
def test_native_batch_preserves_custom_decoder_outputs_hooks_and_seeds():
    from qdk.simulation._qodec.native_batch import prepare_batch
    from qdk.simulation._qodec.protocols import Decoded

    prepared_hooks = []
    scalar_seeds = []
    batch_seeds = []

    class InvertedReadouts:
        def __init__(self, inner):
            self.inner = inner

        def decode_batch(self, records, seeds):
            batch_seeds.extend(seeds)
            return [
                tuple(not bit for bit in row)
                for row in self.inner.decode_batch(records, seeds)
            ]

    class Session:
        def __init__(self, inner, preparing=False):
            self.inner = inner
            self.preparing = preparing

        def before(self, invocation):
            if self.preparing:
                prepared_hooks.append(invocation.call.mnemonic)
            yield from ()

        def decode(self, invocation, records):
            decoded = yield from self.inner.decode(invocation, records)
            return Decoded(tuple(not bit for bit in decoded.outcomes), decoded.flags)

        def prepare_readouts(self, invocation, count):
            prepared = self.inner.prepare_readouts(invocation, count)
            return None if prepared is None else InvertedReadouts(prepared)

        def discarded(self, blocks):
            self.inner.discarded(blocks)

        def close(self):
            self.inner.close()

    class Factory:
        def __init__(self, layer):
            self.inner = prepare_syndrome_decoder(layer)

        def __call__(self, seed):
            scalar_seeds.append(seed)
            return Session(self.inner(seed))

        def prepare_batch(self):
            return Session(self.inner.prepare_batch(), preparing=True)

    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;'
    )
    factory = make_factory(decoder=Factory)
    factory.set_seed(19)
    expected = [factory.build_pipeline().run(program) for _ in range(8)]
    batch = prepare_batch(program, factory)
    assert batch is not None
    assert prepared_hooks == [
        "prepare_z",
        "__quantum__qis__x__body",
        "__quantum__qis__m__body",
    ]
    assert batch.run(8, None, seed=19) == expected == [[Result.Zero]] * 8
    assert batch_seeds == scalar_seeds


@requires_stim
def test_native_batch_frame_decoder_preserves_readouts_and_rejections():
    from contextlib import closing

    from qdk.simulation._qodec.frame_runtime import prepare_frame_decoder
    from qdk.simulation._qodec.native_batch import prepare_batch
    from qdk.simulation._qodec.readout_equations import InconsistentParity
    from .test_execution_pipeline import decode_gadget

    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit readout = measure data;'
    )
    factory = make_factory(decoder=prepare_frame_decoder)
    batch = prepare_batch(program, factory)
    assert batch is not None
    rows = [
        tuple(bool(pattern & (1 << index)) for index in range(3))
        for pattern in range(8)
    ]
    expected = []
    for row in rows:
        with closing(prepare_frame_decoder(layer)(7)) as session:
            decode_gadget(session, layer.gadgets["prepare_z"], ())
            try:
                expected.append(
                    decode_gadget(
                        session, layer.gadgets["__quantum__qis__m__body"], row
                    ).readouts
                )
            except InconsistentParity as error:
                expected.append(error)
    actual = batch.decoders[0].decoder.decode_batch(rows, [7] * 8)
    assert [(type(value), str(value)) for value in actual] == [
        (type(value), str(value)) for value in expected
    ]
    assert batch.run(5, None, seed=7) == [[Result.Zero]] * 5


@requires_stim
@pytest.mark.parametrize("policy", ["raise", "discard"])
@pytest.mark.parametrize(
    "failure_name",
    ["ExecutionRejected", "ExecutionUnresolved", "InconsistentParity", "RuntimeError"],
)
def test_batch_decoder_failure_policy_preserves_order(policy, failure_name):
    from dataclasses import replace
    from types import SimpleNamespace

    from qdk.simulation._qodec import protocols, readout_equations
    from qdk.simulation._qodec.native_batch import prepare_batch

    failure_type = {
        "ExecutionRejected": protocols.ExecutionRejected,
        "ExecutionUnresolved": protocols.ExecutionUnresolved,
        "InconsistentParity": readout_equations.InconsistentParity,
        "RuntimeError": RuntimeError,
    }[failure_name]
    failure = failure_type("failed second shot")
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit readout = measure data;'
    )
    batch = prepare_batch(program, make_factory())
    assert batch is not None
    scripted = SimpleNamespace(
        decode_batch=lambda records, seeds: [(True,), failure, (False,)]
    )
    batch = replace(batch, decoders=(replace(batch.decoders[0], decoder=scripted),))
    if policy == "discard" and failure_name != "RuntimeError":
        assert batch.run(3, None, seed=7, on_shot_failure=policy) == [
            [Result.One],
            [Result.Zero],
        ]
    else:
        with pytest.raises(failure_type) as raised:
            batch.run(3, None, seed=7, on_shot_failure=policy)
        assert raised.value is failure


@requires_stim
@pytest.mark.parametrize("rows", [[(False,)], [(), (), ()], [(None,)] * 3])
def test_native_batch_validates_decoder_output_shape(rows):
    from dataclasses import replace
    from types import SimpleNamespace

    from qdk.simulation._qodec.native_batch import prepare_batch

    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit readout = measure data;'
    )
    batch = prepare_batch(program, make_factory())
    assert batch is not None
    scripted = SimpleNamespace(decode_batch=lambda records, seeds: rows)
    batch = replace(batch, decoders=(replace(batch.decoders[0], decoder=scripted),))
    with pytest.raises(ValueError, match="Batch decoder"):
        batch.run(3, None, seed=7)


@requires_stim
def test_native_batch_does_not_probe_non_batch_decoder_sessions():
    from qdk.simulation._qodec.native_batch import ReplayBatch, prepare_batch

    def decoder(layer):
        def create(seed):
            pytest.fail("A scalar decoder cannot be probed during batch preparation")

        return create

    program = compile_qasm(
        'include "stdgates.inc"; qubit data; bit readout = measure data;'
    )
    # Scalar decoders replay one session per shot; preparing them opens none.
    assert isinstance(
        prepare_batch(program, make_factory(decoder=decoder)), ReplayBatch
    )


@pytest.mark.parametrize(
    "decoder_name",
    ["syndrome", pytest.param("frame", marks=requires_stim), "deq"],
)
def test_prepared_decoder_rejects_unknown_or_mismatched_inputs(decoder_name):
    from contextlib import closing

    from qdk.simulation import decoders
    from .test_execution_pipeline import decode_gadget, invocation_for

    if decoder_name == "deq":
        pytest.importorskip("deq")
        pytest.importorskip("deq_runtime")
    layer = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml")).layers[0]
    factory = getattr(decoders, f"prepare_{decoder_name}_decoder")(layer)
    with closing(factory.prepare_batch()) as session:
        decode_gadget(session, layer.gadgets["prepare_z"], ())
        prepared = session.prepare_readouts(
            invocation_for(layer.gadgets["__quantum__qis__m__body"]), 3
        )
    assert prepared is not None
    result = prepared.decode_batch([(None, False, False)], [7])
    assert len(result) == 1
    assert isinstance(result[0], decoders.ExecutionUnresolved)
    with pytest.raises(ValueError, match="seed"):
        prepared.decode_batch([(False,) * 3], [])
    with pytest.raises(ValueError, match="width|Width"):
        prepared.decode_batch([(False,)], [7])


@requires_stim
def test_retry_policy_keeps_the_interpreted_attempt_sequence(monkeypatch):
    from qdk.simulation._qodec import native_batch
    from qdk.simulation._qodec._run import run_qir_with_qodec

    def forbidden_batch(*args):
        pytest.fail("Retries must not pre-sample later shot seeds")

    monkeypatch.setattr(native_batch, "prepare_batch", forbidden_batch)
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    assert (
        run_qir_with_qodec(qir, codec, None, shots=3, seed=7, on_shot_failure="retry")
        == [Result.One] * 3
    )


@pytest.mark.parametrize(
    "fixture,measurement,width",
    [
        ("repetition3.qodec.yaml", "__quantum__qis__m__body", 3),
        ("steane/qodec.yaml", "measure_z", 7),
    ],
)
def test_deq_batch_matches_individual_seeded_decoder_sessions(
    fixture, measurement, width
):
    from contextlib import closing

    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec.decoding import prepare_deq_decoder
    from .test_execution_pipeline import decode_gadget, invocation_for

    layer = qodec.Qodec.load(str(FIXTURES / fixture)).layers[0]
    gadget = layer.gadgets[measurement]
    factory = prepare_deq_decoder(layer, error_probability=0.02)
    with closing(factory.prepare_batch()) as session:
        prepared = session.prepare_readouts(invocation_for(gadget), width)
    assert prepared is not None
    rows = [
        tuple(bool(pattern & (1 << index)) for index in range(width))
        for pattern in range(1 << width)
    ]
    seeds = [7 + index * 13 for index in range(len(rows))]
    expected = []
    for records, seed in zip(rows, seeds):
        with closing(factory(seed)) as session:
            expected.append(decode_gadget(session, gadget, records).readouts)
    assert prepared.decode_batch(rows, seeds) == expected
    assert prepared.decode_batch(rows, seeds) == expected


@requires_stim
def test_prepared_batch_is_isolated_from_later_qodec_mutation():
    from qdk.simulation._qodec.native_batch import prepare_batch

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;'
    )
    factory = make_factory(codec=codec)
    codec.layers[0].gadgets["__quantum__qis__m__body"].readouts = []
    batch = prepare_batch(program, factory)
    assert batch is not None
    assert batch.run(2, None, seed=7) == [[Result.One]] * 2
    assert factory.build_pipeline().run(program) == [Result.One]


def _syndrome_measuring_repetition_code():
    from qodec.gadgets import Circuit

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    gadget = codec.layers[0].gadgets["__quantum__qis__x__body"]
    gadget.circuit = Circuit(
        codec.layers[-1].instruction_set,
        "X 0 1 2\nR 3\nCX 0 3\nM 3",
        format="stim",
    )
    return codec


@requires_stim
def test_native_batch_replays_intermediate_syndrome_measurements(monkeypatch):
    from qdk.simulation._qodec._run import run_qir_with_qodec
    from qdk.simulation._qodec.executor import Executor
    from qdk.simulation._qodec.native_batch import ReplayBatch, prepare_batch

    codec = _syndrome_measuring_repetition_code()
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;'
    )
    factory = make_factory(codec=codec)
    assert isinstance(prepare_batch(program, factory), ReplayBatch)
    assert factory.build_pipeline().run(program) == [Result.One]

    def unexpected_shot(*args):
        pytest.fail("Eligible shots must not enter the per-shot Python interpreter")

    monkeypatch.setattr(Executor, "run", unexpected_shot)
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    assert run_qir_with_qodec(qir, codec, None, shots=5, seed=7) == [Result.One] * 5


@requires_stim
def test_replay_falls_back_to_the_interpreter_for_non_pauli_corrections(
    monkeypatch,
):
    from qdk.simulation._qodec._run import run_qir_with_qodec
    from qdk.simulation._qodec.native_batch import ReplayBatch
    from qdk.simulation._qodec.protocols import BatchUnsupported

    def unsupported(*args, **kwargs):
        raise BatchUnsupported

    monkeypatch.setattr(ReplayBatch, "run", unsupported)
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    codec = _syndrome_measuring_repetition_code()
    assert run_qir_with_qodec(qir, codec, None, shots=3, seed=7) == [Result.One] * 3


def test_frame_masks_follow_paulis_through_the_traced_circuit():
    from qdk._native import QirInstructionId as Id
    from qdk.simulation._qodec.native_batch import _FrameMasks

    tape = [
        (Id.RESET, 0),
        (Id.RESET, 1),
        (Id.H, 0),
        (Id.CX, 0, 1),
        (Id.MZ, 0, 0),
        (Id.MZ, 1, 1),
        (Id.RESET, 1),
        (Id.MZ, 1, 2),
    ]
    masks = _FrameMasks(tape)
    # X before the CX spreads to the target; the reset clears it before record 2.
    assert masks.mask(3, 0, "x") == 0b011
    assert masks.mask(4, 0, "x") == 0b001
    # Z before the H becomes X; Z after it never flips a Z measurement.
    assert masks.mask(2, 0, "z") == 0b011
    assert masks.mask(3, 0, "z") == 0
    assert masks.mask(3, 1, "y") == 0b010
    assert masks.mask(6, 1, "x") == 0


@requires_stim
def test_native_batch_observes_expected_repetition_noise_distribution():
    import math

    from qdk.simulation._qodec.native_batch import prepare_batch

    probability = 0.2
    shots = 10_000
    noise = NoiseConfig()
    noise.x.x = probability
    program = compile_qasm(
        'include "stdgates.inc"; qubit data; x data; bit readout = measure data;'
    )
    batch = prepare_batch(program, make_factory(noise))
    assert batch is not None
    results = batch.run(shots, noise, seed=17)
    failures = sum(shot == [Result.Zero] for shot in results)
    expected_rate = 3 * probability**2 - 2 * probability**3
    deviation = math.sqrt(shots * expected_rate * (1 - expected_rate))
    assert abs(failures - shots * expected_rate) < 5 * deviation


@requires_stim
def test_native_batch_preserves_mixed_reordered_and_repeated_output_records():
    from qdk.simulation._simulation import preprocess_simulation_input
    from qdk.simulation._qodec.bytecode import compile
    from qdk.simulation._qodec.native_batch import prepare_batch

    module, _, _, _ = preprocess_simulation_input("""
        %Qubit = type opaque
        %Result = type opaque
        define void @main() #0 {
            call void @__quantum__qis__x__body(%Qubit* null)
            call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
            call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
            call void @__quantum__rt__int_record_output(i64 42, i8* null)
            call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
            call void @__quantum__rt__bool_record_output(i1 true, i8* null)
            call void @__quantum__rt__result_record_output(%Result* null, i8* null)
            call void @__quantum__rt__result_record_output(%Result* null, i8* null)
            call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* null)
            ret void
        }
        declare void @__quantum__qis__x__body(%Qubit*)
        declare void @__quantum__qis__mz__body(%Qubit*, %Result*)
        declare void @__quantum__rt__int_record_output(i64, i8*)
        declare void @__quantum__rt__bool_record_output(i1, i8*)
        declare void @__quantum__rt__result_record_output(%Result*, i8*)
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="3" }
    """)
    program = compile(module)
    factory = make_factory()
    batch = prepare_batch(program, factory)
    assert batch is not None
    expected = [42, Result.Zero, True, Result.One, Result.One, Result.Zero]
    assert factory.build_pipeline().run(program) == expected
    assert batch.run(2, None, seed=7) == [expected] * 2


@requires_stim
def test_native_batch_preserves_entangled_measurements():
    from qodec.actions import Clifford, Observe, Stabilize
    from qodec.gadgets import Circuit, Encoding
    from qodec.instructions import Block, BlockOperand
    from qdk.simulation._qodec.native_batch import prepare_batch

    operand = BlockOperand("wire")
    instructions = [
        qodec.Instruction("R", outputs=[operand], action=[Stabilize(["Z_0"])]),
        qodec.Instruction(
            "H",
            inputs=[operand],
            outputs=[operand],
            action=[Clifford({"X_0": "Z_0", "Z_0": "X_0"})],
        ),
        qodec.Instruction(
            "CX",
            inputs=[operand] * 2,
            outputs=[operand] * 2,
            action=[Clifford({"X_0": "X_0 X_1", "Z_1": "Z_0 Z_1"})],
        ),
        qodec.Instruction("M", inputs=[operand], action=[Observe(["Z_0"])]),
    ]
    physical = qodec.InstructionSet(
        "physical", blocks=[Block("wire", 1)], instructions=instructions
    )
    logical = qodec.InstructionSet(
        "logical", blocks=[Block("wire", 1)], instructions=instructions
    )
    code = qodec.Code("wire", [], ["X_0"], ["Z_0"])
    gadgets = []
    for instruction in instructions:
        width = max(len(instruction.inputs), len(instruction.outputs))
        gadgets.append(
            qodec.Gadget(
                instruction,
                Circuit(
                    physical,
                    instruction.mnemonic
                    + " "
                    + " ".join(str(index) for index in range(width)),
                    format="stim",
                ),
                inputs=[
                    Encoding(code, support=[str(index)])
                    for index in range(len(instruction.inputs))
                ],
                outputs=[
                    Encoding(code, support=[str(index)])
                    for index in range(len(instruction.outputs))
                ],
                readouts=[["circuit.readouts[0]"]] if instruction.observe_count else [],
            )
        )
    codec = qodec.Qodec(
        [
            qodec.Layer(logical, codes={"wire": code}, gadgets=gadgets),
            qodec.Layer(physical),
        ]
    )
    program = compile_qasm("""
        include "stdgates.inc"; qubit[2] data;
        h data[0]; cx data[0], data[1];
        bit[2] readout = measure data;
    """)
    batch = prepare_batch(program, make_factory(codec=codec))
    assert batch is not None
    records = batch.run(100, None, seed=7)
    assert all(first == second for first, second in records)
    assert {tuple(shot) for shot in records} == {
        (Result.Zero, Result.Zero),
        (Result.One, Result.One),
    }


@requires_stim
def test_warm_thousand_shot_demo_stays_under_100_ms():
    import statistics
    import time

    from qdk.simulation._qodec._run import run_qir_with_qodec

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit[5] data;
        for int target in [0:4] { x data[target]; }
        bit[5] readout = measure data;
    """,
        target_profile=qdk.TargetProfile.Adaptive,
    )
    noise = NoiseConfig()
    noise.x.x = 0.01
    run_qir_with_qodec(qir, codec, noise, shots=1, seed=7, on_shot_failure="discard")
    timings = []
    for _ in range(3):
        start = time.perf_counter()
        results = run_qir_with_qodec(
            qir, codec, noise, shots=1000, seed=7, on_shot_failure="discard"
        )
        timings.append(time.perf_counter() - start)
        assert len(results) == 1000
        assert all(len(shot) == 5 for shot in results)
    assert statistics.median(timings) < 0.1, timings
