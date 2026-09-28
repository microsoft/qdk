"""Keep the walkthrough's confidence selection independent of its protocol."""

from collections import Counter
from contextlib import nullcontext
from copy import deepcopy
from functools import partial
from importlib import import_module
import json
from pathlib import Path
from types import SimpleNamespace

import pytest
import pyqir
import stim

from qdk import Result, TargetProfile, ec, qsharp
from qdk.simulation import NoiseConfig, run_qir
from qdk.simulation.decoders import prepare_deq_decoder

SAMPLE = Path(__file__).resolve().parents[5] / "samples/notebooks/qdk_ec"


@pytest.fixture(scope="module")
def walkthrough():
    notebook = json.loads((SAMPLE / "qdk_ec_walkthrough.ipynb").read_text())
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
def benchmark_programs(walkthrough):
    _, sources = walkthrough
    qsharp.init(target_profile=TargetProfile.Adaptive_RI)
    for source in sources:
        if source.startswith("%%qsharp"):
            qsharp.eval(source.removeprefix("%%qsharp"))
    (source,) = [source for source in sources if source.startswith("benchmarks =")]
    namespace = {"qsharp": qsharp}
    exec(compile(source, "qdk_ec_walkthrough.ipynb:benchmarks", "exec"), namespace)
    return namespace["benchmarks"]


@pytest.fixture(scope="module")
def spam_programs(benchmark_programs):
    return {basis: benchmark_programs[f"SPAM {basis.upper()}"] for basis in ("x", "z")}


@pytest.mark.parametrize("basis", ["X", "Z"])
@pytest.mark.parametrize("experiment", ["SPAM", "cx_all", "syndrome"])
def test_walkthrough_experiments_call_only_c4_instructions(
    walkthrough, simulation, benchmark_programs, basis, experiment
):
    protocol, sources = walkthrough
    experiment_sources = [
        source for source in sources if source.startswith("%%qsharp\noperation Test")
    ]
    assert len(experiment_sources) == 3
    assert all(
        "prepare :" not in source and "measure :" not in source
        for source in experiment_sources
    )
    assert set(benchmark_programs) == {
        f"{name} {axis}"
        for name in ("SPAM", "cx_all", "syndrome")
        for axis in ("X", "Z")
    }
    program = benchmark_programs[f"{experiment} {basis}"]
    module = pyqir.Module.from_ir(pyqir.Context(), program)
    calls = [
        instruction.callee.name
        for function in module.functions
        for block in function.basic_blocks
        for instruction in block.instructions
        if isinstance(instruction, pyqir.Call)
        and not instruction.callee.name.startswith("__quantum__rt__")
    ]
    prepare = f"prepare_{basis.lower()}_all"
    measure = f"measure_{basis.lower()}_all"
    expected = {
        "SPAM": [prepare, measure],
        "cx_all": [prepare, prepare, "cx_all", measure, measure],
        "syndrome": [prepare, "syndrome", measure],
    }
    assert calls == expected[experiment]
    results = run_qir(
        program,
        qodec=protocol,
        decoder=partial(
            prepare_deq_decoder,
            runtime_factory=partial(simulation._runtime, forced_gap=True),
            max_readout_score=0.05,
        ),
        shots=4,
        seed=42,
        on_shot_failure="raise",
    )
    blocks = 2 if experiment == "cx_all" else 1
    assert results == [([Result.Zero] * blocks, [Result.Zero] * (2 * blocks))] * 4


