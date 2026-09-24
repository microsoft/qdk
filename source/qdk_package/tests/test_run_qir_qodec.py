import sys
from types import ModuleType
from typing import TYPE_CHECKING, cast

import pytest
from qdk.simulation import NoiseConfig, run_qir

if TYPE_CHECKING:
    from qodec import Layer, Qodec


@pytest.mark.parametrize("simulator_type", [None, "clifford", "cpu"])
@pytest.mark.parametrize("custom_decoder", [False, True])
@pytest.mark.parametrize(
    "policy, max_retries", [("raise", 3), ("discard", 0), ("retry", 2)]
)
def test_qodec_selects_encoded_runner(
    monkeypatch, simulator_type, custom_decoder, policy, max_retries
):
    qodec = cast("Qodec", object())
    noise = NoiseConfig()
    expected = [True, False, True]

    def prepare_decoder(layer):
        pytest.fail("Decoder preparation belongs to the encoded runner")

    selected_decoder = prepare_decoder if custom_decoder else None

    def run_encoded(
        qir,
        selected_qodec,
        selected_noise,
        shots,
        seed,
        *,
        type,
        on_shot_failure,
        max_retries: int,
        decoder=None,
    ):
        assert (qir, selected_qodec, selected_noise, shots, seed, type) == (
            "qir",
            qodec,
            noise,
            3,
            42,
            simulator_type,
        )
        assert on_shot_failure == policy
        assert max_retries == expected_retries
        assert decoder is selected_decoder
        return expected

    expected_retries = max_retries
    runner = ModuleType("qdk.simulation._qodec._run")
    monkeypatch.setattr(runner, "run_qir_with_qodec", run_encoded, raising=False)
    monkeypatch.setitem(sys.modules, runner.__name__, runner)

    def unexpected_gpu_probe():
        pytest.fail("Encoded execution must not probe for a GPU")

    monkeypatch.setattr(
        "qdk.simulation._simulation.try_create_gpu_adapter", unexpected_gpu_probe
    )
    assert (
        run_qir(
            "qir",
            3,
            noise,
            42,
            simulator_type,
            qodec=qodec,
            decoder=selected_decoder,
            on_shot_failure=policy,
            max_retries=max_retries,
        )
        is expected
    )


@pytest.mark.parametrize(
    "options", [{"on_shot_failure": "raise"}, {"max_retries": 0}]
)
def test_shot_failure_options_require_a_qodec(options):
    with pytest.raises(ValueError, match="require a Qodec"):
        run_qir("", **options)


def test_decoder_requires_a_qodec():
    def prepare_decoder(layer):
        pytest.fail("Decoder must not be prepared without a Qodec")

    with pytest.raises(ValueError, match="decoder requires a Qodec"):
        run_qir("", decoder=prepare_decoder)


@pytest.mark.parametrize("missing_dependency", ["deq", "deq_runtime"])
def test_deq_missing_dependency_has_install_instructions(
    monkeypatch, missing_dependency
):
    pytest.importorskip("qodec")
    from qdk.simulation.decoders import prepare_deq_decoder

    monkeypatch.delitem(
        sys.modules, "qdk.simulation._qodec.deq_decoding", raising=False
    )
    for name in tuple(sys.modules):
        if name == "deq" or name.startswith("deq."):
            monkeypatch.delitem(sys.modules, name)
    monkeypatch.setitem(sys.modules, missing_dependency, None)
    with pytest.raises(ImportError, match="pip install deq deq-runtime"):
        prepare_deq_decoder(cast("Layer", object()))


@pytest.mark.parametrize("failure", [None, "start", "execute"])
def test_pipeline_routes_readouts_and_closes_each_component(failure):
    pytest.importorskip("qodec")
    from qdk.simulation._qodec.execution_pipeline import ExecutionPipeline
    from qdk.simulation._qodec.protocols import Readouts, Requests, Resources
    from qdk.simulation._qodec.quantum_operations import Operation

    closed = []

    class Runtime:
        def required_resources(self, program: str) -> Resources:
            return Resources(qubits=1)

        def run(self, program: str) -> Requests[Readouts]:
            readouts = yield Operation("measure", (0,))
            assert readouts is not None
            return readouts

        def close(self):
            closed.append("runtime")

    class Layer:
        def required_resources(self, upper: Resources) -> Resources:
            return Resources(qubits=upper.qubits * 3)

        def handle(self, request) -> Requests[Readouts]:
            readouts = yield request
            assert readouts is not None
            return tuple(not bit for bit in readouts)

        def close(self):
            closed.append("layer")

    class Backend:
        def start(self, resources: Resources):
            assert resources == Resources(qubits=3)
            if failure == "start":
                raise RuntimeError("start")

        def execute(self, request: Operation) -> Readouts:
            assert request == Operation("measure", (0,))
            if failure == "execute":
                raise RuntimeError("execute")
            return (False,)

        def close(self):
            closed.append("backend")

    pipeline = ExecutionPipeline(Runtime(), [Layer()], Backend())
    if failure:
        with pytest.raises(RuntimeError, match=failure):
            pipeline.run("program")
    else:
        assert pipeline.run("program") == (True,)
    assert closed == ["runtime", "layer", "backend"]
    with pytest.raises(RuntimeError, match="closed"):
        pipeline.run("program")


