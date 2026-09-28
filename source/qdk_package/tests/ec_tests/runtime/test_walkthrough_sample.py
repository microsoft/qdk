"""Keep the walkthrough's confidence selection independent of its protocol."""

from functools import partial
from importlib import import_module
import json
from pathlib import Path

import pytest

from qdk import Result, TargetProfile, ec, qsharp
from qdk.simulation import NoiseConfig, run_qir
from qdk.simulation.decoders import prepare_deq_decoder

# The walkthrough builds its gadgets from Stim sources.
stim = pytest.importorskip("stim")

SAMPLE = Path(__file__).resolve().parents[5] / "samples/notebooks/qdk_ec"


@pytest.fixture(scope="module")
def walkthrough():
    notebook = json.loads(
        (SAMPLE / "qdk_ec_walkthrough.ipynb").read_text(encoding="utf-8")
    )
    sources = [
        "".join(cell["source"])
        for cell in notebook["cells"]
        if cell["cell_type"] == "code"
    ]
    namespace = {"ec": ec, "stim": stim, "display": lambda value: None}
    for prefix in (
        "import qodec",
        "protocol = ec.build_qodec",
        "gadgets = protocol.layers[0].gadgets",
        "protocol = ec.filled(protocol)",
        "reichardt_source =",
        "flagged_preparation_source =",
    ):
        (source,) = [source for source in sources if source.startswith(prefix)]
        exec(compile(source, "qdk_ec_walkthrough.ipynb:build", "exec"), namespace)
    return namespace["protocol"], sources


@pytest.fixture(scope="module")
def simulation():
    with pytest.MonkeyPatch.context() as patch:
        patch.syspath_prepend(str(SAMPLE))
        yield import_module("_walkthrough_simulation")


@pytest.fixture(scope="module")
def spam_programs(walkthrough):
    _, sources = walkthrough
    qsharp.init(target_profile=TargetProfile.Adaptive_RI)
    for source in sources:
        if source.startswith("%%qsharp"):
            qsharp.eval(source.removeprefix("%%qsharp"))
    return {
        basis: str(
            qsharp.compile(f"TestSpam(prepare_{basis}_all, measure_{basis}_all)")
        )
        for basis in ("x", "z")
    }


def test_walkthrough_confidence_configuration_does_not_edit_the_protocol(walkthrough):
    protocol, sources = walkthrough
    original = protocol.dumps()
    (source,) = [
        source for source in sources if source.startswith("max_readout_score =")
    ]
    namespace = {"protocol": protocol}
    exec(compile(source, "qdk_ec_walkthrough.ipynb:confidence", "exec"), namespace)
    assert 0 < namespace["max_readout_score"] < 0.5
    assert protocol.dumps() == original
    assert all("detection_protocol" not in source for source in sources)
    assert {
        name: list(instruction.flags)
        for name, instruction in protocol.layers[0].instruction_set.instructions.items()
    } == {
        "prepare_x_all": ["reject"],
        "prepare_z_all": ["reject"],
        "syndrome": [],
        "measure_x_all": [],
        "measure_z_all": [],
        "cx_all": [],
    }
    assert not ec.audit(protocol).diagnostics


def test_walkthrough_spam_diagram_renders_existing_gadgets_with_qdk(walkthrough):
    pytest.importorskip("qsharp_widgets")
    protocol, _ = walkthrough
    baseline = protocol.dumps()
    notebook = json.loads(
        (SAMPLE / "qdk_ec_walkthrough.ipynb").read_text(encoding="utf-8")
    )
    source = next(
        "".join(cell["source"])
        for cell in notebook["cells"]
        if "".join(cell["source"]).startswith("from qdk import openqasm")
    )
    namespace = {"gadgets": protocol.layers[0].gadgets, "stim": stim}
    exec(compile(source, "qdk_ec_walkthrough.ipynb:spam-circuit", "exec"), namespace)
    diagram = json.loads(namespace["spam_diagram"].json())
    assert len(diagram["qubits"]) == 5
    components = [
        component
        for column in diagram["componentGrid"]
        for component in column["components"]
    ]
    assert sum(component["kind"] == "measurement" for component in components) == 5
    assert sum(bool(component.get("controls")) for component in components) == 5
    widget = namespace["Circuit"](namespace["spam_diagram"])
    assert widget.circuit_json == namespace["spam_diagram"].json()
    widget.close()
    assert protocol.dumps() == baseline


@pytest.mark.parametrize("basis", ["x", "z"])
def test_walkthrough_same_qir_uses_scores_without_adding_flags(
    walkthrough, simulation, spam_programs, monkeypatch, basis
):
    pytest.importorskip("deq")
    pytest.importorskip("deq_runtime")
    from qdk.simulation._qodec.native_batch import CircuitTrace

    protocol, _ = walkthrough
    original = protocol.dumps()
    program = spam_programs[basis]
    original_sample = CircuitTrace.sample

    def sample(trace, shots, noise, *, seed):
        assert trace.num_measurements == 5
        rows = original_sample(trace, shots, None, seed=seed)
        for row in rows:
            row[1] = Result.Zero if row[1] == Result.One else Result.One
        return rows

    monkeypatch.setattr(CircuitTrace, "sample", sample)
    noise = NoiseConfig()
    noise.h.set_depolarizing(0.01)
    noise.cx.set_depolarizing(0.01)
    noise.mresetz.x = 0.01
    options = dict(
        qodec=protocol, noise=noise, shots=4, seed=42, on_shot_failure="discard"
    )
    runtime_factory = partial(simulation._runtime, forced_gap=True)
    assert (
        len(
            run_qir(
                program,
                decoder=partial(prepare_deq_decoder, runtime_factory=runtime_factory),
                **options,
            )
        )
        == 4
    )
    assert (
        run_qir(
            program,
            decoder=partial(
                prepare_deq_decoder,
                runtime_factory=runtime_factory,
                max_readout_score=0.01,
            ),
            **options,
        )
        == []
    )
    assert protocol.dumps() == original


@pytest.mark.parametrize("threshold", [None, 0.01])
def test_walkthrough_confidence_sweep_runs_in_workers(
    walkthrough, simulation, spam_programs, threshold
):
    protocol, _ = walkthrough
    original = protocol.dumps()
    result = simulation.sample_sweep(
        protocol, spam_programs, [0.0], shots=4, workers=2, max_readout_score=threshold
    )
    assert set(result) == {"x", "z"}
    for points in result.values():
        (point,) = points
        probability, counts = point
        assert probability == 0.0
        assert (counts["attempted"], counts["accepted"], counts["wrong"]) == (4, 4, 0)
    assert protocol.dumps() == original