def test_walkthrough_sweep_cells_run_without_editing_the_protocol(
    walkthrough, simulation, benchmark_programs, monkeypatch
):
    import matplotlib.pyplot as plt

    protocol, sources = walkthrough
    original = protocol.dumps()
    figures = []

    def show():
        figures.append(plt.gcf())
        plt.close(figures[-1])

    monkeypatch.setattr(plt, "show", show)
    namespace = {
        "protocol": protocol,
        "benchmarks": benchmark_programs,
        "fault_probabilities": [0.001, 0.003],
        "shots_per_point": 4,
        "confidence_shots_per_point": 4,
        "workers_per_sweep": 2,
    }
    for prefix in (
        "import _walkthrough_simulation",
        "simulation.plot_sweep(sweep,",
        "max_score =",
    ):
        (source,) = [source for source in sources if source.startswith(prefix)]
        exec(compile(source, "qdk_ec_walkthrough.ipynb:sweep", "exec"), namespace)
    assert namespace["simulation"] is simulation
    assert 0 < namespace["max_score"] < 0.5
    for name in ("sweep", "confidence_sweep"):
        assert set(namespace[name]) == set(benchmark_programs)
        for points in namespace[name].values():
            assert [probability for probability, _ in points] == [0.001, 0.003]
            for _, counts in points:
                assert counts["attempted"] == 4
                assert 0 <= counts["wrong"] <= counts["accepted"] <= 4
    assert len(figures) == 2
    assert all(len(figure.axes) == 2 for figure in figures)
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


def test_walkthrough_serial_example_calls_run_qir_and_matches_the_helper(
    walkthrough, simulation, benchmark_programs, monkeypatch
):
    import qdk.simulation as qdk_simulation
    from deq import runtime as deq_runtime

    protocol, sources = walkthrough
    original = protocol.dumps()
    (source,) = [source for source in sources if "\nrun_qir(" in source]
    runtime_options = []
    calls = []
    original_runtime = deq_runtime.Runtime

    def runtime(**options):
        runtime_options.append(options)
        return original_runtime(**options)

    def sample(program, **options):
        samples = run_qir(program, **options)
        calls.append((str(program), options, samples))
        return samples

    monkeypatch.setattr(deq_runtime, "Runtime", runtime)
    monkeypatch.setattr(qdk_simulation, "run_qir", sample)
    namespace = {"protocol": protocol, "benchmarks": benchmark_programs}
    exec(compile(source, "qdk_ec_walkthrough.ipynb:serial-samples", "exec"), namespace)
    ((program, options, samples),) = calls
    assert program == benchmark_programs["SPAM Z"]
    assert options["shots"] == 8 and options["seed"] == 42
    assert options["qodec"] is protocol
    assert options["type"] == "stabilizer"
    assert options["on_shot_failure"] == "discard"
    assert runtime_options == [
        {
            "decoder": "black-box-relay-bp",
            "decoder_config": {"parallel": 1, "seed": 42},
            "coordinator": "monolithic",
            "controller": "jit",
        }
    ]
    assert len(samples) == 8
    assert all(len(flags) == 1 and len(logical) == 2 for flags, logical in samples)
    assert all(
        bit in (Result.Zero, Result.One)
        for flags, logical in samples
        for bit in flags + logical
    )
    accepted = [
        logical
        for flags, logical in samples
        if all(bit == Result.Zero for bit in flags)
    ]
    simulation._initialize_worker(protocol.dumps())
    counts = simulation._sample_chunk((program, 0.01, 8, 42, None))
    assert counts["attempted"] == 8
    assert counts["accepted"] == len(accepted)
    assert counts["wrong"] == sum(
        any(bit != Result.Zero for bit in logical) for logical in accepted
    )
    assert protocol.dumps() == original


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