@pytest.mark.parametrize("measurement", [False, True])
def test_adaptive_runtime_branches_on_returned_readouts(measurement):
    pytest.importorskip("qodec")
    from qdk import Result
    from qdk.simulation._qodec.adaptive_runtime import AdaptiveRuntime
    from qdk.simulation._qodec.bytecode import compile
    from qdk.simulation._qodec.execution_pipeline import ExecutionPipeline
    from qdk.simulation._simulation import preprocess_simulation_input
    from test_adaptive_cpu_quantum_ops import MEASURE_AND_CORRECT_QIR

    operations = []

    class Backend:
        def execute(self, request):
            operations.append(request.name)
            return (measurement,) if request.name == "measure" else ()

    module, _, _, _ = preprocess_simulation_input(MEASURE_AND_CORRECT_QIR)
    pipeline = ExecutionPipeline(AdaptiveRuntime(), [], Backend())
    assert pipeline.run(compile(module)) == [Result.One if measurement else Result.Zero]
    assert operations == ["prepare", "h", "measure", "prepare"] + (
        ["x"] if measurement else []
    )


@pytest.mark.parametrize("simulator_type", [None, "clifford", "cpu"])
@pytest.mark.parametrize("decoder_name", [None, "syndrome", "frame", "deq"])
def test_public_qodec_runner_matches_adaptive_physical_results(
    simulator_type, decoder_name
):
    qodec = pytest.importorskip("qodec")
    import qdk
    import qdk.openqasm
    from ec_tests.runtime import FIXTURES

    decoder = None
    if decoder_name is not None:
        from qdk.simulation import decoders

        if decoder_name == "deq":
            pytest.importorskip("deq")
            pytest.importorskip("deq_runtime")
        decoder = getattr(decoders, f"prepare_{decoder_name}_decoder")

    qir = qdk.openqasm.compile(
        """
        include "stdgates.inc";
        qubit target;
        x target;
        bit first = measure target;
        if (first) { x target; }
        bit second = measure target;
        """,
        target_profile=qdk.TargetProfile.Adaptive,
    )
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    expected = run_qir(qir, shots=3, seed=7, type="cpu")
    assert (
        run_qir(qir, shots=3, seed=7, type=simulator_type, qodec=codec, decoder=decoder)
        == expected
    )


def test_custom_decoder_changes_public_results_and_closes_each_shot():
    qodec = pytest.importorskip("qodec")
    import qdk
    import qdk.openqasm
    from ec_tests.runtime import FIXTURES
    from qdk.simulation.decoders import Decoded, prepare_syndrome_decoder

    prepared = []
    seeds = []
    closed = []

    def prepare_decoder(layer):
        prepared.append(layer)
        create = prepare_syndrome_decoder(layer)

        class Decoder:
            def __init__(self, seed):
                seeds.append(seed)
                self.inner = create(seed)

            def decode(self, invocation, readouts):
                decoded = yield from self.inner.decode(invocation, readouts)
                return Decoded(
                    tuple(None if bit is None else not bit for bit in decoded.outcomes),
                    decoded.flags,
                )

            def close(self):
                self.inner.close()
                closed.append(self)

        return Decoder

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; bit result = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )
    assert (
        run_qir(qir, shots=3, seed=7, qodec=codec, decoder=prepare_decoder)
        == [qdk.Result.One] * 3
    )
    assert prepared == [codec.layers[0]]
    assert len(set(seeds)) == 3
    assert len({id(session) for session in closed}) == 3


def test_deq_decoder_works_inside_an_asyncio_loop():
    import asyncio

    qodec = pytest.importorskip("qodec")
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    import qdk
    import qdk.openqasm
    from ec_tests.runtime import FIXTURES
    from qdk.simulation.decoders import prepare_deq_decoder

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    qir = qdk.openqasm.compile(
        'include "stdgates.inc"; qubit target; x target; bit result = measure target;',
        target_profile=qdk.TargetProfile.Adaptive,
    )

    async def simulate():
        return run_qir(qir, shots=2, seed=7, qodec=codec, decoder=prepare_deq_decoder)

    assert asyncio.run(simulate()) == [qdk.Result.One] * 2


