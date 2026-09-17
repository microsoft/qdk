import sys
from types import ModuleType
from typing import TYPE_CHECKING, cast

import pytest
from qdk.simulation import NoiseConfig, run_qir

if TYPE_CHECKING:
    from qodec import Qodec


@pytest.mark.parametrize("simulator_type", [None, "clifford", "cpu"])
def test_qodec_selects_encoded_runner(monkeypatch, simulator_type):
    qodec = cast("Qodec", object())
    noise = NoiseConfig()
    expected = [True, False, True]

    def run_encoded(qir, selected_qodec, selected_noise, shots, seed, *, type):
        assert (qir, selected_qodec, selected_noise, shots, seed, type) == (
            "qir",
            qodec,
            noise,
            3,
            42,
            simulator_type,
        )
        return expected

    runner = ModuleType("qdk.simulation._qodec._run")
    monkeypatch.setattr(runner, "run_qir_with_qodec", run_encoded, raising=False)
    monkeypatch.setitem(sys.modules, runner.__name__, runner)

    def unexpected_gpu_probe():
        pytest.fail("Encoded execution must not probe for a GPU")

    monkeypatch.setattr(
        "qdk.simulation._simulation.try_create_gpu_adapter", unexpected_gpu_probe
    )
    assert run_qir("qir", 3, noise, 42, simulator_type, qodec=qodec) is expected


@pytest.mark.parametrize("failure", [None, "start", "execute"])
def test_pipeline_routes_readouts_and_closes_each_component(failure):
    pytest.importorskip("qodec")
    from qdk.simulation._qodec._pipeline import ExecutionPipeline
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
    from qdk.simulation._qodec._pipeline import ExecutionPipeline
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
def test_public_qodec_runner_matches_adaptive_physical_results(simulator_type):
    qodec = pytest.importorskip("qodec")
    import qdk
    import qdk.openqasm
    from ec_tests.runtime import FIXTURES

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
    assert run_qir(qir, shots=3, seed=7, type=simulator_type, qodec=codec) == expected


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
        for name in ("qodec", "paulimer", "numpy", "scipy", "stim"):
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