def test_walkthrough_batches_preserve_seeds_and_counts_across_worker_counts(
    walkthrough, simulation, monkeypatch
):
    protocol, _ = walkthrough
    jobs_by_run = []

    def sample_chunks(function, jobs):
        assert function is simulation._sample_chunk
        jobs = list(jobs)
        jobs_by_run.append(jobs)
        return iter(
            Counter(attempted=shots, accepted=shots - seed % 2, wrong=seed % 3)
            for _, _, shots, seed, _ in jobs
        )

    def executor(**options):
        assert options["mp_context"].get_start_method() == "spawn"
        assert options["initializer"] is simulation._initialize_worker
        assert options["initargs"] == (protocol.dumps(),)
        return nullcontext(SimpleNamespace(map=sample_chunks))

    monkeypatch.setattr(simulation, "ProcessPoolExecutor", executor)
    sweeps = [
        simulation.sample_sweep(
            protocol,
            {"SPAM X": "x", "SPAM Z": "z"},
            [0.001, 0.01],
            shots=1003,
            workers=workers,
            max_readout_score=0.05,
        )
        for workers in (1, 4)
    ]
    expected_jobs = [
        ("x", 0.001, 1000, 42, 0.05),
        ("x", 0.001, 3, 46, 0.05),
        ("z", 0.001, 1000, 43, 0.05),
        ("z", 0.001, 3, 47, 0.05),
        ("x", 0.01, 1000, 44, 0.05),
        ("x", 0.01, 3, 48, 0.05),
        ("z", 0.01, 1000, 45, 0.05),
        ("z", 0.01, 3, 49, 0.05),
    ]
    assert jobs_by_run == [expected_jobs, expected_jobs]
    expected = {
        "SPAM X": [
            (0.001, {"attempted": 1003, "accepted": 1003, "wrong": 1}),
            (0.01, {"attempted": 1003, "accepted": 1003, "wrong": 2}),
        ],
        "SPAM Z": [
            (0.001, {"attempted": 1003, "accepted": 1001, "wrong": 3}),
            (0.01, {"attempted": 1003, "accepted": 1001, "wrong": 1}),
        ],
    }
    assert sweeps == [expected, expected]


def test_walkthrough_plot_pools_counts_and_marks_zero_events(simulation, monkeypatch):
    import matplotlib.pyplot as plt
    from matplotlib.axes import Axes
    import numpy as np
    from scipy.stats import binomtest

    sweep = {
        "SPAM Z": [
            (0.001, Counter(attempted=60, accepted=60, wrong=0)),
            (0.01, Counter(attempted=60, accepted=40, wrong=40)),
            (0.1, Counter(attempted=60, accepted=40, wrong=10)),
        ],
        "SPAM X": [
            (0.001, Counter(attempted=40, accepted=40, wrong=0)),
            (0.01, Counter(attempted=40, accepted=30, wrong=30)),
            (0.1, Counter(attempted=40, accepted=30, wrong=4)),
        ],
    }
    original = deepcopy(sweep)
    series = []
    errorbar = Axes.errorbar

    def capture(axis, x, y, **options):
        series.append((x, y, options))
        return errorbar(axis, x, y, **options)

    monkeypatch.setattr(Axes, "errorbar", capture)
    monkeypatch.setattr(plt, "show", lambda: None)
    simulation.plot_sweep(sweep, title="Pooled counts")
    figure = plt.gcf()
    try:
        assert sweep == original
        assert len(series) == 2
        for (x, y, options), counts in zip(
            series,
            ([(0, 100), (70, 70), (14, 70)], [(0, 100), (30, 100), (30, 100)]),
        ):
            np.testing.assert_array_equal(x, [0.001, 0.01, 0.1])
            expected_y, lower_errors, upper_errors = [], [], []
            for events, trials in counts:
                low, high = binomtest(events, trials).proportion_ci(method="wilson")
                rate = events / trials
                expected_y.append(rate if events else high)
                lower_errors.append(rate - low if events else 0)
                upper_errors.append(high - rate if events else 0)
            np.testing.assert_allclose(y, expected_y)
            np.testing.assert_allclose(options["yerr"], [lower_errors, upper_errors])
            np.testing.assert_array_equal(options["uplims"], [True, False, False])
            assert options["label"] == "SPAM"
        assert figure.axes[0].get_ylabel() == "Wrong / accepted"
        assert figure.axes[1].get_ylabel() == "Rejected / attempted"
    finally:
        plt.close(figure)