def test_qir_return_value_does_not_replace_recorded_outputs():
    qodec = pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES

    qir = """
    define i64 @main() #0 {
    entry:
      call void @__quantum__rt__int_record_output(i64 42, i8* null)
      ret i64 1
    }
    declare void @__quantum__rt__int_record_output(i64, i8*)
    attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    """
    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    assert run_qir(qir, shots=2, type="cpu") == [42, 42]
    assert run_qir(qir, shots=2, type="cpu", qodec=codec) == [42, 42]


def test_qodec_gpu_selection_is_explicitly_unsupported():
    qodec = pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    with pytest.raises(NotImplementedError, match="GPU"):
        run_qir("", qodec=codec, type="gpu")


def test_physical_execution_does_not_import_ec_dependencies():
    import subprocess
    from textwrap import dedent

    code = dedent('''
        import sys
        for name in ("qodec", "paulimer", "numpy", "scipy", "stim", "deq", "deq_runtime"):
            sys.modules[name] = None
        from qdk.simulation import run_qir
        qir = """
        define void @main() #0 { ret void }
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        """
        assert run_qir(qir, type="cpu") == [""]
        assert "qdk.simulation._qodec" not in sys.modules
    ''')
    result = subprocess.run(
        [sys.executable, "-c", code], capture_output=True, text=True, timeout=30
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("missing_dependency", ["deq", "deq_runtime"])
def test_deq_dependency_is_only_required_when_selected(missing_dependency):
    import subprocess
    from textwrap import dedent

    pytest.importorskip("qodec")
    from ec_tests.runtime import FIXTURES

    code = dedent('''
        import sys
        sys.modules[sys.argv[1]] = None
        from qodec import Qodec
        from qdk.simulation import run_qir
        from qdk.simulation.decoders import prepare_deq_decoder, prepare_syndrome_decoder
        codec = Qodec.load(sys.argv[2])
        qir = """
        define void @main() #0 { ret void }
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
        """
        assert run_qir(qir, qodec=codec, decoder=prepare_syndrome_decoder) == [""]
        assert "qdk.simulation._qodec.deq_decoding" not in sys.modules
        try:
            run_qir(qir, qodec=codec, decoder=prepare_deq_decoder)
        except ImportError as error:
            assert 'pip install deq deq-runtime' in str(error), str(error)
        else:
            raise AssertionError("Missing deq dependency was not reported")
    ''')
    result = subprocess.run(
        [
            sys.executable,
            "-c",
            code,
            missing_dependency,
            str(FIXTURES / "repetition3.qodec.yaml"),
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("policy", ["raise", "discard", "retry"])
@pytest.mark.parametrize(
    "failure_type", ["ExecutionRejected", "ExecutionUnresolved", "InconsistentParity"]
)
def test_shot_failure_policy_preserves_order_and_attempt_count(policy, failure_type):
    pytest.importorskip("qodec")
    from qdk.simulation._qodec import _run
    from qdk.simulation._simulation import preprocess_simulation_input

    failure = getattr(_run, failure_type)("failed shot")
    outcomes = iter(["first", failure, "second", "third"])
    attempts = []

    class Executor:
        def set_seed(self, _seed):
            pass

        def run(self, program):
            attempts.append(program)
            outcome = next(outcomes)
            if isinstance(outcome, Exception):
                raise outcome
            return outcome

    module, _, _, _ = preprocess_simulation_input("""
        define void @main() #0 { ret void }
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    """)
    if policy == "raise":
        with pytest.raises(type(failure)) as raised:
            _run.run_qir_raw_records(module, Executor(), 3, on_shot_failure=policy)
        assert raised.value is failure
        assert len(attempts) == 2
    else:
        records = _run.run_qir_raw_records(
            module, Executor(), 3, on_shot_failure=policy
        )
        assert records == (
            ["first", "second"] if policy == "discard" else ["first", "second", "third"]
        )
        assert len(attempts) == (3 if policy == "discard" else 4)


@pytest.mark.parametrize("policy", ["raise", "discard", "retry"])
@pytest.mark.parametrize(
    "error_type", [ValueError, TypeError, RuntimeError, NotImplementedError]
)
def test_shot_failure_policy_does_not_catch_execution_errors(policy, error_type):
    pytest.importorskip("qodec")
    from qdk.simulation._qodec._run import run_qir_raw_records
    from qdk.simulation._simulation import preprocess_simulation_input

    failure = error_type("backend failed")

    class Executor:
        def run(self, program):
            raise failure

    module, _, _, _ = preprocess_simulation_input("""
        define void @main() #0 { ret void }
        attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
    """)
    with pytest.raises(error_type) as raised:
        run_qir_raw_records(module, Executor(), 1, on_shot_failure=policy)
    assert raised.value is failure
